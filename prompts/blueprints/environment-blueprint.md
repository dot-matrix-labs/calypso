
# Environment Blueprint

> [!IMPORTANT]
> This blueprint defines the development environment for AI-agent-driven software projects: where agents run, what they depend on, and how sessions bootstrap into a consistent, reproducible state.

---

## Vision

Software built by AI agents inherits the environment it was built in. When an agent develops on a developer's laptop — with its GUI assumptions, its macOS filesystem semantics, its transient shell sessions — the resulting software silently encodes those assumptions. The code works on the laptop. It fails in production. The gap between "works for me" and "works in prod" is the environment delta, and AI agents are worse at detecting it than humans because they cannot see the physical machine they are running on.

A correct development environment for agent-driven work eliminates the delta entirely. The development host *is* the deployment target. The agent's session persists across disconnections. The toolchain is minimal, explicit, and bootstrapped from a single command. Every session begins by reading the project's conventions before writing a single line of code, because an agent that starts working without reading the rules will confidently produce work that violates them.

The cost of ignoring this blueprint is subtle and compounding. Code that passes tests locally but fails on Linux. Agents that reinvent project conventions because they were never told the real ones. Sessions that die when an SSH connection drops, losing hours of in-flight work. Environment drift between developers — human or AI — that makes "it works on my machine" the default state. These are not edge cases; they are the inevitable outcome of an unmanaged development environment.

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| Agent develops on macOS/Windows; code assumes non-Linux filesystem, process model, or networking | Production parity — code must behave identically in dev and prod |
| SSH connection drops mid-session | Session continuity — in-flight agent work and context must survive disconnection |
| Agent begins work without reading project conventions | Architectural consistency — agent must not fabricate or guess conventions |
| Standards in the upstream repository are updated after project bootstrap | Convention freshness — local standards must track upstream changes |
| Two agents (or an agent and a human) work on the same host with conflicting tool versions | Toolchain isolation — runtime versions and dependencies must be deterministic |
| Agent installs unnecessary system packages or modifies global state | Host stability — the development host is also the demo/preview server |
| Agent attempts to open a GUI, browser window, or display server | Headless integrity — agents are headless; visual output is screenshot-only |
| Development host is unreachable (network, provider outage) | Work recoverability — all state is in version control, not on the host |
| Agent uses an outdated or incompatible version of its own CLI | Agent capability parity — the CLI version determines what the agent can do |

---

## Core Principles

### Production is the only environment that matters

Development environments exist to produce production-correct software. Every divergence between development and production is a latent defect. The development host runs the same operating system, the same runtime, and the same process supervisor as production. There is no staging environment that is "close enough."

### Sessions are durable, not ephemeral

An AI agent's session is its working memory. Losing a session means losing context, partially applied changes, and the reasoning behind in-flight decisions. Sessions must survive network interruptions, terminal closures, and host reboots. The mechanism is a terminal multiplexer, not a hope that the connection stays up.

### Convention is bootstrapped, not discovered

An agent that begins work by reading the codebase will infer conventions from whatever code it encounters first — which may be legacy, wrong, or incomplete. Conventions are explicitly written, explicitly distributed, and explicitly read at the start of every session. The bootstrap is a single command with no ambiguity.

### The toolchain is minimal and declarative

Every tool on the development host exists because a blueprint requires it. No tool is installed speculatively. The dependency list is short, auditable, and version-pinned where possible. A minimal toolchain is easier to reproduce, easier to secure, and harder to break.

### Headless is the only mode

Agents have no display server, no GUI toolkit, no interactive browser. All visual evaluation happens through headless browser automation and screenshot capture. Code that requires a GUI to develop or test is code that cannot be developed or tested by an agent.

---

## Design Patterns

### Pattern 1: Single-Command Bootstrap

**Problem:** An agent starting a new session must configure its environment correctly before doing any work, but the configuration steps are numerous and easy to skip or reorder.

**Solution:** A single idempotent command that downloads the current project conventions, installs them locally, and exits with a non-zero code if anything fails. The agent runs this command first and reads the resulting files before proceeding. The command is a URL-based script fetch so it requires no local state to initiate.

**Trade-offs:** Depends on network access to the convention repository at session start. If the upstream is unreachable, the agent must fall back to whatever local copy exists — which may be stale. This is acceptable because staleness is better than fabrication.

### Pattern 2: Multiplexed Persistent Session

**Problem:** Agent sessions are long-running and stateful. Network interruptions, SSH timeouts, and terminal closures destroy the session and its accumulated context.

**Solution:** Run the agent process inside a terminal multiplexer session. The multiplexer maintains the session independently of the connecting terminal. Reconnection reattaches to the existing session with full history and state preserved.

**Trade-offs:** Adds one layer of indirection to the terminal stack. Operators must remember to reattach rather than start new sessions, or risk orphaned processes. Multiplexer configuration (scrollback, key bindings) can interfere with the agent CLI's own terminal handling.

### Pattern 3: Host-as-Preview

**Problem:** Developers need to see the running application during development. Traditional approaches involve local servers, ngrok tunnels, or separate staging deployments — all of which add environment delta.

**Solution:** The development host exposes a designated port for the running application. The development server *is* the preview. There is no separate preview environment, no tunnel, no proxy unless explicitly chosen. The URL is the host's IP or domain at the designated port.

**Trade-offs:** The development host must have a stable, routable IP address and appropriate firewall rules. Running untested code on a network-exposed host carries risk — acceptable for development and demo purposes, not for production.

### Pattern 4: Convention-First Session Start

**Problem:** An agent that starts coding immediately will produce work based on its training data and whatever it reads first in the codebase, not based on the project's actual conventions.

**Solution:** The session protocol requires the agent to read all files in the conventions directory before executing any other action. This is enforced by instruction (in the bootstrap prompt), not by tooling — because the agent is the actor, not a CI system.

**Trade-offs:** Relies on the agent following the instruction. There is no hard gate that prevents an agent from skipping the read step. However, the instruction is embedded in the bootstrap script output, the project's root configuration, and the blueprint itself — making accidental omission unlikely.

---

## Plausible Architectures

### Architecture A: Single Bare-Metal Host (solo agent, early-stage project)

```
┌─────────────────────────────────────────────┐
│  Bare-Metal Linux Host (cloud VPS)          │
│                                             │
│  ┌───────────────────────────────────────┐  │
│  │  Terminal Multiplexer Session         │  │
│  │                                       │  │
│  │  ┌─────────────┐  ┌───────────────┐  │  │
│  │  │  Agent CLI   │  │  Dev Server   │  │  │
│  │  │  (coding)    │  │  (port N)     │  │  │
│  │  └─────────────┘  └───────────────┘  │  │
│  └───────────────────────────────────────┘  │
│                                             │
│  docs/standards/  ← bootstrapped from repo  │
│  .env             ← host-local secrets      │
│  git repo         ← all persistent state    │
└─────────────────────────────────────────────┘
```

**When appropriate:** Single developer or single agent. Early-stage projects (Scaffold through Demoware). Cost-sensitive. The host is disposable — all durable state is in version control.

**Trade-offs vs. other architectures:** No redundancy. No isolation between dev server and agent process. Acceptable because the blast radius is one developer's work on one project.

### Architecture B: Multi-Agent Host (multiple agents, shared infrastructure)

```
┌──────────────────────────────────────────────────────┐
│  Bare-Metal Linux Host                               │
│                                                      │
│  ┌──────────────────┐  ┌──────────────────────────┐  │
│  │  Multiplexer: A  │  │  Multiplexer: B          │  │
│  │  Agent CLI (web)  │  │  Agent CLI (server)      │  │
│  │  Port 31415       │  │  Port 31416              │  │
│  └──────────────────┘  └──────────────────────────┘  │
│                                                      │
│  Shared:                                             │
│    docs/standards/  ← single bootstrap, shared read  │
│    git repo         ← branch-per-agent workflow      │
│    Runtime (pinned) ← single version, no conflicts   │
└──────────────────────────────────────────────────────┘
```

**When appropriate:** Multiple agents working on the same project simultaneously (e.g., one on frontend, one on backend). Same host to avoid environment drift. Each agent gets its own multiplexer session and preview port.

**Trade-offs vs. Architecture A:** Port management becomes explicit. Agents can interfere with each other's processes if not disciplined about working directories and branches. Git conflicts are possible if agents commit to the same branch.

### Architecture C: Ephemeral Cloud Instances (CI-like, on-demand)

```
┌────────────────────────────────────────┐
│  Orchestrator (API or human trigger)   │
│                                        │
│  Provisions:                           │
│  ┌──────────────────────────────────┐  │
│  │  Fresh Linux Instance            │  │
│  │  1. Install host dependencies    │  │
│  │  2. Clone repo                   │  │
│  │  3. Bootstrap standards          │  │
│  │  4. Run agent with task prompt   │  │
│  │  5. Push results, destroy host   │  │
│  └──────────────────────────────────┘  │
└────────────────────────────────────────┘
```

**When appropriate:** Tasks that are self-contained and don't need persistent preview (batch refactors, test suite runs, documentation generation). Regulatory or security contexts where a fresh environment per task is required.

**Trade-offs vs. Architecture A/B:** No session persistence — the agent starts cold every time. Higher latency (provisioning overhead). Higher cost if instances are large. But perfect isolation and zero drift between runs.

---

## Reference Implementation — Calypso TypeScript

> The following is the Calypso TypeScript reference implementation. The principles and patterns above apply equally to other stacks; this section illustrates one concrete realization using TypeScript, Bun, and the Calypso monorepo layout.

### Host Dependencies

The Calypso development host requires exactly these system-level tools:

| Tool | Purpose |
|---|---|
| `git` | Version control |
| `gh` | GitHub CLI, authenticated via HTTPS (`gh auth login -p https -w`) |
| `tmux` | Terminal multiplexer for persistent sessions |
| `bun` | JavaScript/TypeScript runtime and package manager |
| Agent CLI | Claude Code, Cursor server, Gemini CLI, or equivalent |
| Playwright OS deps | Headless Chromium libraries (`bunx playwright install-deps`) |

### Bootstrap Command

```bash
curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash
```

This script populates `docs/standards/` with the current Calypso conventions. The agent reads all files in this directory before any other action.

### Convention Directory Structure

```
docs/
└── standards/
    ├── calypso-blueprint.md
    ├── documentation-standard.md
    ├── development-standards.md
    ├── git-standards.md
    └── ...
```

Users may customize files in `docs/standards/` for project-specific requirements. The bootstrap script overwrites only files that are older than the upstream version.

### Preview Server

The Calypso dev server binds to port `31415`. This port is the project-wide convention and must be exposed on the host firewall.

### Dependency Justification

| Package | Reason to Buy | Justified |
|---|---|---|
| `tmux` | Session persistence with decades of stability; DIY terminal multiplexer is absurd | Yes |
| `bun` | Runtime, bundler, test runner, package manager in one binary; replaces Node + npm + webpack + jest | Yes |
| `gh` | GitHub API integration with auth management; DIY is fragile and under-tested | Yes |
| Playwright | Headless browser automation with cross-browser support; no viable DIY alternative | Yes |

---

## Implementation Checklist

### Alpha Gate

- [ ] Bare-metal Linux host provisioned with stable IP and SSH access
- [ ] `git`, `gh`, `tmux`, `bun`, and agent CLI installed and version-verified
- [ ] `gh auth login -p https -w` completed; `gh auth status` returns authenticated
- [ ] Playwright OS dependencies installed; `bunx playwright install-deps` exits cleanly
- [ ] Port `31415` open and reachable from external network
- [ ] `tmux` session created; agent CLI launched inside it
- [ ] Bootstrap script executed; `docs/standards/` populated with current conventions
- [ ] Agent has read all files in `docs/standards/` before writing any code
- [ ] Dev server starts and is accessible at `http://<host>:31415`
- [ ] SSH disconnect and reattach tested; `tmux` session survives

### Beta Gate

- [ ] Bootstrap script is idempotent; running it twice does not corrupt local customizations
- [ ] Host dependency versions are pinned in a project-level manifest or script
- [ ] Firewall rules documented; only required ports are exposed
- [ ] Agent session startup time measured and acceptable (under 60 seconds from SSH to coding)
- [ ] Second agent session tested on same host with separate multiplexer and port

### V1 Gate

- [ ] Host provisioning is scripted end-to-end (from bare OS to ready-to-code in one command)
- [ ] Monitoring on the dev host detects disk exhaustion, OOM, and zombie processes
- [ ] Bootstrap script validates upstream version and warns if local standards are stale
- [ ] Recovery procedure documented: host lost, new host provisioned, agent resumes from git state

---

## Antipatterns

- **Laptop-as-production.** Developing on macOS or Windows and assuming the code will work on Linux. Filesystem case sensitivity, process signals, path separators, and network behavior all differ. The agent cannot detect these differences from inside the code.

- **Naked SSH sessions.** Running the agent CLI directly in an SSH session without a terminal multiplexer. A single network hiccup destroys the session and all accumulated context. The agent must restart from scratch, re-reading files it already understood.

- **Convention by inference.** Allowing the agent to infer project conventions by reading existing code rather than reading the explicit standards documents. Existing code may contain legacy patterns, one-off exceptions, or simply bugs that the agent will dutifully replicate.

- **Snowflake host.** Installing tools, tweaking configurations, and adding scripts to the development host without recording those changes anywhere reproducible. When the host dies, the environment dies with it. The next host will be subtly different.

- **GUI development.** Installing a desktop environment, VS Code, or any GUI tool on the development host for the agent to use. Agents are headless. Visual evaluation is screenshot-based. A GUI is wasted resources and a false signal that visual development is supported.

- **Port roulette.** Letting the dev server pick a random available port instead of binding to the designated port. Other tools, scripts, and documentation all assume the designated port. A random port breaks every downstream reference.

- **Bootstrap skip.** Starting a coding session without running the bootstrap command because "nothing has changed upstream." The agent has no way to verify this claim. The cost of running bootstrap is seconds; the cost of working against stale conventions is hours of rework.
