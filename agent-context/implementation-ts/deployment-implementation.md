
# Deployment — Calypso TypeScript Implementation

> This document is the Calypso TypeScript reference implementation for the [Deployment Blueprint](../blueprints/deployment-blueprint.md). The principles, threat model, and patterns in that document apply equally to other stacks. This document covers the concrete realization using Bun, systemd, and GitHub Actions.

---

## Container Packaging

Applications are packaged as immutable Docker containers using a multi-stage approach, and deployed via Kubernetes. The final production image uses the `oven/bun:1-distroless` image to eliminate the shell, package manager, and minimize the attack surface:

```dockerfile
# apps/server/Dockerfile
# STAGE 1: The Builder
FROM oven/bun:1 AS base
WORKDIR /app
COPY package.json bun.lockb ./
RUN bun install --frozen-lockfile
COPY . .

# Build the app explicitly for exact reproducibility
RUN bun build apps/server/index.ts --target bun --outfile dist/server.js

# STAGE 2: The Production Distroless Runner
FROM oven/bun:1-distroless
WORKDIR /app

# Copy the compiled application code
COPY --from=base /app/dist/server.js ./

CMD ["bun", "run", "server.js"]
```

No systemd services, no PM2, no custom restart scripts natively on the host.

## Environment Variables

| File | Contents | In version control? |
|---|---|---|
| `.env` | Production secrets (API keys, DB passwords, signing keys) | No — `.gitignore`d |
| `.env.test` | Test-only credentials, fixture paths | Yes — committed |

## Logging

- **Chronological log:** `stdout` captured by the container orchestrator (e.g., Kubernetes) and aggregated.
- **Unique error log:** `/var/log/calypso/uniques.log` — deduplicated error categories with count and last-seen timestamp (persisted via standard volume mounts if required, or handled by a dedicated service).
- **Rotation:** Managed by the cluster log aggregation facility (e.g., Fluentd, Promtail).

## Browser Error Forwarding

Browser errors are caught via:
- `window.onerror` for synchronous errors
- `window.onunhandledrejection` for promise rejections
- React error boundaries for component tree crashes

All errors POST to `/api/logs` with `{ traceId, error, stack, url, timestamp }`.

## Trace ID

- Generated in the browser at the start of each user action (UUID v4)
- Sent as `X-Trace-Id` request header
- Server middleware extracts and attaches to all log entries for that request
- Returned as `X-Trace-Id` response header

## Build and Deploy

- Browser: `bun build apps/web/index.tsx --outdir dist/web`
- Server: `docker build -f apps/server/Dockerfile -t calypso-server:latest .`
- Deploy: `kubectl apply -f k8s/deployments/server.yaml` (or helm upgrade/etc.)

## Dependency Justification

| Package | Reason | Buy or DIY |
|---|---|---|
| `kubernetes` | Universal container orchestration; required for enterprise deployments | Buy (managed service) |
| UUID generation | Single function; agent generates internal implementation | DIY |
| Error forwarding client | Thin wrapper around fetch; no library needed | DIY |

---

## Antipatterns (TypeScript/Bun-Specific)

- **Hot-reloading `vite dev` loops.** Using hyper-optimized development servers locally instead of deploying the containerized build. Agents prefer reproducible background deployments that precisely match production, omitting human-convenience tooling that breaks environment parity.
