# =============================================================================
# LexHack backend + frontend — production image.
#
# Multi-stage build:
#
#   1. backend-builder   — compiles the Rust backend in release mode.
#   2. frontend-builder  — compiles the Leptos CSR app to WASM with Trunk.
#   3. runtime           — minimal Debian with both artifacts.
#
# The runtime image serves the API and (if FRONTEND_DIST is set) the
# compiled frontend bundle from the same process. This makes a single
# container sufficient for a demo deployment; a production deployment
# would typically put a reverse proxy in front and serve the static
# assets from a CDN.
#
# Requires a database — see docker-compose.yml for a local PostgreSQL.
# =============================================================================

# -----------------------------------------------------------------------------
# Stage 1: backend builder
# -----------------------------------------------------------------------------
FROM rust:1-slim-bookworm AS backend-builder

WORKDIR /build

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        libssl-dev \
        ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# Copy manifests first for layer caching.
COPY Cargo.toml Cargo.lock ./
COPY frontend/Cargo.toml ./frontend/Cargo.toml

# A dummy main lets us fetch and compile dependencies before the real
# sources land, so a source-only change does not invalidate the whole
# dependency layer.
RUN mkdir -p src && echo 'fn main() {}' > src/main.rs \
    && mkdir -p frontend/src && echo '' > frontend/src/lib.rs \
    && cargo fetch

# Copy real sources.
COPY migrations/ ./migrations/
COPY src/ ./src/
COPY tests/ ./tests/
COPY frontend/ ./frontend/

RUN cargo build --release --locked -p lexhack-backend --bin lexhack-backend

# -----------------------------------------------------------------------------
# Stage 2: frontend builder (Trunk + WASM)
# -----------------------------------------------------------------------------
FROM rust:1-slim-bookworm AS frontend-builder

WORKDIR /build/frontend

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        pkg-config \
        ca-certificates \
        git \
    && rm -rf /var/lib/apt/lists/*

RUN rustup target add wasm32-unknown-unknown
RUN cargo install trunk --locked

# Copy the frontend crate. The workspace root is one level up, so we
# copy the whole workspace but build only the frontend member.
WORKDIR /build
COPY Cargo.toml Cargo.lock ./
COPY frontend/ ./frontend/
COPY src/ ./src/
COPY migrations/ ./migrations/

WORKDIR /build/frontend

# The API base URL is a build-time, public value. Override with
# --build-arg API_BASE_URL=... in production.
ARG API_BASE_URL=http://localhost:3000
ENV API_BASE_URL=${API_BASE_URL}

RUN trunk build --release

# -----------------------------------------------------------------------------
# Stage 3: runtime
# -----------------------------------------------------------------------------
FROM debian:bookworm-slim AS runtime

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        ca-certificates \
        libssl3 \
        curl \
    && rm -rf /var/lib/apt/lists/*

# Non-root user.
RUN groupadd --system --gid 10001 lexhack \
    && useradd --system --uid 10001 --gid lexhack --home /app --shell /usr/sbin/nologin lexhack

WORKDIR /app

# Backend binary.
COPY --from=backend-builder /build/target/release/lexhack-backend /usr/local/bin/lexhack-backend

# Migrations — applied by running the binary with `--migrate`, which
# is what docker-compose's `migrate` service does. `sqlx-cli` is not
# shipped in the runtime image; the same binary handles both roles.
COPY --from=backend-builder /build/migrations /app/migrations

# Frontend static assets.
COPY --from=frontend-builder /build/frontend/dist /app/dist

RUN chown -R lexhack:lexhack /app

USER lexhack

ENV FRONTEND_DIST=/app/dist
ENV HOST=0.0.0.0
ENV PORT=3000

EXPOSE 3000

# The backend's /health endpoint is used as the container health check.
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl --fail --silent http://127.0.0.1:3000/health || exit 1

CMD ["/usr/local/bin/lexhack-backend"]
