FROM oven/bun:1-alpine

LABEL org.opencontainers.image.source="https://github.com/dot-matrix-labs/calypso"
LABEL org.opencontainers.image.description="Calypso frontend container — serves pre-built release bundle, no build capability"
LABEL org.opencontainers.image.licenses="MIT"

# The built bundle is COPYed in by CI at image build time.
# This container has no build tools, no package manager, no git, no shell.
# All release lifecycle is managed by the container orchestrator (Kubernetes).

ARG RELEASE_TAG=unknown
ENV RELEASE_TAG=${RELEASE_TAG}

WORKDIR /app

COPY server.ts ./server.ts
# Placeholder bundle — serves "calypso provisioning..." until CI layers the real app.
# CI overwrites dist/ via Dockerfile.release on every app release.
COPY dist/ ./dist/

RUN addgroup -S frontend && adduser -S -G frontend frontend
RUN chown -R frontend:frontend /app

USER frontend

EXPOSE 8080

HEALTHCHECK --interval=15s --timeout=5s --start-period=10s --retries=3 \
    CMD wget -qO- http://localhost:8080/health || exit 1

CMD ["bun", "run", "server.ts"]
