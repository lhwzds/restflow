FROM node:20-alpine AS frontend-builder
WORKDIR /app/web

COPY web/package*.json ./
RUN npm config set fetch-retries 10 \
    && npm config set fetch-retry-mintimeout 20000 \
    && npm config set fetch-retry-maxtimeout 300000 \
    && npm config set fetch-retry-factor 2 \
    && npm config set fetch-timeout 300000 \
    && for i in 1 2 3; do \
         if npm ci --prefer-offline --no-audit --no-fund; then \
           exit 0; \
         fi; \
         echo "npm ci failed, retrying ($i/3)..." >&2; \
         sleep $((i * 10)); \
       done; \
    exit 1

COPY web/ ./
RUN npm run build

FROM rust:bookworm AS backend-builder
WORKDIR /app

RUN apt-get update && \
    apt-get install -y pkg-config libgtk-3-dev libjavascriptcoregtk-4.1-dev libsoup-3.0-dev && \
    rm -rf /var/lib/apt/lists/*

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=frontend-builder /app/web/dist ./web/dist

RUN cargo build --release --package restflow-cli

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=backend-builder /app/target/release/restflow /usr/local/bin/restflow

EXPOSE 3000

# Run the application
CMD ["restflow", "start", "--http", "--no-browser"]
