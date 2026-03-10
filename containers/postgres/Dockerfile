FROM postgres:16-alpine

LABEL org.opencontainers.image.source="https://github.com/dot-matrix-labs/calypso"
LABEL org.opencontainers.image.description="Calypso database container — PostgreSQL 16, minimal, volume-mounted"
LABEL org.opencontainers.image.licenses="MIT"

# No additional packages. No shell utilities beyond what Alpine provides.
# No agent access. Network policy enforces internal-only reachability.
# Data lives on a mounted volume — this image is stateless.

# Initialization scripts run once on first volume mount.
# Place *.sql or *.sh files here to set up schema on a virgin volume.
COPY init/ /docker-entrypoint-initdb.d/

# postgres user and data directory are managed by the upstream image.
# POSTGRES_USER, POSTGRES_PASSWORD, POSTGRES_DB must be set at runtime
# via Kubernetes secrets — never hardcoded here.

EXPOSE 5432

HEALTHCHECK --interval=10s --timeout=5s --start-period=20s --retries=5 \
    CMD pg_isready -U "${POSTGRES_USER}" -d "${POSTGRES_DB}" || exit 1
