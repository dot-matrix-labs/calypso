FROM oven/bun:1-alpine

LABEL org.opencontainers.image.source="https://github.com/dot-matrix-labs/calypso"
LABEL org.opencontainers.image.description="Calypso worker container — headless agent runtime"
LABEL org.opencontainers.image.licenses="MIT"

# Minimal runtime for a headless Bun worker process.
# No build tools, no shell utilities beyond Alpine defaults.
# The worker binary is COPYed in by CI at image build time.
# Sentinel files (/tmp/ready, /tmp/alive) are written by the worker process
# and checked by the Kubernetes liveness and readiness probes.

WORKDIR /app

RUN addgroup -S worker && adduser -S -G worker worker \
    && chown worker:worker /app

USER worker

# Placeholder entrypoint — replaced by CI with the compiled worker bundle.
# On a base image deploy this exits immediately; the pod restarts until CI
# layers the real worker on top and deploys via kubectl set image.
CMD ["bun", "run", "worker.ts"]
