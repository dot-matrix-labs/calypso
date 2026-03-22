# syntax=docker/dockerfile:1

# ──────────────────────────────────────────────────────────────────────────────
# Stage 1 — builder
# Builds the calypso-cli binary in release mode.
# The calypso-blueprint submodule must be available for embed compile-time files.
# ──────────────────────────────────────────────────────────────────────────────
FROM rust:1.88-slim AS builder

# Install git (needed by build.rs to embed git hash, and to init submodules)
RUN apt-get update && apt-get install -y --no-install-recommends \
    git \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

# Copy workspace-level manifests first to leverage layer caching.
COPY Cargo.toml Cargo.lock ./
COPY cli/Cargo.toml ./cli/Cargo.toml
COPY crates/nightshift-core/Cargo.toml ./crates/nightshift-core/Cargo.toml
COPY crates/calypso-web/Cargo.toml ./crates/calypso-web/Cargo.toml

# Copy build scripts and all source needed at compile time.
COPY cli/ ./cli/
COPY crates/ ./crates/
COPY calypso-blueprint/ ./calypso-blueprint/

# Mark the working directory as safe for git (build.rs invokes git rev-parse).
RUN git config --global --add safe.directory /build

# Initialise a minimal git repo so build.rs can read a HEAD hash.
# In CI the context is a plain directory (not a git repo), so we seed one.
RUN if [ ! -d .git ]; then \
        git init && \
        git commit --allow-empty -m "docker-build-context"; \
    fi

# Build only the release binary; skip test-only bins.
RUN cargo build --release --bin calypso-cli

# ──────────────────────────────────────────────────────────────────────────────
# Stage 2 — runtime
# Copies only the compiled binary into a minimal Debian image.
# ──────────────────────────────────────────────────────────────────────────────
FROM debian:bookworm-slim AS runtime

LABEL org.opencontainers.image.source="https://github.com/dot-matrix-labs/calypso"
LABEL org.opencontainers.image.description="Calypso CLI — multi-stage release image"
LABEL org.opencontainers.image.licenses="MIT"

# Create a non-root user for running the CLI.
RUN addgroup --system calypso && adduser --system --ingroup calypso calypso

# Copy binary from builder stage and expose it as `calypso`.
COPY --from=builder /build/target/release/calypso-cli /usr/local/bin/calypso

RUN chmod 755 /usr/local/bin/calypso

USER calypso

ENTRYPOINT ["/usr/local/bin/calypso"]
CMD ["--help"]
