# syntax=docker/dockerfile:1.7

# --- сборка ---------------------------------------------------------------
FROM rust:1.90-slim-bookworm AS builder

# cmake нужен aws-lc-rs, pkg-config — ring/rusqlite.
RUN apt-get update \
    && apt-get install -y --no-install-recommends cmake build-essential pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

COPY Cargo.toml Cargo.lock ./
COPY crates/pms-proto ./crates/pms-proto
COPY crates/pms-server ./crates/pms-server
# Приложение в воркспейсе, но его код серверу не нужен: пустой крейт нужен
# только карго, чтобы разрешить воркспейс.
COPY src-tauri/Cargo.toml ./src-tauri/Cargo.toml
RUN mkdir -p src-tauri/src && printf '' > src-tauri/src/lib.rs

RUN cargo build --profile server -p pms-server --locked

# --- рантайм --------------------------------------------------------------
FROM debian:bookworm-slim

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates openssl sqlite3 tar gzip \
    && rm -rf /var/lib/apt/lists/* \
    && groupadd --gid 10001 pms \
    && useradd --uid 10001 --gid pms --home-dir /var/lib/pms --no-create-home --shell /usr/sbin/nologin pms \
    && mkdir -p /var/lib/pms/certs /etc/pms \
    && chown -R pms:pms /var/lib/pms

COPY --from=builder /src/target/server/pms-server /usr/local/bin/pms-server
COPY deploy/docker/server.toml /etc/pms/server.toml
COPY deploy/docker/entrypoint.sh /usr/local/bin/pms-entrypoint

RUN chmod 0755 /usr/local/bin/pms-entrypoint

ENV RUST_LOG=info
VOLUME ["/var/lib/pms", "/etc/pms"]
EXPOSE 8443

USER pms
ENTRYPOINT ["/usr/local/bin/pms-entrypoint"]
CMD ["run", "--config", "/etc/pms/server.toml"]
