//! End-to-end tests of what keeps a public relay healthy: connection limits,
//! idle timeout and graceful shutdown.

mod common;

use std::time::Duration;

use futures_util::StreamExt;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::{Error, Message};
use weft_protocol::Identity;
use weft_relay::Config;

use common::{
    Device, Event, STEP, TestRelay, Ws, assert_closed, connect, connect_authenticated,
    memory_config, next_event, start_relay_with,
};

/// Opens a WebSocket to `url`, sending `X-Forwarded-For: forwarded_for` if
/// given. Returns the socket, or the HTTP status of the refusal.
async fn try_connect(url: &str, forwarded_for: Option<&str>) -> Result<Ws, u16> {
    let mut request = url.into_client_request().unwrap();
    if let Some(value) = forwarded_for {
        request
            .headers_mut()
            .insert("x-forwarded-for", value.parse().unwrap());
    }
    let connecting = tokio_tungstenite::connect_async(request);
    match tokio::time::timeout(STEP, connecting)
        .await
        .expect("connect timed out")
    {
        Ok((ws, _)) => Ok(ws),
        Err(Error::Http(response)) => Err(response.status().as_u16()),
        Err(err) => panic!("connect failed: {err}"),
    }
}

/// Connects and waits for the relay challenge, so the connection is known to
/// be counted.
async fn open(url: &str, forwarded_for: Option<&str>) -> Ws {
    let mut ws = try_connect(url, forwarded_for)
        .await
        .expect("connection refused");
    assert!(matches!(next_event(&mut ws, STEP).await, Event::Binary(_)));
    ws
}

async fn refused(url: &str, forwarded_for: Option<&str>) -> u16 {
    match try_connect(url, forwarded_for).await {
        Ok(_) => panic!("connection accepted, expected a refusal"),
        Err(status) => status,
    }
}

/// Retries until the relay accepts a connection again, as a closed
/// connection frees its slot only once the relay has seen it close.
async fn eventually_open(url: &str, forwarded_for: Option<&str>) -> Ws {
    let retry = async {
        loop {
            if let Ok(mut ws) = try_connect(url, forwarded_for).await {
                assert!(matches!(next_event(&mut ws, STEP).await, Event::Binary(_)));
                return ws;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    };
    tokio::time::timeout(STEP, retry)
        .await
        .expect("slot never freed")
}

fn limits(max_connections: usize, max_connections_per_ip: usize, trust_proxy: bool) -> Config {
    Config {
        max_connections,
        max_connections_per_ip,
        trust_proxy,
        ..memory_config()
    }
}

#[tokio::test]
async fn refuses_beyond_global_limit_with_503() {
    let url = start_relay_with(limits(2, 100, false)).await;
    let first = open(&url, None).await;
    let _second = open(&url, None).await;
    assert_eq!(refused(&url, None).await, 503);

    // Closing a connection frees its slot.
    drop(first);
    let _third = eventually_open(&url, None).await;
    assert_eq!(refused(&url, None).await, 503);
}

#[tokio::test]
async fn refuses_beyond_per_address_limit_with_429() {
    let url = start_relay_with(limits(100, 2, false)).await;
    let first = open(&url, None).await;
    let _second = open(&url, None).await;
    assert_eq!(refused(&url, None).await, 429);

    drop(first);
    let _third = eventually_open(&url, None).await;
}

#[tokio::test]
async fn ignores_forwarded_for_without_trust() {
    let url = start_relay_with(limits(100, 1, false)).await;
    let _first = open(&url, Some("192.0.2.1")).await;
    // Another forwarded address does not help: the peer address counts.
    assert_eq!(refused(&url, Some("192.0.2.2")).await, 429);
}

#[tokio::test]
async fn uses_last_forwarded_address_with_trust() {
    let url = start_relay_with(limits(100, 1, true)).await;
    let _first = open(&url, Some("198.51.100.7, 192.0.2.1")).await;
    // Same last entry, whatever the client put before it.
    assert_eq!(refused(&url, Some("192.0.2.1")).await, 429);
    assert_eq!(refused(&url, Some("192.0.2.9, 192.0.2.1")).await, 429);
    // A different last entry is another client, even if an earlier entry
    // repeats the first one.
    let _second = open(&url, Some("192.0.2.1, 192.0.2.2")).await;
}

#[tokio::test]
async fn closes_silent_connection_after_idle_timeout() {
    let config = Config {
        idle_timeout: Duration::from_millis(400),
        ..memory_config()
    };
    let url = start_relay_with(config).await;
    let mut ws = connect_authenticated(&url, &Identity::generate()).await;

    // Not reading means the relay's ping is never answered.
    tokio::time::sleep(Duration::from_millis(1000)).await;
    let close = async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Ping(_))) => continue,
                other => return other,
            }
        }
    };
    match tokio::time::timeout(STEP, close).await.unwrap() {
        Some(Ok(Message::Close(Some(frame)))) => {
            assert_eq!(frame.code, CloseCode::Normal);
            assert_eq!(frame.reason, "idle timeout");
        }
        other => panic!("expected an idle-timeout close frame, got {other:?}"),
    }
}

#[tokio::test]
async fn keeps_connection_that_answers_pings() {
    let config = Config {
        idle_timeout: Duration::from_millis(400),
        ..memory_config()
    };
    let url = start_relay_with(config).await;
    let mut a = Device::connect(&url).await;
    let mut b = Device::connect(&url).await;
    common::pair(&mut a, &mut b).await;

    // Reading answers the relay pings, so the connections outlive several
    // idle timeouts without sending anything.
    let (wait_a, wait_b) = tokio::join!(
        next_event(&mut a.ws, Duration::from_millis(1200)),
        next_event(&mut b.ws, Duration::from_millis(1200)),
    );
    assert_eq!(wait_a, Event::Timeout);
    assert_eq!(wait_b, Event::Timeout);
    a.assert_delivers(&mut b, "still connected").await;
}

#[tokio::test]
async fn shutdown_closes_connections_with_a_close_frame() {
    let relay = TestRelay::start(memory_config()).await;
    let url = relay.url.clone();
    let mut device = Device::connect(&url).await;
    // A connection still waiting to authenticate.
    let (mut pending, _) = connect(&url).await;

    relay.shutdown().await;

    for ws in [&mut device.ws, &mut pending] {
        match tokio::time::timeout(STEP, ws.next()).await.unwrap() {
            Some(Ok(Message::Close(Some(frame)))) => assert_eq!(frame.code, CloseCode::Away),
            other => panic!("expected a going-away close frame, got {other:?}"),
        }
    }
    assert!(
        tokio_tungstenite::connect_async(url.as_str())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn shutdown_does_not_wait_for_clients_that_never_close() {
    let relay = TestRelay::start(memory_config()).await;
    // These clients never read, so they never answer the close handshake;
    // the relay must still return promptly.
    let mut idle = Vec::new();
    for _ in 0..3 {
        idle.push(Device::connect(&relay.url).await);
    }
    let started = std::time::Instant::now();
    relay.shutdown().await;
    assert!(started.elapsed() < Duration::from_secs(2));
    for device in &mut idle {
        assert_closed(&mut device.ws).await;
    }
}

async fn http_get(ws_url: &str, path: &str) -> (String, String) {
    let addr = ws_url
        .strip_prefix("ws://")
        .and_then(|rest| rest.strip_suffix("/v1"))
        .expect("expected ws://addr/v1");
    let mut stream = TcpStream::connect(addr).await.unwrap();
    let request = format!("GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n");
    stream.write_all(request.as_bytes()).await.unwrap();
    let mut raw = String::new();
    stream.read_to_string(&mut raw).await.unwrap();
    let (head, body) = raw.split_once("\r\n\r\n").expect("missing HTTP header end");
    (head.to_owned(), body.to_owned())
}

#[tokio::test]
async fn index_serves_self_contained_info_page_and_escapes_contact() {
    let mut config = Config::new("weft.example.test", ":memory:");
    config.contact = Some("Ops <abuse@example.test> & \"DSA\" 'team'".to_owned());
    let url = start_relay_with(config).await;

    let (head, body) = http_get(&url, "/").await;
    let head_lower = head.to_ascii_lowercase();
    assert!(
        head.starts_with("HTTP/1.1 200 OK"),
        "unexpected status: {head}"
    );
    assert!(
        head_lower.contains("content-type: text/html"),
        "expected text/html content-type, got: {head}"
    );
    assert!(body.contains("weft.example.test"));
    assert!(body.contains("/v1"));
    assert!(body.contains("XChaCha20-Poly1305"));
    assert!(body.contains("https://github.com/fraustiz/weft"));
    assert!(body.contains("Ops &lt;abuse@example.test&gt; &amp; &quot;DSA&quot; &#39;team&#39;"));
    assert!(!body.contains("<abuse@example.test>"));

    let default_url = start_relay_with(memory_config()).await;
    let (default_head, default_body) = http_get(&default_url, "/").await;
    assert!(default_head.starts_with("HTTP/1.1 200 OK"));
    assert!(default_body.contains("relay.test"));
    assert!(!default_body.contains("Operator contact"));
}
