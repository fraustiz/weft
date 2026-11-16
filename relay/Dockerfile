# syntax=docker/dockerfile:1
# Weft relay image. Build from the repository root:
#   docker build -f relay/Dockerfile -t weft-relay .

FROM rust:1.99.1-bookworm AS build
WORKDIR /src
# The whole workspace is needed for Cargo to resolve it, but only the relay
# and its dependencies are built.
COPY Cargo.toml Cargo.lock ./
COPY crates crates
COPY cli cli
COPY relay relay
RUN --mount=type=cache,target=/usr/local/cargo/registry,id=cargo-registry \
    --mount=type=cache,target=/src/target,id=weft-relay-target \
    cargo build -p weft-relay --release --locked \
    && install -D -m 0755 target/release/weft-relay /out/weft-relay

FROM debian:bookworm-slim
LABEL org.opencontainers.image.title="weft-relay" \
      org.opencontainers.image.description="Weft relay: forwards end-to-end encrypted clipboard envelopes between paired devices" \
      org.opencontainers.image.source="https://github.com/fraustiz/weft" \
      org.opencontainers.image.licenses="MIT"
# The relay makes no outbound TLS connection: no ca-certificates needed.
RUN groupadd --system --gid 10001 weft \
    && useradd --system --uid 10001 --gid weft --home-dir /data --no-create-home \
       --shell /usr/sbin/nologin weft \
    && mkdir /data \
    && chown weft:weft /data
COPY --from=build /out/weft-relay /usr/local/bin/weft-relay
USER 10001:10001
WORKDIR /data
VOLUME /data
ENV WEFT_BIND=0.0.0.0:8080 \
    WEFT_DB=/data/weft.sqlite3
EXPOSE 8080
STOPSIGNAL SIGTERM
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["/usr/local/bin/weft-relay", "--healthcheck"]
ENTRYPOINT ["/usr/local/bin/weft-relay"]
