
# Environment Blueprint

> [!IMPORTANT]
> This blueprint defines the environment model for AI-agent-driven software projects: what containers run, what they are allowed to do, how the cluster is provisioned, and why the development environment and the production environment are the same thing.

---

## Vision

The promise of AI-led development is that the agent does not just write code — it designs and operates the full system from the first commit. This promise breaks immediately if the environment the agent develops in differs from the environment the software runs in. A developer container on a laptop, a staging server with hand-installed packages, a production cluster configured differently by a human operator: each gap is a place where the software will silently stop working. Agents are worse at detecting these gaps than experienced human engineers, because agents cannot see the physical machine, cannot smell that something is wrong, and will confidently produce work that passes every test in the wrong environment.

Calypso eliminates the gap by collapsing development and production into the same container topology from the first day. The developer container, the frontend container, the worker container, and the database container that run during a prototype session are the same containers — same base images, same constraints, same network rules — that run in production. There is no "works on my machine" because the machine is always the same machine: a container orchestrated by the same runtime that production uses. When the agent vibe-codes a UI for a business process, it is not creating a throwaway demo. It is designing the full production system for free. The prototype and the production artifact are the same build, tagged and released through the same pipeline.

This model also eliminates an entire category of developer-experience engineering. Local development tooling, hot-reload servers, environment-specific feature flags, "devmode" database connections, ngrok tunnels, and staging environments are all symptoms of a broken environment model. They exist because development and production were allowed to diverge. Calypso does not invest in making divergence comfortable. It invests in making divergence impossible. Best practices are enforced from the first commit not because best practices feel good, but because allowing exceptions at prototype stage means rewriting the system when those exceptions compound. Agents do not benefit from environment-specific affordances the way human developers do — they benefit from a simple, consistent, fully-specified world. Simplicity is the agent's native environment.

The cost of ignoring this blueprint is the compounding cost of complexity. Teams that allow development environments to diverge from production spend progressively more time managing that divergence instead of building features. Agents operating in inconsistent environments will produce inconsistent software. A system that "worked in the demo" but requires a week of ops work before it can go live is a system where the demo's value was never real.

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| Agent develops with tools or runtimes not present in production | Production parity — a capability that exists only in the dev container must not reach production-bound code |
| Frontend container is used to build or compile code at runtime | Release integrity — every artifact served in production must have been vetted, tested, and released before the server sees it |
| Database container is modified or queried by an agent directly | Data integrity and audit trail — agents must not have direct access to the database process or its host |
| Agent installs packages or modifies global state on the frontend or database container | Container immutability — non-developer containers must be immutable; unexpected mutations indicate a compromised or misconfigured system |
| Cluster is provisioned with environment-specific configuration differences between demo and production | Topology parity — a cluster that behaves differently in demo mode versus production mode is two different systems pretending to be one |
| Developer runs agent tooling locally instead of in the developer container | Headless integrity and environment parity — local environments reintroduce all the divergence that containerization eliminates |
| IDE or editor runs locally and connects to local files instead of the remote container | Convention contamination — local file edits bypass the container's toolchain and may introduce platform-specific artifacts |
| Cluster is destroyed and must be reprovisioned | State durability — all non-ephemeral state must live in version control or the database volume, never on the container filesystem |
| A release is deployed to the frontend without passing CI and the release pipeline | Release gate integrity — the frontend must not be configurable to pull untagged, untested, or unreleased artifacts |
| Agent session drops mid-task due to SSH timeout or network interruption | Session continuity — in-flight agent context and partially applied changes must survive disconnection |
| Integration or end-to-end test connects to the live database instead of an ephemeral test instance | Data integrity — test runs must never read from or write to the production or demo database |
| Developer container is granted network access to the cluster database service | Database isolation — the dev container's network policy must make the cluster database unreachable, even accidentally |
| Ephemeral test container is left running after a test suite completes | Host resource integrity — leaked containers exhaust disk, memory, and port space on the developer node |

---

## Core Principles

### The prototype is the production system

The container topology that runs during the first demo session is the same topology that runs in production. There are no placeholder components, no "we'll do it properly later" shortcuts, and no environment-specific configurations. Every decision made in a prototype session is a production decision. This is not a constraint — it is the core value proposition. When the prototype is done, the production system is done.

### Containers are role-specialized and capability-constrained

Each container type exists for exactly one role and has only the capabilities required for that role. The developer container can run agents, build artifacts, and push to version control. The frontend container can serve pre-built release bundles. The database container can store and retrieve data. No container has capabilities that belong to another role. A container that can do more than its role requires is a container that can fail in more ways than its role implies.

### Building from source is a developer-only capability

Compilation, bundling, transpilation, dependency installation, and any other transformation of source code into a deployable artifact happens exclusively in the developer container. The frontend container receives only tagged, tested, released artifacts. It cannot build from source because it does not have the tools, and it must not have the tools. Building in production is an antipattern regardless of whether "production" means a customer deployment or a demo to a single stakeholder.

### AI coding assistants run in the developer container; AI workers run in the worker container

Two distinct categories of AI process exist in the cluster and must not be conflated. AI coding assistants — Claude Code, Gemini CLI, Codex, and equivalent interactive LLM tools — run inside the developer container. They write code, run tests, push releases, and manage infrastructure. They do not run on the frontend container, the worker container, the database container, or the developer's local device.

AI workers are a separate category: long-running daemon processes that consume tasks from a queue and call AI vendor APIs or vendor CLI binaries to perform production AI work. They run in the worker container, not the developer container. The worker container is purpose-built for this role — minimal, distroless-style, no shell. The developer container is purpose-built for the coding assistant role — full OS, full toolchain, SSH endpoint. Placing a worker daemon in the developer container, or a coding assistant in the worker container, violates the capability constraints that both containers are designed to enforce.

### Test databases are ephemeral and isolated from all persistent data

Every integration test and end-to-end test that requires a database runs against a fresh, disposable database container spun up by the test runner and torn down when the suite completes. This container is not the cluster database. It has no connection to the cluster database. It has no access to the cluster database's network. It is created with no data, seeded by the test, exercised by the test, and destroyed. The cluster database — whether it holds demo data, early user data, or production data — is never a valid target for a test run, under any circumstances, including convenience, speed, or "the data is not sensitive yet."

### The environment is provisioned by the agent, not the developer

The developer does not manually configure servers, install software, or wire together containers. The agent, given a cloud API key, provisions the cluster, configures the container orchestrator, and produces a running system. The provisioning process is a first-class artifact — versioned, testable, and re-runnable. A system that cannot be reprovisioned from scratch in one command is a system with undocumented state.

---

## Design Patterns

### Pattern 1: Immutable Release Artifact

**Problem:** Software deployed to the frontend must be known-good before it arrives. A frontend that can pull arbitrary code — from the main branch, from a development server, from a local machine — is a frontend that can serve untested code.

**Solution:** The developer container produces a release artifact (a built bundle), pushes it through the standard CI pipeline, passes all automated tests, and tags a version on the version control host. The frontend container is notified of the new release tag via a webhook or polling mechanism and downloads the artifact from the release registry. The frontend has no credentials to the version control system and no build tooling. Its only capability is fetching a named version and serving it.

**Trade-offs:** Adds a mandatory release step between "code compiles" and "code is visible in the browser." For rapid iteration this feels slow, but the pipeline is fast by design (pre-built artifact, not build-on-deploy). The overhead is the correct feedback mechanism: if the release pipeline is too slow to support iteration, the pipeline needs to be optimized, not bypassed.

### Pattern 2: Four-Container Separation of Concerns

**Problem:** Combining development capabilities, serving capabilities, AI work, and data storage in a single runtime or undifferentiated containers makes it impossible to enforce capability constraints and impossible to scale or replace components independently.

**Solution:** Four purpose-built container types, each with a minimal image, minimal capability set, and a single responsibility:

- **Developer Container:** full operating system, AI coding assistant CLIs, language runtimes, build tools, version control client, cloud CLI, and a Docker daemon (Docker-in-Docker). Can write code, run tests, spin up ephemeral test containers, build artifacts, push releases. Cannot serve production traffic. Cannot reach the cluster database over the network.
- **Frontend Container:** minimal base image (not a full OS), a single runtime, a single entry point. Serves pre-built release bundles on a designated port. Cannot install packages, cannot execute build steps, cannot write to persistent volumes.
- **Worker Container:** minimal image with Bun runtime and vendor CLI binaries. Runs AI task daemons that consume from the task queue and call AI vendor APIs. No shell access. Read-only access to task queue views in the database. Cannot write to the database directly — all writes go through the API.
- **Database Container:** distroless base image, database binary and dependencies only. Volume-mounted for persistence. No shell, no package manager, no direct agent access. Backed up on a schedule to durable object storage.

**Trade-offs:** Four containers require a container orchestrator. This is not a cost — it is an explicit design choice that brings network policy enforcement, restart behavior, health checking, and scaling as standard features. The alternative (fewer, larger containers) trades these features for a simpler mental model that breaks as soon as the system grows.

### Pattern 3: Ephemeral Test Containers

**Problem:** Integration tests and end-to-end tests require real infrastructure — a running database, a seeded schema, realistic data volumes — but must not touch the cluster database, which holds real or demo data.

**Solution:** The developer container runs a Docker daemon (Docker-in-Docker). The test runner starts a fresh database container before the suite, exposes it on a randomized local port, runs all tests against it, and stops and removes the container when the suite exits — whether it passes or fails. The cluster database is unreachable from the developer container at the network level: no hostname, no credentials, no route. This is enforced by Kubernetes network policy, not by convention.

The ephemeral test container uses the same image as the cluster database container. Schema migrations are applied from scratch at test startup. This means the test suite also validates that migrations run cleanly against a virgin database — a property that is otherwise easy to lose as a schema evolves.

**Trade-offs:** Docker-in-Docker requires elevated container privileges and careful configuration to avoid container escape. The developer container must be explicitly granted the capability to run nested containers, and this capability must be audited and limited. It adds complexity to the developer container image. This cost is accepted because the alternative — tests that touch the cluster database — is categorically unacceptable.

### Pattern 4: Agent-Provisioned Cluster

**Problem:** Manual infrastructure provisioning is undocumented, non-reproducible, and not auditable. Every manually provisioned server is a unique artifact with undocumented state.

**Solution:** The agent, starting from a cloud API key, runs a provisioning script that: creates a compute instance, bootstraps a container orchestrator (Kubernetes or equivalent), deploys all four container types from their template images, configures networking and ingress, and outputs the cluster endpoint. The provisioning script is checked into version control. Running it again produces an identical cluster. The developer's only manual action is providing the API key.

**Trade-offs:** The agent must have write access to the cloud account during provisioning. This is a privileged operation and should be time-bounded: the API key used for provisioning should be revocable after the cluster is running. Ongoing cluster operations use narrower-scoped credentials.

### Pattern 5: Remote-First IDE Attachment

**Problem:** Running a code editor or IDE locally against remote files introduces platform-specific behavior (line endings, symlinks, file watcher semantics) and bypasses the container's toolchain entirely.

**Solution:** The developer's local IDE connects to the developer container over SSH and mounts the container's filesystem as its workspace. The IDE runs its language server, linter, and formatter inside the container, not on the local device. Agent CLIs (LLM tools) run inside the container, not in a local terminal. The local device is a viewport — keyboard, mouse, and display — not a development environment.

**Trade-offs:** Requires the IDE to support remote development over SSH (most modern editors do). Requires the developer container to have a stable, reachable SSH endpoint. Network latency affects editor responsiveness; this is an argument for locating the container in the nearest cloud region, not an argument for local development.

### Pattern 6: Orchestrator-Driven Rolling Release

**Problem:** Deploying a new version of any container must be zero-downtime, automatically verified against health checks, and automatically rolled back on failure — without any bespoke update server or webhook mechanism inside the container.

**Solution:** The container orchestrator owns the entire release lifecycle. CI builds a new image, pushes it to the registry, and receives an immutable digest. CI then patches the target Deployment or StatefulSet with that digest via a narrow-scoped service account. The orchestrator performs a rolling update: it starts a new pod, waits for its readiness probe to pass, then terminates an old pod. This repeats until all replicas are updated. If any new pod fails its readiness probe before the rollout deadline, the orchestrator halts the rollout and CI triggers a rollback to the previous revision.

This pattern applies identically to the frontend, worker, and database containers. No container type has a special update mechanism. The orchestrator is the only update surface.

**Trade-offs:** Requires a running Kubernetes cluster and a kubeconfig for the CI service account. The rollout deadline (`progressDeadlineSeconds`) must be tuned per container type — a database StatefulSet update takes longer than a frontend Deployment update. Setting the deadline too short causes false rollback failures; too long delays detection of real failures.

---

## Plausible Architectures

### Architecture A: Single-Node Kubernetes Cluster (solo project, early stage)

```
┌─────────────────────────────────────────────────────────────────┐
│  Cloud Compute Instance (provisioned by agent via cloud API)    │
│                                                                 │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  Container Orchestrator (single-node)                   │    │
│  │                                                         │    │
│  │  ┌───────────────────────┐                              │    │
│  │  │  Developer Container  │  ← Agent CLIs, build tools  │    │
│  │  │  (full OS)            │    git, gh, bun, node, npm   │    │
│  │  │                       │    SSH endpoint for IDE       │    │
│  │  └───────────────────────┘                              │    │
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
│  │  External exposure: frontend port only                │    │
│  └─────────────────────────────────────────────────────────┘    │
└─────────────────────────────────────────────────────────────────┘

  Local Device (developer)
  ┌────────────────────┐
  │  IDE (SSH remote)  │──── SSH ──→  Developer Container
  │  Browser           │──── HTTPS ─→ Frontend Container
  └────────────────────┘
```

**When appropriate:** Single developer or single agent working on a new project. All four containers on one node is sufficient for early stages, demo, and demoware. Cost-minimal — one instance. The topology is identical to multi-node production; only the physical distribution differs.

**Trade-offs vs. other architectures:** No redundancy — if the node fails, everything fails. Acceptable at early stage because all durable state is in version control and the database volume backup. Not appropriate once the application serves real end users.

---

### Architecture B: Multi-Node Cluster (team, Beta / V1 stage)

```
┌──────────────────────────────────────────────────────────────────┐
│  Container Orchestrator (multi-node)                             │
│                                                                  │
│  ┌──────────────────┐   ┌──────────────────┐                     │
│  │  Dev Node A      │   │  Dev Node B      │                     │
│  │  Developer       │   │  Developer       │  ← Multiple agents  │
│  │  Container       │   │  Container       │    one per node     │
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

**When appropriate:** Multiple agents working on the same project in parallel. Web tier must handle real traffic. Database requires a replica for read scaling or failover. Each developer container gets its own node to eliminate toolchain interference.

**Trade-offs vs. Architecture A:** Higher cost. Requires networking between nodes. Database replication and consensus must be configured correctly. But these are production requirements, not engineering overhead — this architecture is what production looks like, so moving from Architecture A to Architecture B is a scaling exercise, not a redesign.

---

## Reference Implementation — Calypso TypeScript

> The following is the Calypso TypeScript reference implementation. The principles and patterns above apply equally to other stacks; this section illustrates one concrete realization using TypeScript, Bun, React, and PostgreSQL on DigitalOcean Kubernetes (DOKS).

### Container Images

Calypso provides four base images, published to the project's container registry. Projects derive from these images without modifying them unless a blueprint-documented reason exists.

| Image | Base | Installed | Not Installed |
|---|---|---|---|
| `calypso/dev` | Ubuntu LTS | `claude`, `gemini`, `codex`, `bun`, `node`, `npm`, `git`, `gh`, `bash`, `apt`, `tmux`, `openssh-server`, `playwright-deps`, `dockerd` | Frontend runtime, database |
| `calypso/frontend` | Alpine minimal | `bun` (runtime only) | `apt`, `npm`, `git`, `gh`, build tools, shells |
| `calypso/worker` | Minimal + Node | `bun`, `node`, vendor CLI binaries (`claude`, `gemini`, `codex`) | Shell, `apt`, `git`, `gh`, build tools |
| `calypso/postgres` | Distroless | PostgreSQL binary and libs | Everything else |

### Bootstrap Workflow

The agent executes the following steps from inside the developer container after the cluster is provisioned:

```
1. Agent receives cloud provider API key via environment variable
2. Agent runs: scripts/provision-cluster.sh
   - Creates compute instance (DigitalOcean Droplet)
   - Bootstraps DOKS (DigitalOcean Kubernetes Service)
   - Applies manifests from k8s/ directory
   - Outputs cluster endpoint and kubeconfig
3. Agent reads all files in agent-context/ before proceeding
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
  dev/
    deployment.yaml           ← developer container (Recreate strategy)
    service.yaml              ← SSH NodePort
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

The frontend container serves static files from `/app/dist/` baked into the image. It has no update endpoint and no runtime artifact fetching. See `k8s/rbac/ci-deployer.yaml` for the CI service account.

### Provisioning Script Interface

```typescript
// scripts/provision-cluster.ts
interface ProvisionConfig {
  provider: "digitalocean" | "hetzner" | "vultr";
  region: string;
  nodeSize: string;
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
| GitHub Actions (CI) | Release pipeline must run outside the developer container; hosted CI is the standard | Yes — Buy |
| PostgreSQL (distroless image) | Standard relational database; distroless image eliminates shell-based attack surface | Yes — Buy |

---

## Implementation Checklist

### Alpha Gate

- [ ] Cloud provider API key received and stored as environment variable in developer container; not committed to version control
- [ ] `scripts/provision-cluster.sh` executed; cluster endpoint output and reachable via `kubectl`
- [ ] All four container types running and healthy per `kubectl get pods`
- [ ] Developer container SSH endpoint reachable; local IDE connected via SSH remote
- [ ] Agent CLI (`claude`, `gemini`, or equivalent) running inside developer container, not on local device
- [ ] Frontend container serving a release bundle at the designated external port; RELEASE_TAG in `/health` response matches the deployed git SHA
- [ ] Worker container running and claiming tasks from the task queue; submitting results via API
- [ ] Database container running and accepting connections from frontend and worker containers only; not exposed externally; dev container cannot reach it (verified via network policy)
- [ ] `tmux` session active inside developer container; SSH disconnect and reattach tested
- [ ] Agent has read all files in `agent-context/` before writing any code

### Beta Gate

- [ ] Release pipeline configured: push to main triggers CI, CI builds release overlay image, CI patches deployment with immutable digest, rollout completes within timeout
- [ ] Rollback tested: deploy a bad image (readiness probe fails), confirm CI runs `kubectl rollout undo`, old pods resume serving
- [ ] Database volume backup scheduled and tested; restore procedure documented and executed at least once
- [ ] Firewall rules verified: only frontend port and developer container SSH port reachable externally
- [ ] Frontend container image verified to contain no build tooling (`git`, `npm`, `bun install`, `tsc` absent)
- [ ] Database container verified to have no shell access (`kubectl exec` into db container fails as expected)
- [ ] Integration test suite spins up an ephemeral database container, runs to completion, and tears it down — confirmed via `docker ps` showing no residual containers after the suite exits
- [ ] Network policy verified: `kubectl exec` into developer container cannot reach the cluster database service by hostname or IP
- [ ] Test suite connection string verified to point at the ephemeral container port, not any cluster service
- [ ] Provisioning script idempotent: running it twice produces a clean cluster without manual cleanup
- [ ] Cluster reprovisioned from scratch on a fresh API key; new cluster reaches ready state without manual steps

### V1 Gate

- [ ] Multi-node cluster deployed with frontend replicated across at least two nodes
- [ ] Database replica configured; failover tested
- [ ] Cluster monitoring active: container restarts, disk pressure, memory pressure all generate alerts
- [ ] Rollback verified automatic: `progressDeadlineSeconds` exceeded triggers CI failure; `kubectl rollout undo` restores previous revision without human intervention; confirmed for both frontend and worker deployments
- [ ] Recovery drill completed: cluster destroyed, reprovisioned, database volume restored; end-to-end time measured and within SLA

---

## Antipatterns

- **Agent running on the developer's local device.** When the agent runs locally, it inherits the local operating system, local filesystem, and local toolchain. Every output it produces may silently encode local assumptions. The agent belongs in the developer container, full stop.

- **IDE running against local files.** Using an IDE in local mode against a local checkout of the repository bypasses the container's toolchain. Files edited locally may have different line endings, symlink behavior, or import resolution than files edited inside the container. The IDE must connect to the developer container via SSH remote.

- **Frontend container with build tools installed.** Adding `npm`, `bun install`, `tsc`, or any build capability to the frontend container turns it into a shadow developer container with no CI gate. Code built inside the frontend has not been tested. A frontend that can build from source can serve untested code.

- **Agents accessing the database container directly.** An agent that connects to the database process directly — whether through a shell, through an admin client, or through a root-level credential — can make schema changes, data mutations, and configuration changes with no audit trail and no review gate. Agents interact with the database through the application's data layer only.

- **Environment-specific configuration branches.** Creating configuration files, environment variables, or code paths that behave differently in "development mode" versus "production mode" reintroduces the environment delta. There is one mode. Code that needs a flag to determine its environment is code that does not know where it is running.

- **Manual cluster provisioning.** Clicking through a cloud provider's web console, running ad-hoc CLI commands, or following a written runbook to provision the cluster creates undocumented state. The next time the cluster must be provisioned — whether due to failure, scaling, or migration — the process will produce a different result. Provisioning is code.

- **Serving from the main branch.** Configuring the frontend to pull and serve the latest commit from the main branch eliminates the release gate entirely. Every commit to main — including commits with failing tests, incomplete features, or broken builds — would be immediately served. The frontend serves tagged releases only.

- **Skipping the release pipeline for "just a demo."** A demo is a production event. An investor, a customer, or a stakeholder who sees the demo is seeing the product. Code served at a demo that has not passed CI, has not been tested, and has not been released is code that might fail during the demo. The pipeline is not a formality for demos; it is the mechanism that makes demos reliable.

- **Tests running against the cluster database.** Pointing integration or end-to-end tests at the cluster database — even "just this once" or "it's only demo data" — eliminates the guarantee that test runs are non-destructive. A test that seeds, mutates, or deletes rows in a shared database is a test that can corrupt a demo, a stakeholder session, or eventually real user data. Ephemeral test containers exist precisely so this choice never has to be made.

- **Cluster database reachable from the developer container.** If the developer container can reach the cluster database over the network, it will eventually do so — by mistake, by a misconfigured connection string, or by a well-intentioned agent that "just needed to check something." Network policy must make the cluster database unreachable from the developer container. Reachability is not a matter of trust; it is a matter of topology.

- **Ephemeral test containers not torn down on failure.** A test runner that spins up a database container but only tears it down on success will accumulate zombie containers on the developer node every time a test fails. Over a long development session this exhausts ports, disk, and memory. Teardown must happen in a finally block — unconditionally — regardless of test outcome.

- **Local port-forwarding as a substitute for the frontend container.** Forwarding a local development server port to a browser — via SSH tunnel, ngrok, or a similar tool — is not a preview environment. It is a local server with production traffic pointed at it. It has no release gate, no deployment artifact, and no parity with the actual frontend container. It is invisible to the release pipeline and to every other agent on the project.
