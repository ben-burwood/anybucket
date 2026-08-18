# ---------------------------------------------------------------------------
# 1. Build the Vue SPA (web variant) → /app/dist
# ---------------------------------------------------------------------------
FROM node:22-slim AS web

WORKDIR /app

COPY package.json package-lock.json ./

RUN npm ci

COPY . .

RUN npm run build:web

# ---------------------------------------------------------------------------
# 2. Build the axum server binary
# ---------------------------------------------------------------------------
FROM rust:1.82-slim AS server

# aws-lc (default crypto backend) needs a C toolchain + cmake; TLS needs perl.
RUN apt-get update \
    && apt-get install -y --no-install-recommends build-essential cmake perl pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

COPY . .

RUN cargo build --release -p anybucket-server

# ---------------------------------------------------------------------------
# 3. Slim runtime: server binary + built SPA
# ---------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

# CA certificates for TLS to S3 endpoints.
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=server /src/target/release/anybucket-server /usr/local/bin/anybucket-server
COPY --from=web /app/dist /app/dist

# Static SPA + persisted config/secrets locations
ENV ANYBUCKET_STATIC_DIR=/app/dist \
    ANYBUCKET_CONFIG_DIR=/config \
    ANYBUCKET_PORT=8080

# Connection metadata + encrypted secrets live on this volume.
VOLUME ["/config"]
EXPOSE 8080

ENTRYPOINT ["anybucket-server"]
