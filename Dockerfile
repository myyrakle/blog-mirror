# ── Build stage ─────────────────────────────────────────────────────────────
FROM rust:1.93-alpine AS builder

WORKDIR /app

# musl targets link statically by default; disable that so we can use
# shared system libraries (libgit2.so). reqwest uses rustls (pure Rust),
# so no OpenSSL C library is needed.
ENV RUSTFLAGS="-C target-feature=-crt-static"

RUN apk add --no-cache \
    pkgconfig \
    musl-dev \
    libgit2-dev

# Cache dependencies separately from source
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo 'fn main(){}' > src/main.rs \
    && cargo build --release \
    && rm -rf src

# Build the real binary
COPY src ./src
COPY migrations ./migrations
RUN touch src/main.rs && cargo build --release

# ── Runtime stage ────────────────────────────────────────────────────────────
FROM alpine:3

RUN apk add --no-cache \
    ca-certificates \
    libgit2

WORKDIR /app

COPY --from=builder /app/target/release/blog-mirror /usr/local/bin/blog-mirror
COPY --from=builder /app/migrations ./migrations

ENTRYPOINT ["blog-mirror"]
CMD ["sync-loop"]
