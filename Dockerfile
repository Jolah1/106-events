# One image, one process, one port.
#
# The server binary serves the JSON API, the server-rendered public pages and
# the built dashboard from a single origin — so the deploy artifact is a single
# binary plus the dashboard's static files, and there is nothing to keep in
# sync between two services.

# --- 1. the dashboard ---------------------------------------------------------
FROM node:22-slim AS dashboard

WORKDIR /app/dashboard
# Dependencies first: this layer is cached until the lockfile actually changes.
COPY dashboard/package.json dashboard/package-lock.json ./
RUN npm ci

COPY dashboard/ ./
RUN npm run build


# --- 2. the server ------------------------------------------------------------
FROM rust:1.96-slim-bookworm AS server

WORKDIR /app/server

# Compile-time-checked queries verify against the committed .sqlx cache rather
# than a live database — there is no Postgres to connect to during a build.
ENV SQLX_OFFLINE=true

# Warm the dependency layer with a stub binary, so editing our own source
# doesn't rebuild every crate we depend on.
COPY server/Cargo.toml server/Cargo.lock ./
RUN mkdir -p src && \
    echo 'fn main() {}' > src/main.rs && \
    echo '' > src/lib.rs && \
    cargo build --release && \
    rm -rf src

COPY server/ ./
# Cargo skips rebuilding when only mtimes moved; touch the roots so the real
# sources definitely replace the stubs above.
RUN touch src/main.rs src/lib.rs && cargo build --release


# --- 3. what actually ships ---------------------------------------------------
FROM debian:bookworm-slim AS runtime

# ca-certificates: outbound TLS for SMTP and the messaging providers.
RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates && \
    rm -rf /var/lib/apt/lists/*

# Never run as root.
RUN useradd --create-home --uid 10001 app
WORKDIR /app
USER app

COPY --from=server --chown=app:app /app/server/target/release/server ./server
COPY --from=dashboard --chown=app:app /app/dashboard/dist ./dashboard

ENV BIND_ADDR=0.0.0.0:8080 \
    DASHBOARD_DIST=/app/dashboard \
    COOKIE_SECURE=true \
    RUST_LOG=server=info,tower_http=warn

EXPOSE 8080
CMD ["./server"]
