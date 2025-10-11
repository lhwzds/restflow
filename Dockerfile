FROM node:20-alpine AS frontend-builder
WORKDIR /app/web

COPY web/package*.json ./
RUN npm ci

COPY web/ ./
RUN npm run build

FROM rust:bookworm AS backend-builder
WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
COPY --from=frontend-builder /app/web/dist ./web/dist

RUN cargo build --release --package restflow-server

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=backend-builder /app/target/release/restflow-server /usr/local/bin/restflow

EXPOSE 3000

# Run the application
CMD ["restflow"]
