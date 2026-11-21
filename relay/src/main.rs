//! The `weft-relay` binary.
//!
//! Configuration comes from the environment:
//!
//! | Variable                      | Default               | Meaning |
//! | ----------------------------- | --------------------- | ------- |
//! | `WEFT_BIND`                   | `127.0.0.1:8080`      | Address and port to listen on (plain HTTP/WebSocket; TLS is the reverse proxy's job). |
//! | `WEFT_RELAY_NAME`             | `localhost`           | Name devices sign when they authenticate: the public host name of the relay, without port. |
//! | `WEFT_DB`                     | `weft-relay.sqlite3`  | SQLite database holding the groups. |
//! | `WEFT_TRUST_PROXY`            | `0`                   | `1` to read forwarded client-address headers. Only behind a reverse proxy, with the relay unreachable otherwise (or with `WEFT_TRUSTED_PROXIES` set). |
//! | `WEFT_CLIENT_IP_HEADER`       | unset                 | Header carrying the real client address, set by a trusted edge (e.g. `cf-connecting-ip`). Read before `X-Forwarded-For`. Needs `WEFT_TRUST_PROXY=1`. |
//! | `WEFT_TRUSTED_PROXIES`        | unset                 | Comma-separated proxy addresses whose forwarded headers are trusted. Unset trusts any peer (only safe when the relay is unreachable except through its proxy). |
//! | `WEFT_MAX_CONNECTIONS`        | `2000`                | Open WebSocket connections in total; more are refused with HTTP 503. |
//! | `WEFT_MAX_CONNECTIONS_PER_IP` | `20`                  | Open WebSocket connections per client address (per /64 for IPv6); more are refused with HTTP 429. |
//! | `WEFT_MAX_GROUPS`             | `100000`              | Largest number of groups kept at once; a new group beyond this is refused. |
//! | `WEFT_GROUP_TTL_SECS`         | `7776000` (90 d)      | A group whose members have all been idle this long is pruned. |
//! | `WEFT_IDLE_TIMEOUT_SECS`      | `120`                 | An authenticated connection silent this long is closed; it is pinged halfway. |
//! | `WEFT_CONTACT`                | unset                 | Optional operator contact shown on `GET /` (for abuse/DSA notices). |
//! | `RUST_LOG`                    | `info`                | Log filter, e.g. `weft_relay=debug`. |
//!
//! `weft-relay --healthcheck` asks the relay listening on `WEFT_BIND` for
//! `/healthz` and exits 0 if it answers 200, 1 otherwise (for container
//! health checks, without curl).
//!
//! SIGTERM and Ctrl-C stop the relay gracefully: it stops accepting, closes
//! open connections with a close frame and exits 0.

use std::fmt::Display;
use std::io::{self, Read, Write};
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, TcpStream, ToSocketAddrs};
use std::str::FromStr;
use std::time::Duration;

use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;
use weft_relay::{
    Config, GROUP_INACTIVE_TTL, IDLE_TIMEOUT, MAX_CONNECTIONS, MAX_CONNECTIONS_PER_IP, MAX_GROUPS,
};

const DEFAULT_BIND: &str = "127.0.0.1:8080";

#[tokio::main]
async fn main() -> io::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--healthcheck") {
        std::process::exit(if healthcheck() { 0 } else { 1 });
    }

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let bind = std::env::var("WEFT_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_owned());
    let relay_name = std::env::var("WEFT_RELAY_NAME").unwrap_or_else(|_| "localhost".to_owned());
    let db_path = std::env::var("WEFT_DB").unwrap_or_else(|_| "weft-relay.sqlite3".to_owned());
    let mut config = Config::new(relay_name, db_path);
    config.trust_proxy = flag("WEFT_TRUST_PROXY")?;
    config.client_ip_header = std::env::var("WEFT_CLIENT_IP_HEADER")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());
    config.trusted_proxies = ip_list("WEFT_TRUSTED_PROXIES")?;
    config.max_connections = positive("WEFT_MAX_CONNECTIONS", MAX_CONNECTIONS)?;
    config.max_connections_per_ip =
        positive("WEFT_MAX_CONNECTIONS_PER_IP", MAX_CONNECTIONS_PER_IP)?;
    config.max_groups = positive("WEFT_MAX_GROUPS", MAX_GROUPS)?;
    config.group_inactive_ttl = Duration::from_secs(positive(
        "WEFT_GROUP_TTL_SECS",
        GROUP_INACTIVE_TTL.as_secs(),
    )?);
    config.idle_timeout =
        Duration::from_secs(positive("WEFT_IDLE_TIMEOUT_SECS", IDLE_TIMEOUT.as_secs())?);
    config.contact = std::env::var("WEFT_CONTACT")
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty());

    // Registered before serving, so a SIGTERM sent right after start is not
    // lost.
    let shutdown = shutdown_signal()?;
    let listener = TcpListener::bind(&bind).await?;
    tracing::info!(
        bind = %listener.local_addr()?,
        relay_name = %config.relay_name,
        db = %config.db_path.display(),
        trust_proxy = config.trust_proxy,
        max_connections = config.max_connections,
        max_connections_per_ip = config.max_connections_per_ip,
        idle_timeout_secs = config.idle_timeout.as_secs(),
        "weft relay listening"
    );
    weft_relay::serve_with_shutdown(listener, config, shutdown).await?;
    tracing::info!("weft relay stopped");
    Ok(())
}

/// Completes on SIGTERM or Ctrl-C (SIGINT).
fn shutdown_signal() -> io::Result<impl Future<Output = ()> + Send + 'static> {
    #[cfg(unix)]
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    Ok(async move {
        #[cfg(unix)]
        let terminate = terminate.recv();
        #[cfg(not(unix))]
        let terminate = std::future::pending::<Option<()>>();
        tokio::select! {
            _ = tokio::signal::ctrl_c() => tracing::info!("received Ctrl-C"),
            _ = terminate => tracing::info!("received SIGTERM"),
        }
    })
}

/// A comma-separated list of IP addresses, or an empty list when unset.
fn ip_list(name: &str) -> io::Result<Vec<IpAddr>> {
    let Ok(value) = std::env::var(name) else {
        return Ok(Vec::new());
    };
    value
        .split(',')
        .map(str::trim)
        .filter(|entry| !entry.is_empty())
        .map(|entry| {
            entry
                .parse::<IpAddr>()
                .map_err(|err| invalid(name, entry, err))
        })
        .collect()
}

/// A boolean variable: `1`/`true`/`yes` or `0`/`false`/`no`; unset is false.
fn flag(name: &str) -> io::Result<bool> {
    match std::env::var(name).as_deref().map(str::trim) {
        Err(_) | Ok("" | "0" | "false" | "no") => Ok(false),
        Ok("1" | "true" | "yes") => Ok(true),
        Ok(other) => Err(invalid(name, other, "expected 0 or 1")),
    }
}

/// A number greater than zero, or `default` when unset.
fn positive<T>(name: &str, default: T) -> io::Result<T>
where
    T: FromStr + PartialOrd + Default,
    T::Err: Display,
{
    let Ok(value) = std::env::var(name) else {
        return Ok(default);
    };
    match value.trim().parse::<T>() {
        Ok(parsed) if parsed > T::default() => Ok(parsed),
        Ok(_) => Err(invalid(name, &value, "must be greater than 0")),
        Err(err) => Err(invalid(name, &value, err)),
    }
}

fn invalid(name: &str, value: &str, why: impl Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidInput,
        format!("{name}={value:?}: {why}"),
    )
}

/// GETs `/healthz` from the relay on `WEFT_BIND` (on loopback when it listens
/// on every address). True if it answered 200.
fn healthcheck() -> bool {
    let bind = std::env::var("WEFT_BIND").unwrap_or_else(|_| DEFAULT_BIND.to_owned());
    let Some(mut addr) = bind.to_socket_addrs().ok().and_then(|mut a| a.next()) else {
        return false;
    };
    match addr.ip() {
        IpAddr::V4(ip) if ip.is_unspecified() => addr.set_ip(Ipv4Addr::LOCALHOST.into()),
        IpAddr::V6(ip) if ip.is_unspecified() => addr.set_ip(Ipv6Addr::LOCALHOST.into()),
        _ => {}
    }
    get_healthz(addr).is_ok_and(|status| status.starts_with("HTTP/1.1 200"))
}

fn get_healthz(addr: SocketAddr) -> io::Result<String> {
    let timeout = Duration::from_secs(3);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)?;
    stream.set_read_timeout(Some(timeout))?;
    stream.set_write_timeout(Some(timeout))?;
    stream.write_all(b"GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")?;
    let mut status = [0u8; 12];
    stream.read_exact(&mut status)?;
    Ok(String::from_utf8_lossy(&status).into_owned())
}
