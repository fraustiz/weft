# Weft relay

The relay connects the devices of a Weft group. Each device authenticates
with a signed challenge, then the relay forwards its envelopes to the other
devices of its group while they are connected. Envelopes are end-to-end
encrypted: the relay reads only their clear header (sender, recipient, kind,
message id, counter) to route them, and never decrypts anything. It also
carries the pairing frames that build groups. The wire protocol is described
in [`docs/protocol.md`](../docs/protocol.md).

The relay speaks plain HTTP and WebSocket. Run it behind a reverse proxy that
terminates TLS; clients connect to `wss://<host>/v1`.

## Configuration

All settings come from environment variables.

| Variable | Default | Meaning |
| --- | --- | --- |
| `WEFT_BIND` | `127.0.0.1:8080` (`0.0.0.0:8080` in the image) | Address to listen on. |
| `WEFT_RELAY_NAME` | `localhost` | Public host name of the relay, without port. Devices sign it when they authenticate and refuse a relay whose name differs from the host they connect to. |
| `WEFT_DB` | `weft-relay.sqlite3` (`/data/weft.sqlite3` in the image) | SQLite database. |
| `WEFT_TRUST_PROXY` | `0` | `1` to take the client address from the last `X-Forwarded-For` entry, the one appended by your proxy. Enable it only when the relay cannot be reached except through that proxy; otherwise anyone can pick their address. |
| `WEFT_MAX_CONNECTIONS` | `2000` | Open WebSocket connections in total. |
| `WEFT_MAX_CONNECTIONS_PER_IP` | `20` | Open WebSocket connections per client address (per /64 for IPv6). |
| `WEFT_IDLE_TIMEOUT_SECS` | `120` | Idle timeout of an authenticated connection. |
| `RUST_LOG` | `info` | Log filter, for example `weft_relay=debug`. |

The effective configuration is logged at startup.

## Limits

- Beyond `WEFT_MAX_CONNECTIONS`, new WebSocket upgrades get HTTP 503; beyond
  `WEFT_MAX_CONNECTIONS_PER_IP` from one address, HTTP 429.
- A device has 10 s to answer the authentication challenge.
- An authenticated device silent for half the idle timeout is pinged, and is
  disconnected if it has still sent nothing (not even a pong) when the idle
  timeout expires. Clients ping every 30 s when idle.
- A frame is at most one envelope carrying 256 KiB of clipboard text plus
  its header, nonce and tag; a larger frame closes the connection.
- Each connection may open 10 pairing slots and send 20 pair requests per
  minute. A pairing slot lives 5 minutes.
- A group holds at most 10 devices.

On SIGTERM or Ctrl-C the relay stops accepting connections, closes the open
ones with a close frame (code 1001, "going away"; clients reconnect) and
exits 0 within 5 seconds.

## Running with Docker

The image is `ghcr.io/fraustiz/weft/relay`, for linux/amd64 and linux/arm64.
Tags: the branch name (`main`), the commit (`sha-xxxxxxx`) and, for releases,
the version (`relay-v0.1.0` publishes `0.1.0`).

```sh
docker run -d --name weft-relay \
  -e WEFT_RELAY_NAME=relay.example.org \
  -p 127.0.0.1:8080:8080 \
  -v weft-data:/data \
  ghcr.io/fraustiz/weft/relay:main
```

See [`compose.yml`](compose.yml) for a Compose example with resource limits.
The container runs as uid 10001 and stores its database in the `/data`
volume; a bind-mounted directory must be writable by that uid. The image has
a health check (`weft-relay --healthcheck`, which requests `/healthz`).

To build the image yourself, from the repository root:

```sh
docker build -f relay/Dockerfile -t weft-relay .
```

Or run it without Docker: `cargo run --release -p weft-relay`.

## Data stored

The database holds one row per device that belongs to a group: its device id
(16 bytes derived from its public signing key) and the number of its group.
Nothing else is stored: no keys, no names, no message, no address. Pairing
slots and pending joins live in memory and disappear on restart. Messages are
never stored: an envelope for a device that is not connected is dropped.

Back up `/data/weft.sqlite3` to keep the groups across a reinstall; losing it
means every group has to pair again.

## Logs

Logs go to standard output. At the default level they contain the startup
configuration, connections and disconnections with the first 4 bytes of the
device id, group changes (group number, short device id), replaced
connections, idle timeouts, shutdown, and storage errors.
They never contain message content. At debug level they also contain dropped
frames and pairing events (with short device ids) and the client address of
refused connections.
