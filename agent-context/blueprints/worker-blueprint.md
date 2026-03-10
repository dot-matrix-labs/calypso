# Worker Service Blueprint

<!-- last-edited: 2026-03-10 -->

CONTEXT MAP
  this ──requires────────▶ blueprints/auth-blueprint.md (agent credential mechanics)
  this ──requires────────▶ blueprints/data-blueprint.md (data tier the agent operates on)
  this ──requires────────▶ blueprints/environment-blueprint.md (worker container model)
  this ◀──referenced by──── index.md

> [!IMPORTANT]
> This blueprint defines the agent-as-service execution model: how AI agents run as containerized daemons, what data they may read, why they may never write directly to the database, and how they submit results through user-authenticated API transactions. Read the [Auth Blueprint](./auth-blueprint.md) for agent credential mechanics and the [Data Blueprint](./data-blueprint.md) for the data tier the agent operates on.

---

## Vision

An AI agent that can write directly to a database is an AI agent that can corrupt data without a human in the loop, without an audit trail, and without the business logic and validation that the application layer enforces. This is not a theoretical risk — it is the inevitable outcome of giving an autonomous process unfettered write access to shared state. The agent will use that access, because it is the path of least resistance to completing its task. Every shortcut the architecture permits will eventually be taken.

Calypso treats agents as first-class service participants with a deliberately constrained capability set. An agent reads from a structured task queue, executes work in its domain — calling AI vendor APIs, invoking vendor CLI tools, applying transformations — and then submits its results to the application's API layer using a delegated user credential. The API layer validates, authorizes, and commits the write exactly as it would for a human-initiated request. The agent is not special. Its writes are subject to the same schema validation, the same access control checks, and the same audit logging as any other request. The agent cannot bypass this path because the database is structurally unreachable from the worker container at the network level.

This model also separates concerns cleanly. The agent knows how to execute AI work. It does not know how to validate business rules, enforce schema constraints, or make authorization decisions. Those responsibilities belong to the API layer, which has always owned them. Routing agent writes through the API is not bureaucracy — it is a recognition that the agent is good at one thing and should not be trusted to do another.

Different agent types have different capabilities, different task queue subscriptions, and different database roles. A coding agent and an analysis agent may run in the same container image but hold different credentials, see different task types, and have different read-only views of the database. The capability boundary for each agent type is declared at deployment time and enforced at the infrastructure level — not by the agent's own judgment about what it should or should not access.

The cost of ignoring this blueprint is an agent-shaped hole in the application's security model. An agent with direct write access can bypass validation, silently corrupt records, act on stale reads without conflict detection, and exfiltrate data through any write surface it can reach. These outcomes are not hypothetical. They are the natural consequence of treating agents as trusted insiders rather than as externally-constrained service participants.

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| Agent writes malformed or adversarially crafted data directly to the database | Data integrity — all writes must pass through the API layer's validation and business logic |
| Compromised agent credential grants write access to the database | Database integrity — the agent DB role must be read-only; write access is structurally impossible regardless of credential scope |
| Agent reads data outside its authorized scope (e.g., another user's records) | User data privacy — agent DB role must be restricted to task-queue views and anonymized/aggregated data, enforced by row-level security |
| Agent acts on a task that has already been claimed or cancelled | Task integrity — task queue must use atomic claim operations; acting on a stale task must produce a rejected API response |
| Delegated user token used by agent outlives the task it was issued for | Authorization scope — delegated tokens must be single-use and task-scoped; a consumed token must not be reusable |
| Agent submits a result that impersonates a different user | User identity integrity — the API layer must verify that the delegated token's user identity matches the task's owner |
| Agent container gains shell access or package management capability | Container security — worker containers are distroless; no shell, no package manager, no escalation path |
| Agent type A accesses task types or data views belonging to agent type B | Agent isolation — each agent type's DB role grants access only to its own task queue view; type claims are validated by the API |
| AI vendor API key leaked from worker container environment | Blast radius of key compromise — vendor API keys must be scoped to minimum permissions and rotated on schedule |
| Agent spawns a vendor CLI binary that exfiltrates data via network | Egress control — worker containers must have narrowly scoped network egress; vendor CLI calls must be audited via structured logging |

---

## Core Principles

### Agents are read-authorized, write-prohibited at the database layer

The agent's database role grants read access to a curated set of views — the task queue, anonymized context data, and any aggregated reference data the agent requires. It grants no write permissions to any table, under any circumstances. This is not enforced by application code, which can be buggy or bypassed. It is enforced by the PostgreSQL role definition, which cannot be overridden by the agent. An agent that attempts a direct write receives a permission error from the database before any application logic runs.

### All agent writes are user-authenticated API transactions

Every state change an agent produces is submitted as an authenticated request to the application API, using a delegated credential issued by the user who owns the task. The API processes the request identically to a human-initiated write: it validates input, checks authorization, enforces business rules, and writes through the standard data layer. The agent cannot choose a different path. The database is not reachable. The API is the only write surface.

### Agent capability is declared at deployment, not at runtime

An agent's task type subscription, its database role, its vendor API access, and its network egress rules are all declared in the agent's deployment manifest and enforced by the infrastructure. The agent does not self-select its capabilities at runtime. An agent that attempts to access a task queue view for a different agent type receives a database permission error. An agent that attempts to call a vendor API it was not granted receives a network error. The agent's judgment about what it needs is never the enforcement mechanism.

### Delegated tokens are single-use and task-scoped

When a user creates a task, the API issues a short-lived, task-scoped delegated token and includes it in the task record. The agent reads the token as part of the task, uses it to submit the result, and the token is invalidated on first use. If the agent fails before submitting, the token expires by TTL. A delegated token cannot be used to submit results for a different task, to read data outside its scope, or to initiate a new task. The token's scope is the task's scope.

### Agent types are isolated from each other

Different agent types — coding, analysis, document processing, or any future type — are deployed with different database roles, different task queue views, and different vendor API credentials. An agent of type A cannot read tasks intended for type B, cannot use type B's vendor API keys, and cannot submit results using type B's identity. This isolation is enforced at every layer: network policy, database role, and API validation. The similarity of different worker containers in their implementation does not create any shared access surface.

---

## Design Patterns

### Pattern 1: Task Queue Subscription via Read-Only View

**Problem:** Agents need a reliable, ordered, concurrent-safe mechanism to discover and claim work without racing each other or reading tasks they are not authorized to process.

**Solution:** The database exposes a per-agent-type view over the task queue table. The view filters by task type and status, returning only unclaimed tasks appropriate for that agent type. An agent claims a task by calling the API (not by writing to the database directly), which executes an atomic `UPDATE ... WHERE status = 'pending' RETURNING *` and returns the claimed task to the agent. The agent's database role can read the view but cannot execute the claim — that write goes through the API. This eliminates race conditions (the atomic update is the claim gate) and enforces type isolation (the view filter is the access boundary).

```
Agent DB role:
  GRANT SELECT ON task_queue_view_<agent_type> TO agent_<type>;
  -- No INSERT, UPDATE, DELETE on any table

Claim flow:
  Agent reads view → sees pending task → calls POST /api/tasks/{id}/claim
  API executes atomic UPDATE → returns claimed task with delegated token
  Agent executes work → calls POST /api/tasks/{id}/result with token
  API validates token, writes result, marks task complete
```

**Trade-offs:** The claim operation adds one API round-trip before the agent can begin work. This is the correct cost — the claim is the authorization gate, not a formality to be optimized away. At high task volumes, the API's claim endpoint becomes a bottleneck; the solution is to batch-claim tasks, not to give agents direct write access.

### Pattern 2: Delegated User Token

**Problem:** Agent-submitted results must be authorized as actions of the user who created the task, not as actions of a generic service identity. A service token that grants write access on behalf of any user is too broad; an agent with no user context cannot produce audit-attributable writes.

**Solution:** At task creation, the API generates a single-use, task-scoped capability token derived from the creating user's identity. The token encodes: the task ID it is valid for, the user ID it acts on behalf of, the specific API endpoints it may call, and an expiry time. The token is stored in the task record (encrypted at rest). The agent receives the token as part of the claimed task payload. When submitting a result, the agent presents the token; the API verifies the token's scope against the operation being requested, executes the write on behalf of the encoded user, and invalidates the token.

```
Token claims:
  sub: <user_id>          -- the user this action is attributed to
  task_id: <uuid>         -- exactly one task
  scope: ["task:result"]  -- only allowed operations
  exp: <unix_timestamp>   -- short TTL, typically 1 hour
  use: "once"             -- server-side invalidation on first use
```

**Trade-offs:** Token invalidation requires server-side state (a used-token log or a task status check). Stateless token validation is not sufficient for single-use semantics. This is an acceptable cost — the alternative is replayable tokens, which is not acceptable. If the agent crashes after claiming the token but before submitting, the task must be designed to be retried with a new claim (which issues a new token).

### Pattern 3: Vendor Binary Execution via Process Spawn

**Problem:** Worker containers must invoke AI vendor CLIs (claude, gemini, codex) without a shell, without package management, and without the ability to install or modify the binaries at runtime.

**Solution:** Vendor CLI binaries and their runtime dependencies are copied into the worker container image at build time. The agent daemon invokes them using a direct process spawn call — not via a shell interpreter. The binary path is hardcoded in the agent's configuration; the agent does not discover or resolve binaries at runtime. All input to the binary is passed as arguments or via stdin; no shell interpolation occurs. All output is captured on stdout/stderr and parsed by the agent; it is never piped to another shell command.

```
Correct:   spawn(["claude", "--print", prompt], { stdin: "pipe", stdout: "pipe" })
Incorrect: spawn(["sh", "-c", `claude --print "${prompt}"`])
```

**Trade-offs:** Vendor CLI binaries must be updated by rebuilding and redeploying the worker container image. There is no mechanism for the agent to self-update its CLIs. This is intentional — a container that can update its own binaries is a container that can be induced to run arbitrary code. The release pipeline for the worker image is the update mechanism.

### Pattern 4: Structured Execution Audit Log

**Problem:** An agent that calls an AI vendor API or spawns a vendor CLI generates outputs that must be attributable, inspectable, and retainable for debugging and compliance — but these outputs must not bypass the data layer's privacy controls.

**Solution:** Every agent execution — task claim, vendor API call, vendor CLI invocation, result submission — is logged to a structured audit table via the API (not directly). The log entry includes: task ID, agent type, operation type, input hash (not plaintext, to avoid logging sensitive prompts), output hash, token used, timestamp, and result status. The actual prompt and response content may be stored in the task record itself (subject to the data blueprint's encryption and retention policies) but are never written directly by the agent — they are submitted with the result payload and stored by the API layer.

**Trade-offs:** Hashing inputs and outputs enables tamper detection but prevents content inspection without the original. For debugging this is inconvenient; for compliance it is necessary. The task record, accessible via the API to authorized users, contains the full content where policy permits.

### Pattern 5: Per-Agent-Type Database Role

**Problem:** Multiple agent types running against the same database instance must not be able to read each other's task queues or reference data, even if they share the same container image.

**Solution:** Each agent type is assigned a dedicated PostgreSQL role at database initialization. The role is granted SELECT on the views and reference tables that agent type requires, and nothing else. Row-level security on the underlying task queue table enforces that even if a role's view definition is incorrect, a SELECT against it only returns rows with the matching task type. The agent's database credential corresponds to its type-specific role; the connection string is injected via a Kubernetes Secret scoped to that agent type's deployment.

```
-- At DB init:
CREATE ROLE agent_coding NOLOGIN;
CREATE ROLE agent_analysis NOLOGIN;
GRANT SELECT ON task_queue_view_coding TO agent_coding;
GRANT SELECT ON task_queue_view_analysis TO agent_analysis;
-- RLS policy on task_queue:
CREATE POLICY agent_type_isolation ON task_queue
  USING (task_type = current_setting('app.agent_type'));
```

**Trade-offs:** Each new agent type requires a database init script update and a cluster re-apply. Role definitions are not self-service. This is intentional — adding a new agent type is an architectural decision, not a configuration change, and it should require explicit review.

---

## Plausible Architectures

### Architecture A: Single Agent Type, Single Replica

```
┌─────────────────────────────────────────────────────────────────┐
│  Cluster                                                        │
│                                                                 │
│  ┌─────────────────┐     ┌──────────────────────────────────┐   │
│  │  Worker Container│     │  Frontend / API Container      │   │
│  │                 │     │                                  │   │
│  │  Bun daemon     │────▶│  POST /tasks/{id}/claim          │   │
│  │  Vendor CLIs    │────▶│  POST /tasks/{id}/result         │   │
│  │  (no shell)     │     │  (validates delegated token)     │   │
│  └────────┬────────┘     └──────────────┬───────────────────┘   │
│           │ SELECT only                 │ writes                 │
│           ▼                             ▼                        │
│  ┌─────────────────────────────────────────────────────────┐    │
│  │  Database Container                                     │    │
│  │  task_queue_view_<type>  (agent reads)                  │    │
│  │  task_queue table        (API writes)                   │    │
│  └─────────────────────────────────────────────────────────┘    │
│                                                                 │
│  ──────── Network policy ────────────────────────────────────   │
│  Agent → API:      allowed (HTTPS)                              │
│  Agent → DB:       SELECT on view only (role-enforced)          │
│  Agent → DB write: structurally impossible (no write grants)    │
│  Host → Cluster DB: blocked (no cluster DB credentials on host) │
└─────────────────────────────────────────────────────────────────┘
```

**When appropriate:** Single agent type, early-stage project, low task volume. One replica is sufficient. The task claim is atomic so no queue starvation occurs with a single consumer.

**Trade-offs vs. other architectures:** No redundancy — if the worker container dies, tasks queue up until it restarts. No horizontal scaling. Acceptable for low task volume where the task queue provides natural buffering.

---

### Architecture B: Multiple Agent Types, Concurrent Replicas

```
┌──────────────────────────────────────────────────────────────────────┐
│  Cluster                                                             │
│                                                                      │
│  ┌─────────────────────┐   ┌─────────────────────┐                  │
│  │  Agent: coding (×N) │   │ Agent: analysis (×M) │                 │
│  │  role: agent_coding │   │ role: agent_analysis │                 │
│  │  vendor: claude CLI │   │ vendor: gemini API   │                 │
│  └──────────┬──────────┘   └──────────┬───────────┘                 │
│             │                         │                              │
│             └──────────┬──────────────┘                              │
│                        ▼                                             │
│             ┌──────────────────────┐                                 │
│             │  API / Frontend    │  ← single write surface        │
│             │  claim + result      │                                 │
│             │  validates token     │                                 │
│             │  enforces ownership  │                                 │
│             └──────────┬───────────┘                                 │
│                        │                                             │
│             ┌──────────▼───────────┐                                 │
│             │  Database            │                                 │
│             │  view: _coding       │◀── agent_coding role           │
│             │  view: _analysis     │◀── agent_analysis role         │
│             │  task_queue table    │◀── API role (writes)           │
│             └──────────────────────┘                                 │
└──────────────────────────────────────────────────────────────────────┘
```

**When appropriate:** Multiple agent types running concurrently. Replicas per agent type scale with task volume. Each type is independently deployable and independently scalable.

**Trade-offs vs. Architecture A:** Requires per-type Kubernetes deployments, per-type database roles, and per-type vendor API credentials. The operational surface grows with each agent type added. The isolation guarantees make this necessary at any scale where multiple agent types operate on shared infrastructure.

---

## Reference Implementation — Calypso TypeScript

> The following is the Calypso TypeScript reference implementation. The principles and patterns above apply equally to other stacks; this section illustrates one concrete realization using TypeScript, Bun, and PostgreSQL.

### Package Structure

```
containers/agent/
  Dockerfile              ← distroless-style: bun + node + vendor CLIs, no shell
  daemon.ts               ← main process: poll queue, dispatch work, submit results
  workers/
    coding.ts             ← coding agent worker implementation
    analysis.ts           ← analysis agent worker implementation
  vendor/
    claude.ts             ← typed wrapper for claude CLI spawn
    gemini.ts             ← typed wrapper for gemini API calls
  lib/
    queue.ts              ← task queue polling and claim via API
    token.ts              ← delegated token handling
    audit.ts              ← structured execution logging via API
```

### Core Interfaces

```typescript
interface AgentTask {
  id: string;
  type: AgentTaskType;
  ownerId: string;
  payload: unknown;
  delegatedToken: string;   // single-use, task-scoped, short TTL
  claimedAt: string;
  expiresAt: string;
}

type AgentTaskType = "coding" | "analysis"; // extended as new types are added

interface TaskResult {
  taskId: string;
  agentType: AgentTaskType;
  outputHash: string;       // SHA-256 of the full output, for audit
  payload: unknown;         // the structured result, validated by API on receipt
}

interface VendorExecutionRecord {
  taskId: string;
  vendor: "claude" | "gemini" | "codex";
  inputHash: string;        // SHA-256 of prompt — content not stored here
  outputHash: string;
  durationMs: number;
  exitCode: number;
}
```

### Claim and Submit Flow

```typescript
// queue.ts — simplified; full implementation in containers/agent/lib/queue.ts
async function claimNextTask(apiBase: string, agentType: AgentTaskType): Promise<AgentTask | null> {
  const res = await fetch(`${apiBase}/api/agent/tasks/claim`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "authorization": `Bearer ${process.env.AGENT_SERVICE_TOKEN}`,
    },
    body: JSON.stringify({ agentType }),
  });
  if (res.status === 204) return null; // no tasks available
  if (!res.ok) throw new Error(`claim failed: ${res.status}`);
  return res.json();
}

async function submitResult(apiBase: string, task: AgentTask, result: TaskResult): Promise<void> {
  const res = await fetch(`${apiBase}/api/agent/tasks/${task.id}/result`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-delegated-token": task.delegatedToken,  // user-scoped, single-use
    },
    body: JSON.stringify(result),
  });
  if (!res.ok) throw new Error(`submit failed: ${res.status}`);
}
```

### Dependency Justification

| Package / Tool | Reason | Decision |
|---|---|---|
| `bun` (runtime) | Consistent with project standard; fast process spawn; built-in fetch | Buy |
| `node` (runtime, for vendor CLIs) | Claude Code CLI and Gemini CLI require Node to execute | Buy — no DIY path |
| `claude` CLI binary | Claude Code's tool-use and coding capabilities require the official CLI | Buy |
| `gemini` CLI binary | Gemini's multimodal and analysis capabilities via official CLI | Buy |
| Custom queue poller | Simple polling loop; no queue library justified at this scale | DIY |
| Custom delegated token handler | Token format is application-specific; no library matches the single-use pattern | DIY |

---

## Implementation Checklist

- [ ] Agent container image builds with no shell binary (`/bin/sh` absent, verified via `docker run ... which sh` returning non-zero)
- [ ] Agent DB role created with SELECT-only grants on task queue view; INSERT/UPDATE/DELETE produce permission errors (tested)
- [ ] Task claim endpoint performs atomic update; concurrent claim of the same task by two agents results in exactly one success (load-tested with two replicas)
- [ ] Delegated token validated on result submission; token with wrong task ID returns 403
- [ ] Delegated token invalidated after first use; second submission with same token returns 403
- [ ] Agent network policy verified: worker container cannot reach the database port directly (tested via `kubectl exec` attempt)
- [ ] Vendor CLI binary invoked without shell; `Bun.spawn` call uses array form, not string-with-shell
- [ ] Structured execution log entries written to audit table via API on every vendor invocation
- [ ] Agent service token distinct from user tokens and from frontend tokens; each has non-overlapping scope claims
- [ ] Per-agent-type database roles verified to be isolated: agent_coding cannot SELECT from task_queue_view_analysis (tested)
- [ ] Delegated token TTL enforced: token presented after expiry returns 401 regardless of use count
- [ ] Task retry semantics tested: agent crashes after claim, task re-enters queue after timeout, new claim succeeds
- [ ] Vendor API key rotation tested: new key injected via K8s Secret update, agent picks up new key on restart without image rebuild
- [ ] Audit log entries verified to contain input and output hashes, not plaintext content
- [ ] Agent container image rebuilt and redeployed via CI on changes to `containers/agent/**`; no manual steps required
- [ ] Agent type isolation penetration tested: agent_coding credential used to attempt direct DB write — confirms permission denied at DB layer
- [ ] Delegated token replay attack tested: token intercepted and replayed — confirms 403 on second use
- [ ] Agent horizontal scaling tested: N replicas claiming from the same queue with zero duplicate task execution under sustained load
- [ ] Vendor CLI version pinned in Dockerfile; version mismatch between dev and worker containers produces a build-time warning
- [ ] Agent egress network policy restricts outbound connections to declared vendor API hostnames only; all other egress blocked and logged

---

## Antipatterns

- **Direct database writes from the agent.** An agent with write access to the database bypasses schema validation, business logic, access control, and audit logging simultaneously. Even if the agent's writes are "correct" today, they are unreviewed, unvalidated, and unauditable. The write-through API pattern exists precisely to prevent this. There are no exceptions.

- **Shared service token for all agent types.** A single agent service token that all agent types use to authenticate to the API makes it impossible to revoke one agent type's access without revoking all agents', and makes audit logs unintelligible — every entry appears to come from the same identity. Each agent type must have its own service identity and its own token.

- **Delegated token with broad scope.** A delegated token that grants the agent general write access "on behalf of the user" rather than access to a specific task result endpoint is a user session token in disguise. The scope must be as narrow as the operation requires: submit a result for this task, nothing else.

- **Long-lived delegated tokens.** A delegated token that lives for hours or days outlives the agent session it was created for and becomes a credential that can be intercepted, cached, or leaked. Token TTL must be short enough that expiry before use is the expected outcome of a crashed agent, not an edge case.

- **Agent type capabilities selected at runtime.** An agent that reads a configuration file or environment variable to decide which task types it can process, which database views it can read, or which vendor APIs it can call has moved its capability boundary from the infrastructure layer to the application layer. Infrastructure-layer enforcement (database roles, network policy, K8s manifest) is the only enforcement that cannot be overridden by buggy or adversarially influenced application code.

- **Shell-form vendor CLI invocation.** Invoking a vendor CLI via a shell string (`sh -c "claude --print ..."`) exposes the agent to shell injection if any part of the prompt is interpolated into the command string. Vendor CLIs must be invoked using array-form process spawn, with input passed via stdin. This applies even when the input is "trusted" — the threat model assumes that AI-generated content may contain adversarial payloads.

- **Vendor CLI update inside the running container.** Any mechanism that allows the worker container to download and execute a new version of a vendor CLI at runtime — npm install, pip install, curl-pipe-bash, or any equivalent — is a remote code execution vulnerability. Vendor CLI versions are fixed at image build time. Updates require a new image build and a rolling deployment.

- **Writing prompt content to the audit log.** An audit log that stores full prompt text and AI responses is a log that contains user data, potentially sensitive context, and possibly regulated content. Logs are often less protected than application data — they may be shipped to third-party log aggregators, retained indefinitely, or accessed by operators without user consent. The audit log stores hashes and metadata; the content lives in the task record under the application's standard encryption and access control.
