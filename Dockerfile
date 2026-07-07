# OpenProject RS - Multi-stage Dockerfile
# Optimized for small image size and fast builds

# =============================================================================
# Stage 1: Build
# =============================================================================
# Toolchain MUST match the workspace `rust-version` (Cargo.toml → 1.95);
# an older base (this was pinned at 1.85) fails the build outright.
FROM rust:1.95-slim-bookworm AS builder

# Install build dependencies
RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy the whole workspace. The previous per-crate dummy-lib.rs cache
# scaffolding hardcoded a 16-crate list that drifted out of sync with the
# 24 workspace members (op-generated, op-canon, op-codegen-*, op-surreal-ast,
# ruff_openproject were all missing → the toml-only cache layer was already
# broken). A single COPY is correct and drift-proof; Railway's layer cache
# still skips this step when nothing under the build context changed.
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

# SQLx is used in *offline* mode (no DB reachable at build time). The query
# metadata is embedded via `sqlx::migrate!`, which is compile-time-checked
# against the checked-in ./crates/op-db/migrations, not a live DB.
ENV SQLX_OFFLINE=true

# Build the server binary (release).
RUN cargo build --release --package op-server

# =============================================================================
# Stage 2: Runtime
# =============================================================================
FROM debian:bookworm-slim AS runtime

# Install runtime dependencies
RUN apt-get update && apt-get install -y \
    ca-certificates \
    libssl3 \
    curl \
    tini \
    && rm -rf /var/lib/apt/lists/*

# Create non-root user
RUN useradd -m -u 1000 -s /bin/bash openproject

WORKDIR /app

# Copy binary from builder
COPY --from=builder /app/target/release/openproject-server /app/openproject-server

# Create directories for attachments and logs
RUN mkdir -p /var/openproject/assets /var/log/openproject && \
    chown -R openproject:openproject /var/openproject /var/log/openproject /app

# Switch to non-root user
USER openproject

# Environment defaults
ENV RUST_LOG=info,op_server=debug,op_api=debug \
    HOST=0.0.0.0 \
    PORT=8080 \
    OPENPROJECT_ATTACHMENTS_STORAGE_PATH=/var/openproject/assets

# Expose port
EXPOSE 8080

# Health check
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD curl -f http://localhost:8080/health || exit 1

# Use tini as init process for proper signal handling
ENTRYPOINT ["/usr/bin/tini", "--"]

# Run the server
CMD ["/app/openproject-server"]
