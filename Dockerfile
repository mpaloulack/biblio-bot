# syntax=docker/dockerfile:1

# --- Build stage -----------------------------------------------------------
FROM rust:1.98-slim-bookworm AS builder

WORKDIR /build

# Dependencies change far less often than the code, so compile them in their own
# layer. Both crate roots have to exist for cargo to resolve the manifest.
COPY Cargo.toml Cargo.lock rust-toolchain.toml ./
RUN mkdir src \
    && echo "fn main() {}" > src/main.rs \
    && echo "" > src/lib.rs \
    && cargo build --release \
    && rm -rf src

COPY src ./src
# cargo goes by mtime: without this touch the placeholder build stays cached.
RUN touch src/main.rs src/lib.rs && cargo build --release

# --- Runtime stage ---------------------------------------------------------
FROM debian:bookworm-slim

LABEL org.opencontainers.image.source="https://github.com/mpaloulack/biblio-bot" \
      org.opencontainers.image.description="Discord bot that searches ebooks through Prowlarr and sends them to qBittorrent" \
      org.opencontainers.image.licenses="MIT"

RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Standing searches are the only thing written to disk, and /data is expected
# to be a volume. Nothing here needs root.
RUN useradd --system --create-home --uid 10001 biblio \
    && mkdir -p /data \
    && chown biblio:biblio /data
USER biblio
WORKDIR /data
VOLUME ["/data"]
ENV WATCHLIST_PATH=/data/watchlist.json

COPY --from=builder /build/target/release/biblio-bot /usr/local/bin/biblio-bot

ENTRYPOINT ["/usr/local/bin/biblio-bot"]
