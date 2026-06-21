# syntax=docker/dockerfile:1

# 1. Build the SvelteKit static SPA
FROM node:22-bookworm-slim AS web
WORKDIR /web
COPY web/package.json web/package-lock.json web/.npmrc ./
RUN npm ci
COPY web/ ./
RUN npm run build

# 2. Cache Rust dependencies with cargo-chef
FROM rust:1.93-bookworm AS chef
RUN cargo install cargo-chef --locked
WORKDIR /app

FROM chef AS planner
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo chef prepare --recipe-path recipe.json

# 3. Build the release binary (deps are cached unless they change)
FROM chef AS builder
COPY --from=planner /app/recipe.json recipe.json
RUN cargo chef cook --release --recipe-path recipe.json
COPY Cargo.toml Cargo.lock ./
COPY src ./src
RUN cargo build --release --bin kash-server

# 4. Minimal runtime image
FROM debian:bookworm-slim AS runtime
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates wget \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /app
RUN useradd -r -u 10001 kash && mkdir -p /app/data && chown kash:kash /app/data
COPY --from=builder /app/target/release/kash-server /usr/local/bin/kash-server
COPY --from=web /web/build /app/web/build
ENV SERVER_HOST=0.0.0.0 \
    SERVER_PORT=3000 \
    DATABASE_PATH=/app/data \
    STATIC_DIR=/app/web/build \
    RUST_LOG=kash_server=info
USER kash
EXPOSE 3000
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://127.0.0.1:3000/ >/dev/null 2>&1 || exit 1
CMD ["kash-server"]
