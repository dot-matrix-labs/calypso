
# Deployment Blueprint

> [!IMPORTANT]
> This blueprint defines how AI-agent-built software is deployed, kept alive, observed, and recovered — from the first demo through production.

---

## Vision

Deployment is the moment a codebase becomes a system. Code that passes every test and satisfies every requirement is worthless if it cannot be started, kept running, observed when it misbehaves, and recovered when it fails. Most deployment complexity exists to manage the gap between the development environment and the production environment. When that gap is eliminated — when the development host, the test environment, and the deployment target are the exact same containerized runtime — deployment reduces to its essential operations: build, containerize, and orchestrate.

The deployment strategy for agent-built software has two fundamental requirements that traditional deployments lack. First, an AI agent must be able to diagnose a production issue by reading logs, not by watching dashboards or receiving pages. Second, human-convenience tooling like hot-reloading (`vite dev`) optimizes for incremental human thought, but agents write full files and prefer exact reproduction. By building the code exactly as it will run in production and running it in a background container for dev previews, we eliminate hybrid environment bugs and drastically reduce the toolchain complexity across environments.

The cost of ignoring this blueprint is a system that works in development and fails in production in ways no one can diagnose. Environments that drift. Errors that repeat ten thousand times and fill the disk while the root cause remains invisible. Deployments that require tribal knowledge that no agent can access. Containerized deployment is not overhead — it is the discipline that makes the system identically operable by any agent in any session, from local dev to enterprise Kubernetes.

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| Application container crashes and is not restarted | Service availability — the container orchestrator must restart crashed containers automatically |
| Server runs out of disk space due to unrotated logs | Host stability — log retention policies must prevent disk exhaustion |
| Error occurs in the browser and is never reported to the server | Observability completeness — browser errors must be captured and forwarded to the server |
| Agent reads logs to diagnose an issue but context window fills with duplicate errors | Diagnostic efficiency — deduplicated error summaries must exist alongside chronological logs |
| Deployment requires manual steps that an agent cannot perform | Deployment autonomy — the full deploy process must be scriptable and non-interactive |
| Environment variables containing secrets are committed to the repository | Secret protection — production secrets must never be in version control |
| A deploy happens with failing tests | Deployment safety — CI must gate all deployments |
| The server is unreachable and no one knows why | Network observability — the container orchestrator and health checks must report status |
| A rollback is needed but the previous version is not available | Rollback capability — previous builds must be recoverable from version control |

---

## Core Principles

### Containers are the great unifier

An application process that is started manually and dies when the session ends is not deployed — it is running. Deployment means the application is packaged as an immutable container image, deployed to an orchestrator, and restarted on crash. We exclusively use containerized services. For enterprise deployments, the most credible architecture is Kubernetes. By enforcing containers across all environments, we reduce the amount of code and tools we need to maintain for hybrid environments and create sane reproductions across dev, test, and prod. For production environments, these containers must be "distroless" — stripped of package managers, shells, and all OS-level tools not strictly required to run the application, drastically reducing the attack surface.

### No incremental hot-reloading dev servers

We will not use tools like `vite dev` to create hot-reloadable, on-the-fly servers. While human developers find the build-and-deploy cycle annoying, agents do not care. We will build the code with our runtime (e.g., Bun) and deploy it in the background to a running container, exactly as if it were production. This enforces environment parity from the first line of code.

### Logs are for machines first

Every log entry must be structured, timestamped, and traceable. A chronological log file serves as the complete record. A deduplicated summary file serves as the diagnostic entry point — an agent reads the summary to understand what categories of errors exist, then dives into the chronological log for specifics. Log formats are designed for parsing, not for reading in a terminal.

### Traces span the full stack

A single user action — clicking a button, submitting a form — generates a trace that follows the request from the browser through the API server to the database and back. Every component in the chain tags its work with the same trace ID. Reconstructing any user workflow is a matter of filtering by trace ID, not correlating timestamps across multiple log files.

### Deployment is a build, not a ceremony

Deploying a new version means building the code, stopping the old process, starting the new process, and verifying it is healthy. These steps are scripted, idempotent, and non-interactive. No SSH session, no manual file copy, no "run these five commands in this order." An agent or a CI pipeline can deploy without human assistance.

### Secrets are runtime configuration, not build artifacts

Environment variables containing secrets (API keys, database passwords, signing keys) are injected at runtime from files on the host that are not in version control. Test-only environment variables may be committed (they contain no production secrets). The boundary is clear: if it would be dangerous in a public repository, it does not go in version control.

---

## Design Patterns

### Pattern 1: Immutable Distroless Container Builds

**Problem:** Applications behave differently in development, test, and production environments due to host-level drift, installed dependencies, or missing system libraries. Furthermore, standard container images contain shells and package managers that drastically expand the attack surface.

**Solution:** Every deployment—including local development previews—starts by building an immutable container image. The container encapsulates the exact runtime, dependencies, and compiled application assets. For the final production artifact, we implement a multi-stage build that copies the compiled application into a "distroless" base image. This final image contains only the application and its runtime, omitting the shell and OS utilities entirely.

**Trade-offs:** Building a container image for every local change is slower than a hot-reloading development server, and debugging distroless images requires more structured logging since you cannot `exec` into a shell. This is a deliberate trade-off: human convenience is discarded in favor of perfect mechanical reproducibility and zero-trust security.

### Pattern 2: Dual-Log Architecture

**Problem:** A chronological log file is complete but overwhelming. An agent diagnosing an issue must read potentially thousands of lines, most of which are duplicates of the same error. The signal-to-noise ratio makes log-based diagnosis expensive in tokens and time.

**Solution:** Maintain two log outputs:
- **Chronological log:** Every event, in order, with full detail. The complete record.
- **Unique error log:** A deduplicated set of error categories currently affecting the system. Each entry appears once regardless of how many times it occurred. Includes a count and the most recent timestamp.

The agent reads the unique log first to understand the error landscape, then consults the chronological log for specific trace IDs or time ranges.

**Trade-offs:** Two log files to manage and rotate. The unique log requires deduplication logic (hashing error signatures). The implementation cost is low; the diagnostic benefit is high.

### Pattern 3: Browser-to-Server Error Forwarding

**Problem:** Errors that occur in the browser — unhandled promise rejections, React error boundaries, DOM exceptions — are invisible to the server. The server logs show a healthy system while users experience failures.

**Solution:** The browser application catches all unhandled errors and forwards them to a server endpoint via HTTP POST. The error payload includes the error message, stack trace, user context, and the current trace ID. The server logs these browser errors alongside its own errors, creating a unified view of system health.

**Trade-offs:** Adds network traffic for error reporting. Errors that occur when the network is down cannot be forwarded (acceptable — the network being down is itself a detectable condition). The error endpoint must be protected against abuse (rate limiting, payload size limits).

### Pattern 4: Trace-ID Propagation

**Problem:** A user reports "it did not work." The developer must reconstruct what happened: which API calls were made, what the server did, what the database returned. Without a shared identifier across all components, reconstruction requires timestamp correlation across multiple log files — which is slow, imprecise, and sometimes impossible.

**Solution:** Generate a unique trace ID at the start of every user-initiated action (page load, form submission, API call). Pass this ID through every layer: browser → API request header → server handler → database query tag → response header → browser. Every log entry includes the trace ID. Reconstructing a workflow is a single filter operation.

**Trade-offs:** Requires discipline to propagate the trace ID through every layer. Missing propagation in one component breaks the chain. Mitigation: the trace ID middleware is implemented once in the server framework and once in the browser HTTP client — individual handlers do not need to manage it.

---

## Plausible Architectures

### Architecture A: Background Dev Container (solo app, early-stage)

```
┌─────────────────────────────────────────────┐
│  Development Host                           │
│                                             │
│  ┌───────────────────────────────────────┐  │
│  │  Container Engine (Docker/Podman)     │  │
│  │                                       │  │
│  │  ┌─────────────────────────────────┐  │  │
│  │  │  App Container (Immutable)      │  │  │
│  │  │  - Serves API routes            │  │  │
│  │  │  - Serves static assets (built) │  │  │
│  │  │  - Writes chronological log     │  │  │
│  │  │  - Writes unique error log      │  │  │
│  │  └─────────────────────────────────┘  │  │
│  └───────────────────────────────────────┘  │
│                                             │
│  .env        ← injected at container start  │
│  .env.test   ← test credentials (committed) │
│  Container Logs ← stdout/stderr             │
└─────────────────────────────────────────────┘
```

**When appropriate:** Local development and preview environments. The runtime serves both API and static assets from a background container. No `vite dev`, no hot reloading on the host.

**Trade-offs:** Slightly slower iteration loop compared to traditional local dev, but guarantees the code running is identical to production packaging.

### Architecture B: Kubernetes Enterprise Deployment (production, scale)

```
┌──────────────────────────────────────────────────┐
│  Kubernetes Cluster                              │
│                                                  │
│  ┌────────────────────────────────────────────┐  │
│  │  Ingress Controller (TLS, routing)         │  │
│  │                                            │  │
│  │  ┌──────────────┐  ┌───────────────────┐  │  │
│  │  │ App Service  │  │ App Pods          │  │  │
│  │  │ (ClusterIP)  │──│ (ReplicaSet)      │  │  │
│  │  │              │  │                   │  │  │
│  │  └──────────────┘  └───────────────────┘  │  │
│  └────────────────────────────────────────────┘  │
│                                                  │
│  ConfigMaps/Secrets ← injected securely          │
└──────────────────────────────────────────────────┘
```

**When appropriate:** Any enterprise-grade deployment. Kubernetes acts as the universal orchestrator for routing, replication, and self-healing.

**Trade-offs:** Adds Kubernetes cluster operational overhead. Justified because Kubernetes is the most credible architecture for enterprise deployments and provides declarative infrastructure APIs perfect for agents.

**When appropriate:** Multiple applications on one host, or any application that needs TLS. The reverse proxy handles TLS termination and routes requests to the correct application by domain or path.

**Trade-offs:** Adds a reverse proxy component to configure and maintain. Justified when TLS or multi-app routing is required. Overkill for a single HTTP-only dev preview.

### Architecture C: CI-Driven Deploy Pipeline (automated, gated)

```
┌────────────────────────────────────────────────────┐
│  CI Platform                                       │
│                                                    │
│  Push to main                                      │
│       │                                            │
│       ▼                                            │
│  All test workflows pass                           │
│       │                                            │
│       ▼                                            │
│  Deploy workflow:                                  │
│    1. Build app bundle (bun build)                 │
│    2. Layer bundle onto base image                 │
│    3. Push image → receive immutable digest        │
│    4. kubectl set image deployment/<name>          │
│       <name>=<image>@<digest>                      │
│    5. kubectl rollout status --timeout=5m          │
│    6. On failure: kubectl rollout undo             │
│       │                                            │
│       ▼                                            │
│  Deployment complete                               │
└────────────────────────────────────────────────────┘
```

**When appropriate:** All deployments — this is the only release mechanism Calypso supports. Every container type (frontend, worker, database) follows this pipeline. The CI service account holds narrow kubectl credentials (patch deployments only); no SSH access to nodes is required.

**Trade-offs:** Requires a Kubernetes cluster and a narrow-scoped CI service account (`k8s/rbac/ci-deployer.yaml`). The `rollout undo` step is automatic on failure — there is no human rollback procedure. Previous revisions are retained by Kubernetes and are immediately available.

---

> For the Calypso TypeScript implementation of these patterns, see [deployment-implementation.md](../implementation-ts/deployment-implementation.md).

## Implementation Checklist

### Alpha Gate

- [ ] `Dockerfile` or container manifest created and application starts via container runtime
- [ ] Container orchestrator (e.g., K8s or local Docker) restarts automatically after crash
- [ ] Secrets injected into container securely (not baked into image)
- [ ] `.env.test` committed with test-only credentials
- [ ] Stdout/stderr captured by container engine; logs accessible via standard container tools
- [ ] Node/Bun process writes structured entries to standard output/error
- [ ] Trace ID generated and propagated browser → server → response header
- [ ] Browser error forwarding implemented; errors appear in server logs
- [ ] Health endpoint (`/health`) returns 200 when the application is running

### Beta Gate

- [ ] `uniques.log` implemented; deduplicated error categories with counts
- [ ] Log rotation configured; 14-day retention verified
- [ ] Deploy script exists and is idempotent (running twice has no side effects)
- [ ] CI deploy workflow created; deploys only after all test suites pass
- [ ] Rollback procedure tested: revert to previous commit, restart, verify health
- [ ] CI service account kubeconfig (`KUBE_CONFIG`) stored in GitHub Secrets; generated via `scripts/setup-ci-deployer.sh`
- [ ] Deployment verified to use immutable image digest (not mutable tag) in `kubectl get deployment -o yaml`
- [ ] Disk usage monitoring; alert when log volume exceeds threshold
- [ ] All environment variables documented in `docs/` with descriptions (not values)

### V1 Gate

- [ ] Zero manual SSH steps required for a standard deploy
- [ ] Health check includes dependency status (database, external APIs reachable)
- [ ] Trace ID search: given a trace ID, all related log entries can be retrieved in one query
- [ ] Browser error rate tracked; anomalous spikes trigger alerts
- [ ] Backup strategy for application data (database dumps, uploaded files)
- [ ] Disaster recovery tested: fresh host provisioned and application deployed from scratch

---

## Antipatterns

- **Hybrid environments.** Developing with hot-reloading dev servers (`vite dev`) locally, but deploying compiled containers in production. This guarantees environment drift and "works on my machine" bugs. Agents do not benefit from hot-reloading. Build the container. Run the container.

- **Process babysitting.** Starting the application with `bun run` in a tmux pane and hoping it does not crash. When it does, no one notices until a user reports downtime. The container orchestrator exists to eliminate this class of failure entirely.

- **Log and pray.** Writing logs to a file without configuring rotation, retention, or aggregation in your orchestrator. The volume fills up. The container crashes because it cannot write. Rely on container `stdout` and cluster-level log aggregation.

- **Dashboard-only observability.** Building a monitoring dashboard that a human must watch. An AI agent cannot watch a dashboard. Observability for agent-operated systems means structured logs and error summaries that an agent can fetch via CLI tools over the cluster namespace.

- **Manual deploy rituals.** A deployment that requires running a sequence of commands from memory. Scripted container builds and declarative Kubernetes applies are repeatable, auditable, and agent-executable.

- **Silent browser errors.** Catching browser errors in `console.error` and assuming someone will see them. No one sees browser console output in production. Errors that are not forwarded to the server are errors that do not exist from the system's perspective.
