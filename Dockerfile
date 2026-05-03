FROM rust:bookworm AS backend-builder
WORKDIR /app

RUN apt-get update && \
    apt-get install -y pkg-config libgtk-3-dev libjavascriptcoregtk-4.1-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates

RUN cargo build --release --package restflow-cli

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && \
    apt-get install -y ca-certificates \
    libgtk-3-0 \
    libjavascriptcoregtk-4.1-0 \
    libwebkit2gtk-4.1-0 \
    libsoup-3.0-0 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=backend-builder /app/target/release/restflow /usr/local/bin/restflow

EXPOSE 8787

# Run the daemon with MCP HTTP server
CMD ["restflow", "daemon", "start", "--foreground"]
