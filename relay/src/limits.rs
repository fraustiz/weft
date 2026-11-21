//! Caps on open WebSocket connections, for the whole relay and per client
//! address, and how the client address is found.

use std::collections::HashMap;
use std::net::{IpAddr, Ipv6Addr, SocketAddr};
use std::pin::pin;
use std::sync::{Arc, Mutex};

use axum::http::HeaderMap;
use tokio::sync::Notify;

/// Why a connection was refused.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Refusal {
    /// The relay holds its maximum number of connections.
    Full,
    /// The client address holds its maximum number of connections.
    TooManyFromAddress,
}

/// Counts open connections. A connection holds a [`ConnectionSlot`] for as
/// long as it lives.
pub(crate) struct Limiter {
    max_total: usize,
    max_per_ip: usize,
    counts: Mutex<Counts>,
    /// Woken when the last connection closes.
    drained: Notify,
}

#[derive(Default)]
struct Counts {
    total: usize,
    per_ip: HashMap<IpAddr, usize>,
}

impl Limiter {
    pub(crate) fn new(max_total: usize, max_per_ip: usize) -> Limiter {
        Limiter {
            max_total,
            max_per_ip,
            counts: Mutex::new(Counts::default()),
            drained: Notify::new(),
        }
    }

    /// Takes a slot for a new connection from `ip`, if the limits allow it.
    pub(crate) fn acquire(self: &Arc<Self>, ip: IpAddr) -> Result<ConnectionSlot, Refusal> {
        let key = limit_key(ip);
        let mut counts = self.counts.lock().unwrap();
        if counts.total >= self.max_total {
            return Err(Refusal::Full);
        }
        let from_ip = counts.per_ip.entry(key).or_insert(0);
        if *from_ip >= self.max_per_ip {
            return Err(Refusal::TooManyFromAddress);
        }
        *from_ip += 1;
        counts.total += 1;
        Ok(ConnectionSlot {
            limiter: self.clone(),
            key,
        })
    }

    /// Waits until every slot has been released.
    pub(crate) async fn drained(&self) {
        loop {
            let mut notified = pin!(self.drained.notified());
            // Register before checking, so a release in between is not missed.
            notified.as_mut().enable();
            if self.counts.lock().unwrap().total == 0 {
                return;
            }
            notified.await;
        }
    }
}

/// One open connection; dropping it frees the slot.
pub(crate) struct ConnectionSlot {
    limiter: Arc<Limiter>,
    key: IpAddr,
}

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        let mut counts = self.limiter.counts.lock().unwrap();
        counts.total -= 1;
        if let Some(from_ip) = counts.per_ip.get_mut(&self.key) {
            *from_ip -= 1;
            if *from_ip == 0 {
                counts.per_ip.remove(&self.key);
            }
        }
        if counts.total == 0 {
            self.limiter.drained.notify_waiters();
        }
    }
}

/// The client address of a request.
///
/// With `trust_proxy` off, it is always the TCP peer. With it on, forwarded
/// headers are read, but **only when the TCP peer is a trusted proxy**: if
/// `trusted_proxies` is non-empty and the peer is not in it, the headers are
/// ignored and the peer is used. This stops a neighbour on a shared network
/// (e.g. the Docker overlay) from forging a client address.
///
/// When trusted, the lookup order is:
/// 1. `client_ip_header` (e.g. `cf-connecting-ip`), a single address set by
///    the edge and overwritten there, so a client cannot spoof it;
/// 2. otherwise the last `X-Forwarded-For` entry, which the nearest proxy
///    appended. Earlier entries come from the client and are never trusted.
pub(crate) fn client_ip(
    peer: SocketAddr,
    headers: &HeaderMap,
    trust_proxy: bool,
    client_ip_header: Option<&str>,
    trusted_proxies: &[IpAddr],
) -> IpAddr {
    if trust_proxy && peer_is_trusted(peer.ip(), trusted_proxies) {
        if let Some(ip) = client_ip_header.and_then(|name| single_ip_header(headers, name)) {
            return ip;
        }
        if let Some(ip) = last_forwarded_for(headers) {
            return ip;
        }
    }
    peer.ip()
}

/// Whether forwarded headers from `peer` may be trusted. An empty allowlist
/// trusts any peer (the relay must then be unreachable except through its
/// proxy); a non-empty one trusts only its addresses.
fn peer_is_trusted(peer: IpAddr, trusted_proxies: &[IpAddr]) -> bool {
    trusted_proxies.is_empty() || trusted_proxies.contains(&peer)
}

/// A single IP address carried by `name`, e.g. `CF-Connecting-IP`.
fn single_ip_header(headers: &HeaderMap, name: &str) -> Option<IpAddr> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.trim().parse::<IpAddr>().ok())
}

/// The last entry of `X-Forwarded-For`, which the nearest proxy appended.
fn last_forwarded_for(headers: &HeaderMap) -> Option<IpAddr> {
    headers
        .get_all("x-forwarded-for")
        .iter()
        .next_back()
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.rsplit(',').next())
        .and_then(|last| last.trim().parse::<IpAddr>().ok())
}

/// The address a connection is counted under. An IPv6 client usually holds
/// a whole /64, so its connections are counted per /64; IPv4-mapped IPv6
/// addresses count as their IPv4 address.
fn limit_key(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V4(_) => ip,
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => {
                let prefix = u128::from(v6) & !((1u128 << 64) - 1);
                IpAddr::V6(Ipv6Addr::from(prefix))
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(values: &[&str]) -> HeaderMap {
        let mut headers = HeaderMap::new();
        for value in values {
            headers.append("x-forwarded-for", value.parse().unwrap());
        }
        headers
    }

    const PEER: &str = "10.0.0.9:4000";

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn forwarded_for_is_ignored_without_trust() {
        let got = client_ip(
            PEER.parse().unwrap(),
            &headers(&["1.2.3.4"]),
            false,
            None,
            &[],
        );
        assert_eq!(got, ip("10.0.0.9"));
    }

    #[test]
    fn last_forwarded_address_is_used_with_trust() {
        let headers = headers(&["6.6.6.6, 7.7.7.7", "1.2.3.4 , 5.6.7.8"]);
        let got = client_ip(PEER.parse().unwrap(), &headers, true, None, &[]);
        assert_eq!(got, ip("5.6.7.8"));
    }

    #[test]
    fn invalid_or_missing_forwarded_for_falls_back_to_peer() {
        let peer = PEER.parse().unwrap();
        assert_eq!(
            client_ip(peer, &headers(&[]), true, None, &[]),
            ip("10.0.0.9")
        );
        assert_eq!(
            client_ip(peer, &headers(&["1.2.3.4, junk"]), true, None, &[]),
            ip("10.0.0.9")
        );
    }

    fn with_header(name: &str, value: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.append(
            axum::http::HeaderName::from_bytes(name.as_bytes()).unwrap(),
            value.parse().unwrap(),
        );
        headers
    }

    #[test]
    fn client_ip_header_is_preferred_over_forwarded_for() {
        let mut h = headers(&["9.9.9.9"]);
        h.append("cf-connecting-ip", "203.0.113.7".parse().unwrap());
        let got = client_ip(
            PEER.parse().unwrap(),
            &h,
            true,
            Some("cf-connecting-ip"),
            &[],
        );
        assert_eq!(got, ip("203.0.113.7"));
    }

    #[test]
    fn falls_back_to_forwarded_for_when_client_ip_header_absent() {
        let got = client_ip(
            PEER.parse().unwrap(),
            &with_header("x-forwarded-for", "203.0.113.8"),
            true,
            Some("cf-connecting-ip"),
            &[],
        );
        assert_eq!(got, ip("203.0.113.8"));
    }

    #[test]
    fn forwarded_headers_ignored_from_an_untrusted_peer() {
        // The peer 10.0.0.9 is not the trusted proxy, so its forwarded header
        // is ignored and it is counted under its own address.
        let got = client_ip(
            PEER.parse().unwrap(),
            &with_header("cf-connecting-ip", "203.0.113.9"),
            true,
            Some("cf-connecting-ip"),
            &[ip("10.0.0.1")],
        );
        assert_eq!(got, ip("10.0.0.9"));
    }

    #[test]
    fn forwarded_headers_trusted_only_from_a_listed_proxy() {
        let got = client_ip(
            PEER.parse().unwrap(),
            &with_header("cf-connecting-ip", "203.0.113.10"),
            true,
            Some("cf-connecting-ip"),
            &[ip("10.0.0.9")],
        );
        assert_eq!(got, ip("203.0.113.10"));
    }

    #[test]
    fn ipv6_counts_per_slash_64() {
        let a = limit_key("2001:db8:1:2:aaaa::1".parse().unwrap());
        let b = limit_key("2001:db8:1:2:bbbb::2".parse().unwrap());
        let c = limit_key("2001:db8:1:3::1".parse().unwrap());
        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_eq!(
            limit_key("::ffff:1.2.3.4".parse().unwrap()),
            "1.2.3.4".parse::<IpAddr>().unwrap()
        );
    }

    #[test]
    fn slots_are_counted_and_released() {
        let limiter = Arc::new(Limiter::new(3, 2));
        let ip: IpAddr = "1.2.3.4".parse().unwrap();
        let other: IpAddr = "1.2.3.5".parse().unwrap();
        let first = limiter.acquire(ip).unwrap();
        let _second = limiter.acquire(ip).unwrap();
        assert_eq!(limiter.acquire(ip).err(), Some(Refusal::TooManyFromAddress));
        let _third = limiter.acquire(other).unwrap();
        assert_eq!(limiter.acquire(other).err(), Some(Refusal::Full));
        drop(first);
        assert!(limiter.acquire(ip).is_ok());
    }
}
