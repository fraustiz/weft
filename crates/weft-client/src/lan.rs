//! Mutually authenticated LAN streams (weft-lan-v3). Payloads remain
//! end-to-end encrypted envelopes and must pass through
//! `Session::receive_frame` for replay checks. Receipts carry a MAC under a
//! key derived from the pairwise session keys and this connection's nonces
//! (see `LanReceiptKeys`), so an on-path relay of the handshake cannot forge
//! them. This transport never accepts relay control or pairing frames.

use std::collections::BTreeMap;
use std::io;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::{Mutex, mpsc, oneshot};
use tokio::task::JoinSet;
use tokio::time::timeout;
use weft_cli::state::TrustedDevice;
use weft_protocol::{
    HEADER_LEN, Header, Identity, Kind, LAN_AUTH_LEN, LAN_CHALLENGE_LEN, LAN_RECEIPT_MAC_LEN,
    LanAuth, LanChallenge, LanReceiptKey, LanReceiptKeys, MAX_ENVELOPE_FRAME_LEN, NONCE_LEN,
    TAG_LEN, sign_lan_auth, verify_lan_auth,
};

const IO_TIMEOUT: Duration = Duration::from_secs(5);
const MIN_FRAME_LEN: usize = HEADER_LEN + NONCE_LEN + TAG_LEN;

fn invalid() -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, "invalid LAN frame")
}

/// A stream bound to a locally trusted peer by a fresh signed challenge.
pub struct Connection {
    stream: TcpStream,
    own: [u8; 16],
    peer: [u8; 16],
    keys: LanReceiptKeys,
}

impl Connection {
    /// Both endpoints call this after connecting/accepting. Trust is supplied
    /// locally, never learned from the remote stream.
    pub async fn authenticate(
        mut stream: TcpStream,
        identity: &Identity,
        trusted: &[TrustedDevice],
    ) -> io::Result<Self> {
        stream.set_nodelay(true)?;
        let own = identity.device_id();
        let (peer, keys) = timeout(IO_TIMEOUT, async {
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
            let keys = LanReceiptKeys::derive(
                identity,
                &auth.device_id,
                &device.kx_pk,
                challenge.nonce(),
                remote.nonce(),
            )
            .map_err(|_| invalid())?;
            Ok::<_, io::Error>((auth.device_id, keys))
        })
        .await??;
        Ok(Self {
            stream,
            own,
            peer,
            keys,
        })
    }

    pub fn peer(&self) -> [u8; 16] {
        self.peer
    }

    /// Exchanges a single sealed envelope and its receipt on an otherwise
    /// idle connection. The managed transport supports simultaneous sends.
    pub async fn send(&mut self, frame: &[u8]) -> io::Result<()> {
        let id = check_frame(frame, self.own, self.peer)?;
        timeout(IO_TIMEOUT, async {
            write_frame(&mut self.stream, frame).await?;
            match read_record(&mut self.stream).await? {
                Record::Receipt(received, mac)
                    if received == id && self.keys.outgoing.verify(&id, &mac) =>
                {
                    Ok(())
                }
                _ => Err(invalid()),
            }
        })
        .await?
    }

    /// Reads a complete envelope without acknowledging it.
    pub async fn receive(&mut self) -> io::Result<Vec<u8>> {
        match read_record(&mut self.stream).await? {
            Record::Frame(frame) => {
                check_frame(&frame, self.peer, self.own)?;
                Ok(frame)
            }
            Record::Receipt(..) => Err(invalid()),
        }
    }

    /// Sends a receipt after the receiver has accepted a complete envelope.
    pub async fn acknowledge(&mut self, frame: &[u8]) -> io::Result<()> {
        let id = check_frame(frame, self.peer, self.own)?;
        let receipt = receipt_record(&self.keys.incoming, id);
        timeout(IO_TIMEOUT, self.stream.write_all(&receipt)).await?
    }
}

type MessageId = [u8; 16];

type ReceiptMac = [u8; LAN_RECEIPT_MAC_LEN];

// LAN v3 records use a big-endian u32. A positive length is followed by the
// sealed envelope, unchanged. Zero is followed by the 16-byte message ID of
// an accepted envelope and its 16-byte MAC under the receipt key of that
// direction. Each direction has at most one send awaiting receipt.
enum Record {
    Frame(Vec<u8>),
    Receipt(MessageId, ReceiptMac),
}

async fn read_record(reader: &mut (impl AsyncRead + Unpin)) -> io::Result<Record> {
    // Allow idle routes, but bound the rest of a record once it starts.
    let mut prefix = [0; 4];
    prefix[0] = reader.read_u8().await?;
    timeout(IO_TIMEOUT, async {
        reader.read_exact(&mut prefix[1..]).await?;
        let len = u32::from_be_bytes(prefix) as usize;
        if len == 0 {
            let mut receipt = [0; 16 + LAN_RECEIPT_MAC_LEN];
            reader.read_exact(&mut receipt).await?;
            let (id, mac) = receipt.split_at(16);
            return Ok(Record::Receipt(
                id.try_into().unwrap(),
                mac.try_into().unwrap(),
            ));
        }
        if !(MIN_FRAME_LEN..=MAX_ENVELOPE_FRAME_LEN).contains(&len) {
            return Err(invalid());
        }
        let mut frame = vec![0; len];
        reader.read_exact(&mut frame).await?;
        Ok(Record::Frame(frame))
    })
    .await?
}

async fn write_frame(writer: &mut (impl AsyncWrite + Unpin), frame: &[u8]) -> io::Result<()> {
    writer.write_u32(frame.len() as u32).await?;
    writer.write_all(frame).await
}

/// `u32 0 || message_id || mac`, for an envelope the peer sent us.
fn receipt_record(key: &LanReceiptKey, id: MessageId) -> [u8; 4 + 16 + LAN_RECEIPT_MAC_LEN] {
    let mut receipt = [0; 4 + 16 + LAN_RECEIPT_MAC_LEN];
    receipt[4..20].copy_from_slice(&id);
    receipt[20..].copy_from_slice(&key.mac(&id));
    receipt
}

enum NetworkEvent {
    Connected(Connection),
    DialFailed([u8; 16]),
}

type WriteRequest = (Vec<u8>, MessageId, oneshot::Sender<()>);

/// Per-relay-connection LAN routes. Dropping it aborts all listeners, dials and
/// stream tasks. Bounded queues limit memory even if the relay is busy.
pub struct Transport {
    own: [u8; 16],
    identity: Arc<Identity>,
    trusted: Vec<TrustedDevice>,
    events: mpsc::Receiver<NetworkEvent>,
    event_sink: mpsc::Sender<NetworkEvent>,
    incoming: mpsc::Sender<Vec<u8>>,
    routes: BTreeMap<[u8; 16], mpsc::Sender<WriteRequest>>,
    dialing: std::collections::BTreeSet<[u8; 16]>,
    tasks: JoinSet<()>,
    port: u16,
}

impl Transport {
    pub async fn start(
        identity: Arc<Identity>,
        trusted: Vec<TrustedDevice>,
        incoming: mpsc::Sender<Vec<u8>>,
    ) -> io::Result<Self> {
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
            incoming,
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
        let keys = connection.keys;
        let (mut reader, writer) = connection.stream.into_split();
        let writer = Mutex::new(writer);
        let (writes, mut requests) = mpsc::channel::<WriteRequest>(1);
        let (receipts, mut received) = mpsc::channel(1);
        self.routes.insert(peer, writes);
        let (incoming, own) = (self.incoming.clone(), self.own);
        self.tasks.spawn(async move {
            let receive = async {
                while let Ok(record) = read_record(&mut reader).await {
                    match record {
                        // A receipt that does not verify ends the route: the pending
                        // frame then goes through the relay with the same bytes.
                        Record::Receipt(id, mac) => {
                            if !keys.outgoing.verify(&id, &mac) {
                                return Err(invalid());
                            }
                            receipts.try_send(id).map_err(|_| invalid())?
                        }
                        Record::Frame(frame) => {
                            let id = check_frame(&frame, peer, own)?;
                            // The worker might itself be waiting for a receipt. A full queue
                            // ends the route so this unaccepted frame can go through the relay.
                            incoming.try_send(frame).map_err(|_| invalid())?;
                            let receipt = receipt_record(&keys.incoming, id);
                            timeout(IO_TIMEOUT, async {
                                writer.lock().await.write_all(&receipt).await
                            })
                            .await??;
                        }
                    }
                }
                Ok::<_, io::Error>(())
            };
            let send = async {
                while let Some((frame, id, confirmed)) = requests.recv().await {
                    timeout(IO_TIMEOUT, async {
                        write_frame(&mut *writer.lock().await, &frame).await?;
                        if received.recv().await != Some(id) {
                            return Err(invalid());
                        }
                        Ok(())
                    })
                    .await??;
                    let _ = confirmed.send(());
                }
                Ok::<_, io::Error>(())
            };
            // Either direction failing closes the socket and drops the in-flight receipt waiter.
            tokio::select! { _ = receive => {}, _ = send => {} }
        });
    }

    /// Installs newly authenticated routes while the worker waits for other
    /// input. The event receive is cancellation safe.
    pub async fn maintain(&mut self) {
        while let Some(event) = self.events.recv().await {
            match event {
                NetworkEvent::Connected(connection) => self.install(connection),
                NetworkEvent::DialFailed(peer) => {
                    self.dialing.remove(&peer);
                }
            }
        }
    }

    /// Whether an authenticated stream to `peer` is open, for logs.
    pub fn has_route(&self, peer: &[u8; 16]) -> bool {
        self.routes
            .get(peer)
            .is_some_and(|route| !route.is_closed())
    }

    /// Returns true only after the peer accepts this envelope. The worker
    /// sends serially, so an unconfirmed frame falls back before the next send.
    pub async fn send(&mut self, peer: [u8; 16], frame: &[u8]) -> bool {
        while self.tasks.try_join_next().is_some() {}
        let Ok(id) = check_frame(frame, self.own, peer) else {
            return false;
        };
        let Some(route) = self.routes.get(&peer) else {
            return false;
        };
        let (confirmed, result) = oneshot::channel();
        route.try_send((frame.to_vec(), id, confirmed)).is_ok() && result.await.is_ok()
    }
}

fn private_address(ip: &IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_loopback(),
        IpAddr::V6(_) => false,
    }
}

fn check_frame(frame: &[u8], sender: [u8; 16], recipient: [u8; 16]) -> io::Result<MessageId> {
    if !(MIN_FRAME_LEN..=MAX_ENVELOPE_FRAME_LEN).contains(&frame.len()) {
        return Err(invalid());
    }
    let header = Header::decode(&frame[..HEADER_LEN]).map_err(|_| invalid())?;
    if header.sender != sender
        || header.recipient != recipient
        || !matches!(
            header.kind,
            Kind::ClipboardText | Kind::Packet | Kind::FileChunk
        )
    {
        return Err(invalid());
    }
    Ok(header.message_id)
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

    fn sealed_packet(from: &Identity, to: &Identity, counter: u64) -> Vec<u8> {
        let header = Header {
            version: VERSION,
            kind: Kind::Packet,
            sender: from.device_id(),
            recipient: to.device_id(),
            message_id: random_message_id(),
            counter,
        };
        let keys = session_keys(from, &to.device_id(), &to.kx_pk()).unwrap();
        seal(
            &header,
            &vec![b'x'; weft_protocol::MAX_PACKET_LEN],
            &keys.tx,
        )
        .unwrap()
    }

    #[tokio::test]
    async fn simultaneous_sends_succeed_and_backpressure_refuses_new_frames() {
        let mut a = Arc::new(Identity::generate());
        let mut b = Arc::new(Identity::generate());
        if a.device_id() > b.device_id() {
            std::mem::swap(&mut a, &mut b);
        }
        let (to_left, mut left_frames) = mpsc::channel(1);
        let (to_right, mut right_frames) = mpsc::channel(1);
        let mut left = Transport::start(a.clone(), vec![trusted(&b)], to_left)
            .await
            .unwrap();
        let mut right = Transport::start(b.clone(), vec![trusted(&a)], to_right)
            .await
            .unwrap();
        left.connect(b.device_id(), right.port, &["127.0.0.1".into()]);
        timeout(IO_TIMEOUT, async {
            for transport in [&mut left, &mut right] {
                let NetworkEvent::Connected(connection) = transport.events.recv().await.unwrap()
                else {
                    panic!("missing LAN connection");
                };
                transport.install(connection);
            }
        })
        .await
        .unwrap();

        let first = sealed_packet(&a, &b, 1);
        let reverse = sealed_packet(&b, &a, 1);
        assert_eq!(
            tokio::join!(
                left.send(b.device_id(), &first),
                right.send(a.device_id(), &reverse),
            ),
            (true, true)
        );
        assert_eq!(left_frames.try_recv().unwrap(), reverse);

        // The right worker hasn't consumed its first frame yet. The second
        // is refused rather than acknowledged or blocking the stream reader.
        let next = sealed_packet(&a, &b, 2);
        assert!(!left.send(b.device_id(), &next).await);
        assert_eq!(right_frames.try_recv().unwrap(), first);
        assert!(right_frames.try_recv().is_err());
        assert!(!left.has_route(&b.device_id()));
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
        let (sent, ()) = tokio::join!(sender.send(&frame), async {
            let received = receiver.receive().await.unwrap();
            assert_eq!(received, frame);
            receiver.acknowledge(&received).await.unwrap();
        });
        sent.unwrap();
        assert!(receiver.send(&frame).await.is_err());
        assert!(sender.send(&[0; 10]).await.is_err());
    }

    /// An authenticated connection from `a` (dialing) to `b` (accepting).
    async fn connected(a: &Identity, b: &Identity) -> (Connection, Connection) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let accept = async {
            let (stream, _) = listener.accept().await.unwrap();
            Connection::authenticate(stream, b, &[trusted(a)])
                .await
                .unwrap()
        };
        let connect = async {
            let stream = TcpStream::connect(address).await.unwrap();
            Connection::authenticate(stream, a, &[trusted(b)])
                .await
                .unwrap()
        };
        let (receiver, sender) = tokio::join!(accept, connect);
        (sender, receiver)
    }

    fn clipboard(from: &Identity, to: &Identity) -> Vec<u8> {
        let header = Header {
            version: VERSION,
            kind: Kind::ClipboardText,
            sender: from.device_id(),
            recipient: to.device_id(),
            message_id: random_message_id(),
            counter: 1,
        };
        let keys = session_keys(from, &to.device_id(), &to.kx_pk()).unwrap();
        seal(&header, b"test", &keys.tx).unwrap()
    }

    fn message_id(frame: &[u8]) -> MessageId {
        Header::decode(&frame[..HEADER_LEN]).unwrap().message_id
    }

    /// The receiver reads the frame, then `reply` writes whatever receipt
    /// bytes an attacker (or a confused peer) would. Returns the send result.
    async fn send_with_receipt(
        sender: &mut Connection,
        receiver: &mut Connection,
        frame: &[u8],
        reply: impl AsyncFnOnce(&mut Connection, MessageId),
    ) -> io::Result<()> {
        let (sent, ()) = tokio::join!(sender.send(frame), async {
            let received = receiver.receive().await.unwrap();
            reply(receiver, message_id(&received)).await;
        });
        sent
    }

    #[tokio::test]
    async fn a_receipt_needs_the_mac_of_this_connection_and_direction() {
        let a = Identity::generate();
        let b = Identity::generate();
        let (mut sender, mut receiver) = connected(&a, &b).await;
        let (_, other) = connected(&a, &b).await;
        // Right id, then a wrong MAC, the MAC of the other direction, the MAC
        // another connection between the same devices would carry, and no MAC.
        type Forge = fn(&Connection, &Connection, MessageId) -> Vec<u8>;
        let forgeries: [Forge; 4] = [
            |_, _, id| {
                receipt_record(
                    &LanReceiptKey::derive(&[9; 32], &[1; 16], &[2; 16], &[3; 32], &[4; 32]),
                    id,
                )
                .to_vec()
            },
            |receiver, _, id| receipt_record(&receiver.keys.outgoing, id).to_vec(),
            |_, other, id| receipt_record(&other.keys.incoming, id).to_vec(),
            |_, _, id| [[0; 4].as_slice(), &id].concat(),
        ];
        for forge in forgeries {
            let frame = clipboard(&a, &b);
            let other = &other;
            let result =
                send_with_receipt(&mut sender, &mut receiver, &frame, async |receiver, id| {
                    let bytes = forge(receiver, other, id);
                    receiver.stream.write_all(&bytes).await.unwrap();
                    // A short receipt must not leave the sender waiting for the
                    // next record: the link closes, as an attacker could do.
                    receiver.stream.shutdown().await.unwrap();
                })
                .await;
            assert!(result.is_err());
            (sender, receiver) = connected(&a, &b).await;
        }

        // The genuine receipt still works on a fresh connection.
        let frame = clipboard(&a, &b);
        send_with_receipt(&mut sender, &mut receiver, &frame, async |receiver, id| {
            let receipt = receipt_record(&receiver.keys.incoming, id);
            receiver.stream.write_all(&receipt).await.unwrap();
        })
        .await
        .unwrap();
        // And a replay of it, for the next frame, is refused.
        let replayed = {
            let next = clipboard(&a, &b);
            let previous = message_id(&frame);
            send_with_receipt(
                &mut sender,
                &mut receiver,
                &next,
                async move |receiver, _| {
                    let receipt = receipt_record(&receiver.keys.incoming, previous);
                    receiver.stream.write_all(&receipt).await.unwrap();
                },
            )
            .await
        };
        assert!(replayed.is_err());
    }

    #[tokio::test]
    async fn a_forged_receipt_drops_the_route_and_fails_the_send() {
        let mut a = Arc::new(Identity::generate());
        let mut b = Arc::new(Identity::generate());
        if a.device_id() > b.device_id() {
            std::mem::swap(&mut a, &mut b);
        }
        // The transport of B accepts; the test plays A, which holds the right
        // keys, but answers with a receipt whose MAC is wrong.
        let (to_b, _b_frames) = mpsc::channel(1);
        let mut transport = Transport::start(b.clone(), vec![trusted(&a)], to_b)
            .await
            .unwrap();
        let stream = TcpStream::connect(("127.0.0.1", transport.port))
            .await
            .unwrap();
        let devices = [trusted(&b)];
        let (peer, event) = tokio::join!(
            Connection::authenticate(stream, &a, &devices),
            transport.events.recv()
        );
        let mut peer = peer.unwrap();
        let Some(NetworkEvent::Connected(connection)) = event else {
            panic!("missing LAN connection");
        };
        transport.install(connection);
        assert!(transport.has_route(&a.device_id()));

        let frame = sealed_packet(&b, &a, 1);
        let (sent, ()) = tokio::join!(transport.send(a.device_id(), &frame), async {
            let received = peer.receive().await.unwrap();
            assert_eq!(received, frame);
            let mut receipt = receipt_record(&peer.keys.incoming, message_id(&received));
            receipt[35] ^= 1;
            peer.stream.write_all(&receipt).await.unwrap();
        });
        // False makes the worker relay the same bytes.
        assert!(!sent);
        timeout(IO_TIMEOUT, async {
            while transport.has_route(&a.device_id()) {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }

    #[tokio::test]
    async fn a_v2_peer_fails_the_handshake_at_once() {
        let a = Identity::generate();
        let b = Identity::generate();
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let accept = async {
            let (stream, _) = listener.accept().await.unwrap();
            Connection::authenticate(stream, &b, &[trusted(&a)]).await
        };
        // A v2 peer sends `0xC0 || nonce` and then waits for ours.
        let v2 = async {
            let mut stream = TcpStream::connect(address).await.unwrap();
            let mut challenge = [0xC0; LAN_CHALLENGE_LEN];
            challenge[1..].copy_from_slice(&[5; 32]);
            stream.write_all(&challenge).await.unwrap();
            let mut ours = [0; LAN_CHALLENGE_LEN];
            stream.read_exact(&mut ours).await.unwrap();
            // A v2 client rejects the v3 type byte and closes, as Rust and
            // Kotlin v2 did.
            assert_eq!(ours[0], weft_protocol::LAN_CHALLENGE);
            assert_ne!(ours[0], 0xC0);
        };
        let started = std::time::Instant::now();
        let (result, ()) = tokio::join!(accept, v2);
        assert!(result.is_err());
        assert!(started.elapsed() < Duration::from_secs(1));
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
