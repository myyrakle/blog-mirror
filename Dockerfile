# ── Build stage ─────────────────────────────────────────────────────────────
FROM rust:1.93-alpine AS builder

WORKDIR /app

# Install build dependencies (git2 needs libssl, libgit2; sqlx needs pkgconfig)
RUN apk add --no-cache \
    pkgconfig \
    openssl-dev \
    libgit2-dev \
    musl-dev

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
    openssl \
    libgit2 \
    git

WORKDIR /app

COPY --from=builder /app/target/release/blog-mirror /usr/local/bin/blog-mirror
COPY --from=builder /app/migrations ./migrations

ENTRYPOINT ["blog-mirror"]
CMD ["sync-loop"]
