FROM node:20-alpine AS frontend-builder
WORKDIR /app/frontend

COPY frontend/package*.json ./
RUN npm ci

COPY frontend/ ./
RUN npm run build

FROM rust:latest AS backend-builder
WORKDIR /app

COPY backend/ ./backend/

COPY --from=frontend-builder /app/frontend/dist ./frontend/dist/

WORKDIR /app/backend

RUN cargo build --release

FROM debian:bookworm-slim
WORKDIR /app

RUN apt-get update && \
    apt-get install -y ca-certificates && \
    rm -rf /var/lib/apt/lists/*

COPY --from=backend-builder /app/backend/target/release/backend /usr/local/bin/restflow

EXPOSE 3000

# Run the application
CMD ["restflow"]