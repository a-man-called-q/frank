# Build the headless daemon from the pinned workspace toolchain.  The Flutter
# client is a separate artifact and is deliberately not part of this image.
FROM rust:1.89-bookworm AS builder

WORKDIR /src
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
COPY backend ./backend
COPY packs ./packs

RUN cargo build --locked --release -p frank-server --bin frankd

FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install --no-install-recommends --yes ca-certificates curl \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --create-home --home-dir /var/lib/frank --shell /usr/sbin/nologin frank \
    && mkdir -p /var/lib/frank /etc/frank/tls \
    && chown -R frank:frank /var/lib/frank /etc/frank

COPY --from=builder /src/target/release/frankd /usr/local/bin/frankd

USER frank
WORKDIR /var/lib/frank
EXPOSE 37465

HEALTHCHECK --interval=10s --timeout=3s --start-period=10s --retries=3 \
    CMD curl --fail --silent --show-error --insecure https://127.0.0.1:37465/v2/health || exit 1

ENTRYPOINT ["/usr/local/bin/frankd"]
