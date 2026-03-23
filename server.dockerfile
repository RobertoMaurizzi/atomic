# syntax=docker/dockerfile:1
# =============================================================================
# Stage 1a: Cargo Chef planner (dependency caching)
# =============================================================================
FROM rust:1.94-bookworm AS planner
RUN cargo install cargo-chef
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

# =============================================================================
# Stage 1b: Cargo Chef cook + build atomic-server
# =============================================================================
FROM rust:1.94-bookworm AS rust-builder

# Install mold linker + cargo-chef
RUN apt-get update && apt-get install -y --no-install-recommends mold && rm -rf /var/lib/apt/lists/*
RUN cargo install cargo-chef
WORKDIR /app

# Copy linker config
COPY .cargo/ .cargo/

# Cook dependencies (cached until Cargo.toml/lock changes)
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo chef cook --profile server --recipe-path recipe.json -p atomic-server

# Copy real workspace source
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# Workspace stubs for crates we don't build but Cargo needs for resolution
COPY src-tauri/Cargo.toml src-tauri/Cargo.toml
RUN mkdir -p src-tauri/src && \
    echo "fn main() {}" > src-tauri/src/main.rs && \
    echo "pub fn lib() {}" > src-tauri/src/lib.rs && \
    echo "fn main() { tauri_build::build(); }" > src-tauri/build.rs

# Build atomic-server with the faster server profile
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --profile server -p atomic-server && \
    cp /app/target/server/atomic-server /usr/local/bin/atomic-server

# =============================================================================
# Runtime
# =============================================================================
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends ca-certificates curl && \
    rm -rf /var/lib/apt/lists/*

RUN useradd --system --create-home --shell /bin/false atomic && \
    mkdir -p /data && chown atomic:atomic /data

COPY --from=rust-builder /usr/local/bin/atomic-server /usr/local/bin/atomic-server

USER atomic
VOLUME /data
EXPOSE 8080

ENTRYPOINT ["atomic-server", "--db-path", "/data/atomic.db"]
CMD ["serve", "--bind", "0.0.0.0", "--port", "8080"]

HEALTHCHECK --interval=10s --timeout=3s --start-period=5s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1
