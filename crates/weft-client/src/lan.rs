//! Mutually authenticated LAN streams. Payloads remain end-to-end encrypted
//! envelopes and must pass through `Session::receive_frame` for replay checks.
//! This transport never accepts relay control or pairing frames.

use std::collections::BTreeMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinSet;
use tokio::time::timeout;
use weft_cli::state::TrustedDevice;
use weft_protocol::{
    Header, Identity, LAN_AUTH_LEN, LAN_CHALLENGE_LEN, LanAuth, LanChallenge,
    MAX_ENVELOPE_FRAME_LEN, sign_lan_auth, verify_lan_auth,
};

const IO_TIMEOUT: Duration = Duration::from_secs(5);

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid LAN frame")
}

/// A stream bound to a locally trusted peer by a fresh signed challenge.
pub struct Connection {
    stream: TcpStream,
    own: [u8; 16],
    peer: [u8; 16],
}

impl Connection {
    /// Both endpoints call this after connecting/accepting. Trust is supplied
    /// locally, never learned from the remote stream.
    pub async fn authenticate(
        mut stream: TcpStream,
        identity: &Identity,
        trusted: &[TrustedDevice],
    ) -> io::Result<Self> {
        let own = identity.device_id();
        let peer = timeout(IO_TIMEOUT, async {
            let challenge = LanChallenge::generate();
            stream.write_all(&challenge.encode()).await?;
            let mut bytes = [0; LAN_CHALLENGE_LEN];
            stream.read_exact(&mut bytes).await?;
            let remote = LanChallenge::decode(&bytes).map_err(|_| invalid())?;
            if remote.nonce() == challenge.nonce() {
                return Err(invalid());
            }
            let auth = sign_lan_auth(identity, remote.nonce(), challenge.nonce());
            stream.write_all(&auth.encode()).await?;
            let mut bytes = [0; LAN_AUTH_LEN];
            stream.read_exact(&mut bytes).await?;
            let auth = LanAuth::decode(&bytes).map_err(|_| invalid())?;
            let device = trusted
                .iter()
                .find(|d| d.device_id() == auth.device_id)
                .filter(|d| d.device_id() != own)
                .ok_or_else(invalid)?;
            verify_lan_auth(&device.sign_pk, challenge.nonce(), remote.nonce(), &auth)
                .map_err(|_| invalid())?;
            Ok::<_, io::Error>(auth.device_id)
        })
        .await??;
        Ok(Self { stream, own, peer })
    }

    pub fn peer(&self) -> [u8; 16] {
        self.peer
    }

    /// Writes an already sealed envelope. On failure, the caller may send the
    /// same bytes through the relay; the shared replay window rejects duplicates.
    pub async fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        self.check(frame, self.own, self.peer)?;
        timeout(IO_TIMEOUT, async {
            self.stream.write_u32(frame.len() as u32).await?;
            self.stream.write_all(frame).await
        })
        .await?
    }

    pub async fn receive(&mut self) -> io::Result<Vec<u8>> {
        let frame = timeout(IO_TIMEOUT, async {
            let len = self.stream.read_u32().await? as usize;
            if len > MAX_ENVELOPE_FRAME_LEN {
                return Err(invalid());
            }
            let mut frame = vec![0; len];
            self.stream.read_exact(&mut frame).await?;
            Ok::<_, io::Error>(frame)
        })
        .await??;
        self.check(&frame, self.peer, self.own)?;
        Ok(frame)
    }

    fn check(&self, frame: &[u8], sender: [u8; 16], recipient: [u8; 16]) -> io::Result<()> {
        if frame.len() > MAX_ENVELOPE_FRAME_LEN {
            return Err(invalid());
        }
        let prefix = frame.get(..weft_protocol::HEADER_LEN).ok_or_else(invalid)?;
        let header = Header::decode(prefix).map_err(|_| invalid())?;
        if header.sender != sender || header.recipient != recipient {
            return Err(invalid());
        }
        Ok(())
    }
}

enum NetworkEvent {
    Connected(Connection),
    Frame(Vec<u8>),
    DialFailed([u8; 16]),
}

type WriteRequest = (Vec<u8>, oneshot::Sender<bool>);

/// Per-relay-connection LAN routes. Dropping it aborts all listeners, dials and
/// stream tasks. Bounded queues limit memory even if the relay is busy.
pub struct Transport {
    own: [u8; 16],
    identity: Arc<Identity>,
    trusted: Vec<TrustedDevice>,
    events: mpsc::Receiver<NetworkEvent>,
    event_sink: mpsc::Sender<NetworkEvent>,
    routes: BTreeMap<[u8; 16], mpsc::Sender<WriteRequest>>,
    dialing: std::collections::BTreeSet<[u8; 16]>,
    tasks: JoinSet<()>,
    port: u16,
}

impl Transport {
    pub async fn start(identity: Arc<Identity>, trusted: Vec<TrustedDevice>) -> io::Result<Self> {
        let listener = TcpListener::bind("0.0.0.0:0").await?;
        let port = listener.local_addr()?.port();
        let (event_sink, events) = mpsc::channel(16);
        let mut tasks = JoinSet::new();
        let (sink, local, peers) = (event_sink.clone(), identity.clone(), trusted.clone());
        tasks.spawn(async move {
            let mut handshakes = JoinSet::new();
            loop {
                tokio::select! {
                    accepted = listener.accept() => {
                        let Ok((stream, _)) = accepted else { break; };
                        if handshakes.len() >= 10 { continue; }
                        let (sink, local, peers) = (sink.clone(), local.clone(), peers.clone());
                        handshakes.spawn(async move {
                            if let Ok(connection) = Connection::authenticate(stream, &local, &peers).await {
                                let _ = sink.send(NetworkEvent::Connected(connection)).await;
                            }
                        });
                    }
                    _ = handshakes.join_next(), if !handshakes.is_empty() => {}
                }
            }
        });
        Ok(Self {
            own: identity.device_id(),
            identity,
            trusted,
            events,
            event_sink,
            routes: BTreeMap::new(),
            dialing: Default::default(),
            tasks,
            port,
        })
    }

    /// The route selected by the OS, without sending a discovery packet or
    /// exposing a service to public addresses. Multi-interface discovery can
    /// add further private IPs later; a missing route simply uses the relay.
    pub fn endpoints(&self) -> (u16, Vec<String>) {
        let address = std::net::UdpSocket::bind("0.0.0.0:0")
            .ok()
            .and_then(|socket| {
                socket.connect("192.0.2.1:9").ok()?;
                socket.local_addr().ok().map(|addr| addr.ip())
            });
        let addrs = address
            .filter(private_address)
            .map(|a| vec![a.to_string()])
            .unwrap_or_default();
        (self.port, addrs)
    }

    /// Only the smaller identity dials, preventing simultaneous routes.
    /// Addresses come from an authenticated envelope but are still constrained
    /// to numeric private IPs and a bounded list.
    pub fn connect(&mut self, peer: [u8; 16], port: u16, addrs: &[String]) {
        if self.own >= peer
            || port < 1024
            || self.dialing.contains(&peer)
            || self.routes.get(&peer).is_some_and(|s| !s.is_closed())
        {
            return;
        }
        let Some(trusted) = self.trusted.iter().find(|d| d.device_id() == peer).cloned() else {
            return;
        };
        let addresses: Vec<_> = addrs
            .iter()
            .take(4)
            .filter_map(|s| s.parse::<IpAddr>().ok())
            .filter(private_address)
            .map(|a| SocketAddr::new(a, port))
            .collect();
        if addresses.is_empty() {
            return;
        }
        self.dialing.insert(peer);
        let (identity, sink) = (self.identity.clone(), self.event_sink.clone());
        self.tasks.spawn(async move {
            for addr in addresses {
                let Ok(Ok(stream)) = timeout(IO_TIMEOUT, TcpStream::connect(addr)).await else {
                    continue;
                };
                if let Ok(connection) =
                    Connection::authenticate(stream, &identity, std::slice::from_ref(&trusted))
                        .await
                {
                    let _ = sink.send(NetworkEvent::Connected(connection)).await;
                    return;
                }
            }
            let _ = sink.send(NetworkEvent::DialFailed(peer)).await;
        });
    }

    fn install(&mut self, connection: Connection) {
        while self.tasks.try_join_next().is_some() {}
        let peer = connection.peer;
        if self.routes.get(&peer).is_some_and(|s| !s.is_closed()) {
            return;
        }
        self.dialing.remove(&peer);
        let (mut reader, mut writer) = connection.stream.into_split();
        let (writes, mut requests) = mpsc::channel::<WriteRequest>(2);
        self.routes.insert(peer, writes);
        let (sink, own) = (self.event_sink.clone(), self.own);
        self.tasks.spawn(async move {
            let receive = async {
                loop {
                    let len = reader.read_u32().await? as usize;
                    if len > MAX_ENVELOPE_FRAME_LEN {
                        return Err(invalid());
                    }
                    let mut frame = vec![0; len];
                    timeout(IO_TIMEOUT, reader.read_exact(&mut frame)).await??;
                    check_frame(&frame, peer, own)?;
                    if sink.send(NetworkEvent::Frame(frame)).await.is_err() {
                        return Ok::<_, io::Error>(());
                    }
                }
            };
            let send = async {
                while let Some((frame, ack)) = requests.recv().await {
                    let result = timeout(IO_TIMEOUT, async {
                        writer.write_u32(frame.len() as u32).await?;
                        writer.write_all(&frame).await
                    })
                    .await;
                    let success = matches!(result, Ok(Ok(())));
                    let _ = ack.send(success);
                    if !success {
                        break;
                    }
                }
            };
            tokio::select! { _ = receive => {}, _ = send => {} }
        });
    }

    /// Returns only frames from authenticated streams, ready for the common
    /// decryption/replay path. Channel receive is cancellation safe.
    pub async fn receive(&mut self) -> Option<Vec<u8>> {
        loop {
            match self.events.recv().await? {
                NetworkEvent::Connected(connection) => self.install(connection),
                NetworkEvent::Frame(frame) => return Some(frame),
                NetworkEvent::DialFailed(peer) => {
                    self.dialing.remove(&peer);
                }
            }
        }
    }

    pub async fn send(&mut self, peer: [u8; 16], frame: &[u8]) -> bool {
        while self.tasks.try_join_next().is_some() {}
        if check_frame(frame, self.own, peer).is_err() {
            return false;
        }
        let Some(route) = self.routes.get(&peer) else {
            return false;
        };
        let (ack, result) = oneshot::channel();
        if route.try_send((frame.to_vec(), ack)).is_err() {
            return false;
        }
        matches!(timeout(IO_TIMEOUT, result).await, Ok(Ok(true)))
    }
}

fn private_address(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback(),
        IpAddr::V6(_) => false,
    }
}

fn check_frame(frame: &[u8], sender: [u8; 16], recipient: [u8; 16]) -> io::Result<()> {
    if frame.len() > MAX_ENVELOPE_FRAME_LEN {
        return Err(invalid());
    }
    let prefix = frame.get(..weft_protocol::HEADER_LEN).ok_or_else(invalid)?;
    let header = Header::decode(prefix).map_err(|_| invalid())?;
    if header.sender != sender || header.recipient != recipient {
        return Err(invalid());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use weft_protocol::{Kind, VERSION, random_message_id, seal, session_keys};

    fn trusted(identity: &Identity) -> TrustedDevice {
        TrustedDevice {
            sign_pk: identity.sign_pk(),
            kx_pk: identity.kx_pk(),
            name: None,
        }
    }

    #[tokio::test]
    async fn managed_routes_exchange_frames_and_report_failure_for_relay_fallback() {
        let mut a = Arc::new(Identity::generate());
        let mut b = Arc::new(Identity::generate());
        if a.device_id() > b.device_id() {
            std::mem::swap(&mut a, &mut b);
        }
        let mut left = Transport::start(a.clone(), vec![trusted(&b)])
            .await
            .unwrap();
        let mut right = Transport::start(b.clone(), vec![trusted(&a)])
            .await
            .unwrap();
        left.connect(b.device_id(), right.port, &["127.0.0.1".into()]);
        timeout(IO_TIMEOUT, async {
            loop {
                if left.routes.contains_key(&b.device_id())
                    && right.routes.contains_key(&a.device_id())
                {
                    break;
                }
                tokio::select! {
                    _ = left.receive() => {},
                    _ = right.receive() => {},
                    _ = tokio::time::sleep(Duration::from_millis(10)) => {},
                }
            }
        })
        .await
        .unwrap();
        let header = Header {
            version: VERSION,
            kind: Kind::Packet,
            sender: a.device_id(),
            recipient: b.device_id(),
            message_id: random_message_id(),
            counter: 1,
        };
        let keys = session_keys(&a, &b.device_id(), &b.kx_pk()).unwrap();
        let frame = seal(&header, b"{}", &keys.tx).unwrap();
        assert!(left.send(b.device_id(), &frame).await);
        assert_eq!(
            timeout(IO_TIMEOUT, right.receive()).await.unwrap().unwrap(),
            frame
        );
        drop(right);
        timeout(IO_TIMEOUT, async {
            while !left.routes.get(&b.device_id()).unwrap().is_closed() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
        assert!(!left.send(b.device_id(), &frame).await);
    }

    #[tokio::test]
    async fn trusted_peers_exchange_sealed_envelopes_and_bind_routing() {
        let a = Identity::generate();
        let b = Identity::generate();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let accept = async {
            let (stream, _) = listener.accept().await.unwrap();
            Connection::authenticate(stream, &b, &[trusted(&a)])
                .await
                .unwrap()
        };
        let connect = async {
            let stream = TcpStream::connect(address).await.unwrap();
            Connection::authenticate(stream, &a, &[trusted(&b)])
                .await
                .unwrap()
        };
        let (mut receiver, mut sender) = tokio::join!(accept, connect);
        let header = Header {
            version: VERSION,
            kind: Kind::ClipboardText,
            sender: a.device_id(),
            recipient: b.device_id(),
            message_id: random_message_id(),
            counter: 1,
        };
        let keys = session_keys(&a, &b.device_id(), &b.kx_pk()).unwrap();
        let frame = seal(&header, b"test", &keys.tx).unwrap();
        sender.send(&frame).await.unwrap();
        assert_eq!(receiver.receive().await.unwrap(), frame);
        assert!(receiver.send(&frame).await.is_err());
        assert!(sender.send(&[0; 10]).await.is_err());
    }

    #[tokio::test]
    async fn unknown_peer_is_rejected() {
        let a = Identity::generate();
        let b = Identity::generate();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let accept = async {
            let (stream, _) = listener.accept().await.unwrap();
            Connection::authenticate(stream, &b, &[]).await
        };
        let connect = async {
            Connection::authenticate(
                TcpStream::connect(address).await.unwrap(),
                &a,
                &[trusted(&b)],
            )
            .await
        };
        let (result, _) = tokio::join!(accept, connect);
        assert!(result.is_err());
    }
}
