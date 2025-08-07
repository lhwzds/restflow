FROM rustlang/rust:nightly-alpine AS builder

RUN apk add --no-cache musl-dev pkgconfig openssl-dev

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs && \
    cargo build --release && \
    rm -rf src

COPY src ./src
RUN touch src/main.rs && \
    cargo build --release

FROM alpine:3.20

RUN apk add --no-cache ca-certificates openssl libgcc

COPY --from=builder /app/target/release/backend /usr/local/bin/restflow

EXPOSE 3000

CMD ["restflow"]