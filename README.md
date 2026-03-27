# Calypso

Supergreen software is here. We provide a method, tools, and a reference implementation for:

- **Fused AI** — one AI over coherent, owned data
- **Tree-shaken** — distill the 5% of features you actually use across your SaaS vendors into one seamless app
- **Correct by construction** — every line verified, maximal control over the bytes, DIY over buy
- **Self-improving** — the agent has access to live logs and telemetry, is never idle, and enters hardening mode when there is nothing left to build

---

## Quick Start

### Install the CLI

```bash
curl -fsSL https://github.com/dot-matrix-labs/calypso/releases/latest/download/install.sh | bash
calypso-cli --version
```

For specific versions or canary builds, see [Installation](#installation) below.

### For Project Bootstrapping

Paste into your AI agent (Claude Code, Gemini CLI, Codex, etc.):

```
Agent, bootstrap a new Calypso project. You will handle all setup steps autonomously.

Follow agent-context/init/scaffold-task.md from Step 1.

Context: I am replacing GitHub Projects for a software team of 3.
```

Replace the context line with your own project description. Run this from a cloud host you've SSH'd into (or connected to via VS Code Remote SSH). The scaffold task will guide the agent through setting up the app cluster, creating the GitHub repo, and getting the project running — all on the host you're already on.

---

## The Vision

Since 2025, a solo operator can produce applications that would have taken a 20-person engineering team two years to ship. That is not an exaggeration — it is the current state of the art. AI lets every fruit stand build NASA-quality software, if you let it.

From 2026, we can go further. Super apps that leave behind human constraints entirely — deeply fused systems, highly complex security policies, deeply verified, continuously improved, never idle.

**To get there, you need to go supergreen.** Orchestrating your existing SaaS stack with AI wrappers is a local maximum. You are routing around the constraints of software that was never designed for this environment.

There is a deeper problem with the fragmented SaaS model: *N* smart AIs across *N* vendor data silos will always produce worse outcomes than a single AI — even a less capable one — over fused, coherent data. You cannot reason well across data you do not hold. Fragmentation is a fundamental cap on every AI-assisted decision your organization can make.

This vision has always required superhuman implementation capacity. We have arrived at the moment that exists.

**Supergreen:**

- **Fused AI** — one AI over coherent, owned data
- **Tree-shaken** — distill the 5% of features you actually use across your SaaS vendors into one seamless app
- **Correct by construction** — every line verified, maximal control over the bytes, DIY over buy
- **Self-improving** — the agent has access to live logs and telemetry, is never idle, and enters hardening mode when there is nothing left to build

---

## Why

**1 — Programming took a leap — and changed the build vs. buy calculus**

- An AI operator delivers what a 20-person team would take years to build
- The cost of building has dropped sharply
- The cost of depending on third parties has not
- Buying SaaS was how you avoided writing expensive code — today you pay a premium to not have control

→ Building is no longer the risk. External dependency is now the risk.

**2 — "Vibe coding" is not an enterprise standard — it is a false sense of progress**

Generating software has become easy. Operating critical systems remains hard.

Your expectations: security, reliability, business continuity.

The reality of improvised apps: no threat model, no consistent controls, no incident response.

The real world charges: hackers and ransomware, data leaks or loss, production failures, audits and compliance.

It is not a question of "if" — it is "when." Works in the demo, fails in the crisis.

→ On the day you need it most, it exposes you.

**3 — AI on top of silos is a structural ceiling**

It looks like progress, but it is a local maximum. Organizations implementing AI are discovering they need to rebuild their data systems — and end up with a new patchwork of vendors instead of the old one. You cannot reason well about data you do not control, cannot see in full, or have fragmented across systems.

→ Fragmentation limits any AI.

**4 — Without a greenfield project, there is no real transformation**

AI demands a data architecture from the start. Legacy + ETL + SaaS exports do not solve it. You end up reconciling imperfect data, adapting models to the past, and accumulating complexity. You cannot transform an enterprise with AI if it cannot even send invoices correctly.

The alternative: owned data from day one, AI integrated from day one, every feature compounding the advantage.

→ Stop patching. Start building advantage.

**5 — Superfield — for greenfield AI projects**

An AI model that defines how custom enterprise platforms should be built — with AI integrated from the start, designed to continuously improve. An engineering brain that condenses thousands of hours of real-world experience.

→ Let AI build the software it wants — not within human constraints.

---

## The Blueprint

Calypso is opinionated. Several choices are counter-intuitive coming from a human development culture — they make full sense once humans are out of the development loop.

**Process** — The agent operates as a self-advancing state machine. Each commit updates the implementation plan and writes the next prompt. The agent is never waiting for human input between tasks. When there is nothing left to build, it enters hardening mode.

**Testing** — Never mock. Not APIs, not the database, not the DOM. Humans mock because writing the real thing takes time they do not have. Agents do not have that constraint. Mocks hide bugs; real fixtures catch them. All browser tests run in headless Chromium — agents have no display server, and neither should the test suite.

**Dependencies** — DIY over buy. Humans import libraries to avoid writing code. Agents write the code directly, perfectly tree-shaken to the exact behavior needed, with no transitive dependency surface to audit or upgrade. Buy only when the domain is genuinely specialized (cryptography, payment SDKs, compliance-critical integrations).

**Data** — No ORMs. Agents write SQL directly with no cognitive overhead. ORMs exist to make databases approachable for humans; they abstract away performance and generate massive footprint. The agent does not need the abstraction. Start with SQLite, graduate to PostgreSQL.

**UX** — Beauty is a gate condition, not a preference. An ugly early version sets an anchor that is nearly impossible to reverse. The AI agent is a first-class user of every application it builds: it interacts through typed APIs, not through browser automation or interfaces designed for human perception. Admin is also a first-class user — never through raw database tooling or developer consoles.

**Security** — The threat model is not "prevent breaches." It is "make a breach useless." Greenfield applications have no brownfield trade-offs to honor, so there is no excuse for anything less than banking-grade authorization, HIPAA-grade privacy, and adversarial hardening from day one. Novel cryptographic architectures — homomorphic encryption, zero-knowledge proofs, encrypted computation — open an opportunity that legacy systems can never reach: deeply analytical applications that operate over sensitive data without ever exposing it in plaintext. High analytical power and high customer confidence in privacy are not in tension. In a supergreen system, they are the same design.

**Deployment** — Exclusively containerized, Kubernetes. The app (frontend, worker, database) runs in a three-container K8s cluster on the cloud host. The agent and developer work directly on the host OS — SSH in or use VS Code Remote SSH. No dev containers, no local laptops, no environment drift.

---

## Reference Implementation

### Calypso TS

Calypso TS exists for your current engineering team. Familiar tooling, no hype, best practices applied with discipline. The supergreen principles do not require a new language — they require a new approach.

| Layer | Choice |
|---|---|
| Language | TypeScript only |
| Runtime | Bun |
| UI | React + Tailwind CSS |
| Testing | Vitest (unit) + Playwright (headless E2E) |
| CI/CD | GitHub Actions |
| Database | SQLite → PostgreSQL |
| Auth | Passkey-first, self-hosted JWT (HTTP-only cookies), customer-side encryption before data is committed |
| Deploy | Exclusively containerized, Kubernetes |

No ORMs. No SaaS auth vendors. No mocks in tests.

### What Comes Next

Once you go post-human, the stack goes lower. The constraints that TypeScript and its runtime impose exist for human reasons — readability, ecosystem familiarity, fast iteration by engineers. An agent operating continuously does not need those affordances. The stack can descend toward the metal.

**Calypso RS** — a minimalist Rust stack end-to-end, with a fully WASM client for state management and DOM rendering. No React.

**[Alien Stack](https://github.com/dot-matrix-labs/alien-stack)** — our research lab paper on the future of software process. One day, maybe LLVM.

---

## Installation

### One-liner Install

The installer auto-detects your OS and architecture, downloads the correct binary, verifies the SHA-256 checksum, and places it in `/usr/local/bin`:

```bash
curl -fsSL https://github.com/dot-matrix-labs/calypso/releases/latest/download/install.sh | bash
```

**Specific version:**
```bash
curl -fsSL https://github.com/dot-matrix-labs/calypso/releases/latest/download/install.sh | bash -s -- 0.1.0
```

**Latest canary (pre-release):**
```bash
curl -fsSL https://github.com/dot-matrix-labs/calypso/releases/latest/download/install.sh | bash -s -- canary
```

**Custom install directory** (no sudo required):
```bash
INSTALL_DIR=~/.local/bin curl -fsSL https://github.com/dot-matrix-labs/calypso/releases/latest/download/install.sh | bash
```

### Supported Platforms

| OS | Architecture | Target |
|---|---|---|
| macOS | Apple Silicon (M1/M2/M3/M4) | `macos-aarch64` |
| macOS | Intel | `macos-x86_64` |
| Linux | x86_64 | `linux-x86_64` |
| Linux | ARM64 | `linux-aarch64` |

All binaries and checksums are published to [GitHub Releases](https://github.com/dot-matrix-labs/calypso/releases).

### Manual Download and Verification

1. Visit [GitHub Releases](https://github.com/dot-matrix-labs/calypso/releases)
2. Download the binary for your platform: `calypso-cli-<platform>-<version>.tar.gz`
3. Extract and verify the checksum:
   ```bash
   tar -xzf calypso-cli-*.tar.gz
   sha256sum -c calypso-cli-*.tar.gz.sha256
   ```
4. Move to your PATH:
   ```bash
   sudo mv calypso-cli /usr/local/bin/
   sudo chmod +x /usr/local/bin/calypso-cli
   ```

---

## Documentation

- [CLI Build Instructions](crates/calypso-cli/README.md) — building and developing the Calypso CLI
- [Calypso Blueprint](calypso-blueprint/blueprints/calypso-blueprint.md) — full architecture and process standard
- [Scaffold Task](calypso-blueprint/init/scaffold-task.md) — bootstrapping new Calypso projects
- [Agent Communication](agent-communication.md) — how to write documents agents interpret reliably
- See [calypso-blueprint/](calypso-blueprint/) for complete architecture, rules, and guidelines
