# Environment Blueprint

<!-- last-edited: 2026-03-10 -->

CONTEXT MAP
  this ◀──implemented by── implementation-ts/environment-implementation.md
  this ◀──requires────────── blueprints/deployment-blueprint.md (deploy pipeline)
  this ◀──referenced by──── index.md

> [!IMPORTANT]
> This blueprint defines the environment model for AI-agent-driven software projects: what containers run, what they are allowed to do, how the cluster is provisioned, and why the three-container app topology is the same in development and production.

---

## Vision

The promise of AI-led development is that the agent does not just write code — it designs and operates the full system from the first commit. This promise breaks immediately if the environment the agent develops in differs from the environment the software runs in. A staging server with hand-installed packages, a production cluster configured differently by a human operator: each gap is a place where the software will silently stop working. Agents are worse at detecting these gaps than experienced human engineers, because agents cannot see the physical machine, cannot smell that something is wrong, and will confidently produce work that passes every test in the wrong environment.

Calypso eliminates the gap by collapsing development and production into the same container topology from the first day. The frontend container, the worker container, and the database container that run during a prototype session are the same containers — same base images, same constraints, same network rules — that run in production. When the agent builds a UI for a business process, it is not creating a throwaway demo. It is designing the full production system for free. The prototype and the production artifact are the same build, tagged and released through the same pipeline.

The development environment is the cloud host itself. The agent and developer work directly on the host OS — the same machine that runs the K8s cluster. There is no developer container inside the cluster. The IDE connects to the host via SSH (or a remote development extension). This simplifies the environment without sacrificing parity: the app containers the agent builds and tests against are the same containers that run in production.

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| Frontend container is used to build or compile code at runtime | Release integrity — every artifact served in production must have been vetted, tested, and released before the server sees it |
| Database container is modified or queried by the agent directly | Data integrity and audit trail — agents must not have direct access to the database process or its host |
| Agent installs packages or modifies global state on the frontend or database container | Container immutability — non-developer containers must be immutable; unexpected mutations indicate a compromised or misconfigured system |
| Cluster is provisioned with environment-specific configuration differences between demo and production | Topology parity — a cluster that behaves differently in demo mode versus production mode is two different systems pretending to be one |
| Agent runs on the developer's local laptop instead of the cloud host | Headless integrity and environment parity — local environments reintroduce all the divergence that a shared cloud host eliminates; agents belong on the cloud host |
| Cluster is destroyed and must be reprovisioned | State durability — all non-ephemeral state must live in version control or the database volume, never on the host filesystem |
| A release is deployed to the frontend without passing CI and the release pipeline | Release gate integrity — the frontend must not be configurable to pull untagged, untested, or unreleased artifacts |
| Agent session drops mid-task due to SSH timeout or network interruption | Session continuity — in-flight agent context and partially applied changes must survive disconnection (tmux on the host) |
| Integration or end-to-end test connects to the live database instead of an ephemeral test instance | Data integrity — test runs must never read from or write to the production or demo database |
| Ephemeral test container is left running after a test suite completes | Host resource integrity — leaked containers exhaust disk, memory, and port space on the host |

---

## Core Principles

### The prototype is the production system

The container topology that runs during the first demo session is the same topology that runs in production. There are no placeholder components, no "we'll do it properly later" shortcuts, and no environment-specific configurations. Every decision made in a prototype session is a production decision. This is not a constraint — it is the core value proposition. When the prototype is done, the production system is done.

### Containers are role-specialized and capability-constrained

Each container type exists for exactly one role and has only the capabilities required for that role. The frontend container can serve pre-built release bundles. The database container can store and retrieve data. The worker container can run AI task daemons. No container has capabilities that belong to another role. A container that can do more than its role requires is a container that can fail in more ways than its role implies.

### Building from source is a host-only capability

Compilation, bundling, transpilation, dependency installation, and any other transformation of source code into a deployable artifact happens exclusively on the development host. The frontend container receives only tagged, tested, released artifacts. It cannot build from source because it does not have the tools, and it must not have the tools. Building in production is an antipattern regardless of whether "production" means a customer deployment or a demo to a single stakeholder.

### AI coding assistants run on the host; AI workers run in the worker container

Two distinct categories of AI process exist and must not be conflated. AI coding assistants — Claude Code, Gemini CLI, Codex, and equivalent interactive LLM tools — run directly on the cloud host. They write code, run tests, push releases, and manage infrastructure. They do not run on the frontend container, the worker container, the database container, or the developer's local device.

AI workers are a separate category: long-running daemon processes that consume tasks from a queue and call AI vendor APIs or vendor CLI binaries to perform production AI work. They run in the worker container, not on the host. The worker container is purpose-built for this role — minimal, distroless-style, no shell. Placing a worker daemon on the host, or a coding assistant in the worker container, violates the capability constraints that both are designed to enforce.

### Test databases are ephemeral and isolated from all persistent data

Every integration test and end-to-end test that requires a database runs against a fresh, disposable database container spun up by the test runner and torn down when the suite completes. This container is not the cluster database. It has no connection to the cluster database. It is created with no data, seeded by the test, exercised by the test, and destroyed. The cluster database is never a valid target for a test run, under any circumstances.

### The environment is provisioned by the agent, not the developer

The developer does not manually configure servers, install software, or wire together containers. The agent, given a cloud API key (or starting from an already-provisioned host), runs `scripts/provision-cluster.sh` to install k3s, apply all manifests, and produce a running app cluster. The provisioning process is a first-class artifact — versioned, testable, and re-runnable. A system that cannot be reprovisioned from scratch in one command is a system with undocumented state.

---

## Design Patterns

### Pattern 1: Immutable Release Artifact

**Problem:** Software deployed to the frontend must be known-good before it arrives. A frontend that can pull arbitrary code — from the main branch, from a development server, from a local machine — is a frontend that can serve untested code.

**Solution:** The host builds a release artifact (a compiled bundle), pushes it through the standard CI pipeline, passes all automated tests, and tags a version on the version control host. The frontend container is notified of the new release tag via CI and downloads the artifact from the release registry. The frontend has no credentials to the version control system and no build tooling. Its only capability is fetching a named version and serving it.

**Trade-offs:** Adds a mandatory release step between "code compiles" and "code is visible in the browser." For rapid iteration this feels slow, but the pipeline is fast by design (pre-built artifact, not build-on-deploy). The overhead is the correct feedback mechanism: if the release pipeline is too slow to support iteration, the pipeline needs to be optimized, not bypassed.

### Pattern 2: Three-Container Separation of Concerns

**Problem:** Combining serving capabilities, AI work, and data storage in a single runtime or undifferentiated containers makes it impossible to enforce capability constraints and impossible to scale or replace components independently.

**Solution:** Three purpose-built container types, each with a minimal image, minimal capability set, and a single responsibility:

- **Frontend Container:** minimal base image (not a full OS), a single runtime, a single entry point. Serves pre-built release bundles on a designated port. Cannot install packages, cannot execute build steps, cannot write to persistent volumes.
- **Worker Container:** minimal image with Bun runtime and vendor CLI binaries. Runs AI task daemons that consume from the task queue and call AI vendor APIs. No shell access. Read-only access to task queue views in the database. Cannot write to the database directly — all writes go through the API.
- **Database Container:** distroless base image, database binary and dependencies only. Volume-mounted for persistence. No shell, no package manager, no direct agent access. Backed up on a schedule to durable object storage.

The development environment is the cloud host OS itself — not a fourth container. The host runs the agent, the build toolchain, the version control client, and Docker. It can spin up ephemeral test containers via Docker directly (no Docker-in-Docker required).

**Trade-offs:** Three containers require a container orchestrator. This is not a cost — it is an explicit design choice that brings network policy enforcement, restart behavior, health checking, and scaling as standard features.

### Pattern 3: Ephemeral Test Containers

**Problem:** Integration tests and end-to-end tests require real infrastructure — a running database, a seeded schema, realistic data volumes — but must not touch the cluster database, which holds real or demo data.

**Solution:** The host runs Docker directly. The test runner starts a fresh database container before the suite, exposes it on a randomized local port, runs all tests against it, and stops and removes the container when the suite exits — whether it passes or fails. The cluster database is unreachable from the host at the network level: no hostname, no credentials, no route. This is enforced by Kubernetes network policy, not by convention.

The ephemeral test container uses the same image as the cluster database container. Schema migrations are applied from scratch at test startup. This means the test suite also validates that migrations run cleanly against a virgin database.

**Trade-offs:** The host must have Docker installed and running. Teardown must happen in a finally block — unconditionally — regardless of test outcome.

### Pattern 4: Agent-Provisioned Cluster

**Problem:** Manual infrastructure provisioning is undocumented, non-reproducible, and not auditable. Every manually provisioned server is a unique artifact with undocumented state.

**Solution:** The agent runs `scripts/provision-cluster.sh` which: optionally creates a compute instance (or runs locally if already on the host), bootstraps k3s, deploys all three container types from their template images, configures networking and ingress, and outputs the cluster endpoint. The provisioning script is checked into version control. Running it again produces an identical cluster. The developer's only manual action is providing the API key (if provisioning a new host remotely).

**Trade-offs:** When provisioning a new remote host, the agent must have write access to the cloud account. This is a privileged operation and should be time-bounded: the API key used for provisioning should be revocable after the cluster is running.

### Pattern 5: Remote-First IDE Attachment

**Problem:** Running a code editor or IDE locally against remote files introduces platform-specific behavior (line endings, symlinks, file watcher semantics) and bypasses the host's toolchain entirely.

**Solution:** The developer's local IDE connects to the cloud host over SSH and mounts the host's filesystem as its workspace. The IDE runs its language server, linter, and formatter on the host, not on the local device. Agent CLIs (LLM tools) run on the host, not in a local terminal. The local device is a viewport — keyboard, mouse, and display — not a development environment. VS Code Remote SSH, Cursor Remote SSH, and JetBrains Gateway are all supported.

**Trade-offs:** Requires the IDE to support remote development over SSH (most modern editors do). Network latency affects editor responsiveness; this is an argument for locating the host in the nearest cloud region, not an argument for local development.

### Pattern 6: Orchestrator-Driven Rolling Release

**Problem:** Deploying a new version of any container must be zero-downtime, automatically verified against health checks, and automatically rolled back on failure.

**Solution:** The container orchestrator owns the entire release lifecycle. CI builds a new image, pushes it to the registry, and receives an immutable digest. CI then patches the target Deployment or StatefulSet with that digest via a narrow-scoped service account. The orchestrator performs a rolling update: it starts a new pod, waits for its readiness probe to pass, then terminates an old pod. If any new pod fails its readiness probe before the rollout deadline, the orchestrator halts the rollout and CI triggers a rollback to the previous revision.

**Trade-offs:** Requires a running Kubernetes cluster and a kubeconfig for the CI service account. The rollout deadline must be tuned per container type.

---

## Plausible Architectures

### Architecture A: Single-Node Kubernetes Cluster (solo project)

```
┌─────────────────────────────────────────────────────────────────┐
│  Cloud Host (Ubuntu 24.04 LTS)                                  │
│                                                                 │
│  Host OS — agent CLIs, build tools, git, gh, bun, tmux, docker │
│  ← Agent/developer works here directly over SSH                 │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  Container Orchestrator (k3s, single-node)              │    │
│  │                                                         │    │
│  │  ┌───────────────────────┐                              │    │
│  │  │  Frontend Container   │  ← Serves tagged releases   │    │
│  │  │  (minimal image)      │    K8s rolling update only   │    │
│  │  │  Port: 443 / 80       │    No build tooling          │    │
│  │  └───────────────────────┘                              │    │
│  │                                                         │    │
│  │  ┌───────────────────────┐                              │    │
│  │  │  Worker Container     │  ← AI task daemon           │    │
│  │  │  (minimal+bun+CLIs)   │    Reads task queue (RO)    │    │
│  │  │  No shell             │    Writes via API only       │    │
│  │  └───────────────────────┘                              │    │
│  │                                                         │    │
│  │  ┌───────────────────────┐                              │    │
│  │  │  Database Container   │  ← Distroless, no shell     │    │
│  │  │  (distroless image)   │    Volume-mounted data       │    │
│  │  │  Internal network only│    Scheduled volume backup   │    │
│  │  └───────────────────────┘                              │    │
│  │                                                         │    │
│  │  Internal network: containers communicate by service    │    │
│  │  External exposure: frontend port only                  │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘

  Local Device (developer)
  ┌────────────────────┐
  │  IDE (SSH remote)  │──── SSH ──→  Cloud Host OS
  │  Browser           │──── HTTPS ─→ Frontend Container
  └────────────────────┘
```

**When appropriate:** Single developer or single agent working on a project. Cost-minimal — one instance. The topology is identical to multi-node production; only the physical distribution differs.

**Trade-offs vs. other architectures:** No redundancy — if the node fails, everything fails. Acceptable for a single-node deployment because all durable state is in version control and the database volume backup. Not appropriate once the application serves real end users.

---

### Architecture B: Multi-Node Cluster (team)

```
┌──────────────────────────────────────────────────────────────────┐
│  Container Orchestrator (multi-node)                             │
│                                                                  │
│  ┌──────────────────┐   ┌──────────────────┐                     │
│  │  Dev Host A      │   │  Dev Host B      │                     │
│  │  (cloud host OS) │   │  (cloud host OS) │  ← Multiple agents  │
│  │  SSH endpoint    │   │  SSH endpoint    │    one per host     │
│  └──────────────────┘   └──────────────────┘                     │
│                                                                  │
│  ┌─────────────────────────────────────────┐                     │
│  │  Frontend Tier (replicated)             │                     │
│  │  ┌─────────────┐   ┌─────────────┐      │                     │
│  │  │  Frontend   │   │  Frontend   │  ← Load-balanced          │
│  │  │  Container  │   │  Container  │    release serving         │
│  │  └─────────────┘   └─────────────┘      │                     │
│  └─────────────────────────────────────────┘                     │
│                                                                  │
│  ┌─────────────────────────────────────────┐                     │
│  │  Worker Tier (replicated per type)      │                     │
│  │  ┌─────────────┐   ┌─────────────┐      │                     │
│  │  │  Worker     │   │  Worker     │  ← One deployment         │
│  │  │  (coding)   │   │  (analysis) │    per worker type         │
│  │  └─────────────┘   └─────────────┘      │                     │
│  └─────────────────────────────────────────┘                     │
│                                                                  │
│  ┌─────────────────────────────────────────┐                     │
│  │  Database Node                          │                     │
│  │  ┌─────────────────────────┐            │                     │
│  │  │  Database Container     │            │  ← Primary + replica│
│  │  │  Primary + Replica      │            │    Volume to tape   │
│  │  └─────────────────────────┘            │                     │
│  └─────────────────────────────────────────┘                     │
└──────────────────────────────────────────────────────────────────┘
```

**When appropriate:** Multiple agents working on the same project in parallel. Web tier must handle real traffic. Database requires a replica for read scaling or failover. Each developer gets their own host to eliminate toolchain interference.

**Trade-offs vs. Architecture A:** Higher cost. Requires networking between nodes. Database replication and consensus must be configured correctly. But these are production requirements, not engineering overhead — this architecture is what production looks like, so moving from Architecture A to Architecture B is a scaling exercise, not a redesign.

---

## Reference Implementation — Calypso TypeScript

> The following is the Calypso TypeScript reference implementation. The principles and patterns above apply equally to other stacks; this section illustrates one concrete realization using TypeScript, Bun, React, and PostgreSQL.

### Container Images

Calypso provides three base images for the app cluster, published to the project's container registry. Projects derive from these images without modifying them unless a blueprint-documented reason exists.

| Image | Base | Installed | Not Installed |
|---|---|---|---|
| `calypso/frontend` | Alpine minimal | `bun` (runtime only) | `apt`, `npm`, `git`, `gh`, build tools, shells |
| `calypso/worker` | Minimal + Node | `bun`, `node`, vendor CLI binaries (`claude`, `gemini`, `codex`) | Shell, `apt`, `git`, `gh`, build tools |
| `calypso/postgres` | Distroless | PostgreSQL binary and libs | Everything else |

The host itself provides the development toolchain: `claude`, `gemini`, `codex`, `bun`, `node`, `npm`, `git`, `gh`, `bash`, `tmux`, `docker`, `kubectl`.

### Bootstrap Workflow

```
1. User SSH's into cloud host (or uses VS Code Remote SSH / similar)
2. Agent runs on the host: reads scaffold-task.md and begins Step 1
3. Agent verifies host tools, starts tmux session
4. Agent runs: scripts/provision-cluster.sh
   - Optionally creates a new Droplet (if DIGITALOCEAN_TOKEN provided)
   - Installs k3s on the host (or target host)
   - Applies manifests from k8s/ directory (frontend, worker, db)
   - Outputs cluster health summary
5. Agent continues with GitHub setup, cloning, and scaffolding
```

### Kubernetes Manifest Structure

```
k8s/
  namespace.yaml              ← project namespace
  network-policy.yaml         ← inter-container network rules
  ingress.yaml                ← external TLS ingress
  rbac/
    ci-deployer.yaml          ← CI service account (patch deployments only)
  secrets/
    postgres-credentials.sh   ← secret creation script
    worker-credentials.sh     ← secret creation script
  frontend/
    deployment.yaml           ← frontend (RollingUpdate, maxUnavailable=0)
    service.yaml              ← ClusterIP
  worker/
    deployment.yaml           ← worker template (copy per worker type)
  db/
    statefulset.yaml          ← postgres StatefulSet
    service.yaml              ← internal ClusterIP only
```

### Release Pipeline

All container types follow the same two-stage release model:

1. **Base image** (`containers/<type>/Dockerfile`) — rebuilt when runtime dependencies change (bun version, OS packages, vendor CLI binaries). Rare. Tagged `base-latest`.
2. **Release overlay** (`apps/<type>/Dockerfile.release`) — layers the compiled application bundle onto the current base image. Rebuilt on every merge to main. Tagged with the immutable SHA-256 digest.

CI deploys by patching the Deployment or StatefulSet image to the new digest:

```
kubectl set image deployment/frontend \
  frontend=ghcr.io/.../frontend@sha256:<digest>
kubectl rollout status deployment/frontend --timeout=5m
# On failure: kubectl rollout undo deployment/frontend
```

### Provisioning Script Interface

```typescript
// scripts/provision-cluster.ts
interface ProvisionConfig {
  provider: "local" | "digitalocean" | "hetzner" | "vultr";
  region?: string;
  nodeSize?: string;
  projectName: string;
  registryCredentials: string; // base64 encoded
}
```

### Dependency Justification

| Package / Tool | Reason to Buy | Justified |
|---|---|---|
| `kubectl` / `helm` | Kubernetes is complex; the CLI is the canonical control plane interface | Yes — Buy |
| `doctl` (DigitalOcean CLI) | Cloud provider API surface is large; official CLI is the supported interface | Yes — Buy |
| Bun (frontend runtime) | Consistent with project standard; fast cold starts for minimal containers | Yes — Buy |
| GitHub Actions (CI) | Release pipeline must run outside the host; hosted CI is the standard | Yes — Buy |
| PostgreSQL (distroless image) | Standard relational database; distroless image eliminates shell-based attack surface | Yes — Buy |

---

## Implementation Checklist

- [ ] Cloud host provisioned and accessible via SSH; agent running directly on the host
- [ ] `scripts/provision-cluster.sh` executed; cluster endpoint reachable via `kubectl`
- [ ] All three container types running and healthy per `kubectl get pods`
- [ ] Agent CLI (`claude`, `gemini`, or equivalent) running on the host, not on the developer's local device
- [ ] Frontend container serving a release bundle at the designated external port; RELEASE_TAG in `/health` response matches the deployed git SHA
- [ ] Worker container running and claiming tasks from the task queue; submitting results via API
- [ ] Database container running and accepting connections from frontend and worker containers only; not exposed externally
- [ ] `tmux` session active on host; SSH disconnect and reattach tested
- [ ] Agent has read all files in `agent-context/` before writing any code
- [ ] Release pipeline configured: push to main triggers CI, CI builds release overlay image, CI patches deployment with immutable digest, rollout completes within timeout
- [ ] Rollback tested: deploy a bad image (readiness probe fails), confirm CI runs `kubectl rollout undo`, old pods resume serving
- [ ] Database volume backup scheduled and tested; restore procedure documented and executed at least once
- [ ] Firewall rules verified: only frontend port and host SSH port reachable externally
- [ ] Frontend container image verified to contain no build tooling (`git`, `npm`, `bun install`, `tsc` absent)
- [ ] Database container verified to have no shell access (`kubectl exec` into db container fails as expected)
- [ ] Integration test suite spins up an ephemeral database container (via `docker run` on the host), runs to completion, and tears it down — confirmed via `docker ps` showing no residual containers after the suite exits
- [ ] Network policy verified: host cannot reach the cluster database service by hostname or IP from inside the app cluster
- [ ] Test suite connection string verified to point at the ephemeral container port, not any cluster service
- [ ] Provisioning script idempotent: running it twice produces a clean cluster without manual cleanup
- [ ] Cluster reprovisioned from scratch; new cluster reaches ready state without manual steps
- [ ] Multi-node cluster deployed with frontend replicated across at least two nodes
- [ ] Database replica configured; failover tested
- [ ] Cluster monitoring active: container restarts, disk pressure, memory pressure all generate alerts
- [ ] Rollback verified automatic: `progressDeadlineSeconds` exceeded triggers CI failure; `kubectl rollout undo` restores previous revision without human intervention
- [ ] Recovery drill completed: cluster destroyed, reprovisioned, database volume restored; end-to-end time measured and within SLA

---

## Antipatterns

- **Agent running on the developer's local laptop.** When the agent runs locally, it inherits the local operating system, local filesystem, and local toolchain. Every output it produces may silently encode local assumptions. The agent belongs on the cloud host.

- **IDE running against local files.** Using an IDE in local mode against a local checkout of the repository bypasses the host's toolchain. Files edited locally may have different line endings, symlink behavior, or import resolution than files edited on the host. The IDE must connect to the cloud host via SSH remote.

- **Frontend container with build tools installed.** Adding `npm`, `bun install`, `tsc`, or any build capability to the frontend container turns it into a shadow development environment with no CI gate. Code built inside the frontend has not been tested. A frontend that can build from source can serve untested code.

- **Agents accessing the database container directly.** An agent that connects to the database process directly — whether through a shell, through an admin client, or through a root-level credential — can make schema changes, data mutations, and configuration changes with no audit trail and no review gate. Agents interact with the database through the application's data layer only.

- **Environment-specific configuration branches.** Creating configuration files, environment variables, or code paths that behave differently in "development mode" versus "production mode" reintroduces the environment delta. There is one mode. Code that needs a flag to determine its environment is code that does not know where it is running.

- **Manual cluster provisioning.** Clicking through a cloud provider's web console, running ad-hoc CLI commands, or following a written runbook to provision the cluster creates undocumented state. The next time the cluster must be provisioned — whether due to failure, scaling, or migration — the process will produce a different result. Provisioning is code.

- **Serving from the main branch.** Configuring the frontend to pull and serve the latest commit from the main branch eliminates the release gate entirely. The frontend serves tagged releases only.

- **Skipping the release pipeline for "just a demo."** A demo is a production event. Code served at a demo that has not passed CI, has not been tested, and has not been released is code that might fail during the demo. The pipeline is not a formality for demos; it is the mechanism that makes demos reliable.

- **Tests running against the cluster database.** Pointing integration or end-to-end tests at the cluster database — even "just this once" or "it's only demo data" — eliminates the guarantee that test runs are non-destructive. Ephemeral test containers exist precisely so this choice never has to be made.

- **Ephemeral test containers not torn down on failure.** A test runner that spins up a database container but only tears it down on success will accumulate zombie containers on the host every time a test fails. Over a long development session this exhausts ports, disk, and memory. Teardown must happen in a finally block — unconditionally — regardless of test outcome.

- **Local port-forwarding as a substitute for the frontend container.** Forwarding a local development server port to a browser — via SSH tunnel, ngrok, or a similar tool — is not a preview environment. It is a local server with production traffic pointed at it. It has no release gate, no deployment artifact, and no parity with the actual frontend container.
