# Calypso

A method, git-native tools, and a TypeScript reference implementation for building **supergreen** software:

- **Fused AI** — one AI over coherent, owned data
- **Tree-shaken** — distill the 5% of features you actually use across your SaaS vendors into one seamless app
- **Correct by construction** — every line verified, maximal control over the bytes, DIY over buy
- **Self-improving** — the agent has access to live logs and telemetry, is never idle, and enters hardening mode when there is nothing left to build

---

## Quickstart

Paste into your AI agent (Claude Code, Gemini CLI, Codex, etc.):

```
Agent, bootstrap a new Calypso project.

First, run:
curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Then read docs/standards/calypso-blueprint.md before doing anything else.

Context: I am replacing GitHub Projects for a software team of 3.
```

---

## The Vision

Since 2025, a solo operator can produce applications that would have taken a 20-person engineering team two years to ship. That is not an exaggeration — it is the current state of the art. AI lets every fruit stand build NASA-quality software, if you let it.

From 2026, we can go further. Super apps that leave behind the constraints of the human development cycle entirely — continuously verified, continuously improved, never idle.

**To get there, you need to go supergreen.** Orchestrating your existing SaaS stack with AI wrappers is a local maximum. You are routing around the constraints of software that was never designed for this environment.

There is a deeper problem with the fragmented SaaS model: *N* smart AIs across *N* vendor data silos will always produce worse outcomes than a single AI — even a less capable one — over fused, coherent data. You cannot reason well across data you do not hold. Fragmentation is a fundamental cap on every AI-assisted decision your organization can make.

This vision has always required superhuman implementation capacity. We have arrived at the moment that exists.

**Supergreen:**

- **Fused AI** — one AI over coherent, owned data
- **Tree-shaken** — distill the 5% of features you actually use across your SaaS vendors into one seamless app
- **Correct by construction** — every line verified, maximal control over the bytes, DIY over buy
- **Self-improving** — the agent has access to live logs and telemetry, is never idle, and enters hardening mode when there is nothing left to build

---

## Reference Implementation

Before making a giant leap, we start with familiar tooling. The supergreen principles can be applied today with the stack you already know.

**Calypso TS** — TypeScript reference implementation:

| Layer | Choice |
|---|---|
| Language | TypeScript only |
| Runtime | Bun |
| UI | React + Tailwind CSS |
| Testing | Vitest (unit) + Playwright (headless E2E) |
| CI/CD | GitHub Actions |
| Database | SQLite → PostgreSQL |
| Auth | Self-hosted JWT (HTTP-only cookies) |
| Deploy | Bare metal Linux, systemd |

No Docker. No ORMs. No SaaS auth vendors. No mocks in tests.

**Calypso RS** — the next level: a minimalist Rust stack end-to-end, with a fully WASM client for state management and DOM rendering. No React.

**Alien Stack** — our research lab paper on the future of software process. Someday.

---

## Documentation

- [Calypso Blueprint](prompts/calypso-blueprint.md) — full architecture and process standard
- [UX Blueprint](prompts/ux-blueprint.md) — UX posture, agent UX, beauty as a gate condition
- [Data Security Blueprint](prompts/data-security-blueprint.md) — agent auth, scopes, and security posture
- [Scaffold Task Entrypoint](prompts/scaffold-task.md) — the agent's first action on a new project
- [Blueprint Authoring Standard](prompts/blueprint-standard.md) — how blueprints are written and structured
