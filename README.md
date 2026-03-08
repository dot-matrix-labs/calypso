# Calypso

AI lets every fruit stand build NASA-quality software. A solo operator can now produce applications that would have taken a 20-person engineering team two years to ship. That is not an exaggeration — it is the current state of the art.

But there is a catch.

**To unlock AI's full capability, you need to build new.** Orchestrating your existing SaaS stack with AI wrappers is a local maximum — you are routing around the constraints of software that was never designed for this environment. Truly AI-native software is greenfield, coherent, and purpose-built. That is what Calypso is for.

There is a second, deeper problem with the fragmented SaaS model: *N* smart AIs deployed across *N* vendor data silos will always produce worse outcomes than a single AI — even a less capable one — operating over fused, coherent data. Fragmentation is not just an operational cost. It is a fundamental cap on the quality of every AI-assisted decision your organization can make. You cannot reason well across data you do not hold.

This vision has always required superhuman implementation capacity. We have arrived at the moment that exists.

---

## What Calypso Is

Calypso is three things:

1. **A method.** A staged, architecture-first discipline for building AI-native applications — from first scaffold to production V1 — without accumulating technical debt at each step.

2. **A set of lightweight git-native tools.** Prompts, blueprints, and bootstrap scripts that live in your repository and travel with your code. No platform, no dashboard, no vendor lock-in. The standards are files; the agent reads them at the start of every session.

3. **A TypeScript reference implementation.** A concrete, opinionated stack — Bun, React, Tailwind, Vitest, Playwright — with tested conventions for monorepo structure, CI pipelines, headless testing, deployment, auth, and logging. It is not a starter template; it is the architecture an agent follows to build *your* product.

---

## Quickstart

Copy this prompt to your AI agent of choice (Claude Code, Gemini CLI, Codex, etc.):

```
Agent, I want to build a project tracking app with Calypso.

CRITICAL: Before beginning, you MUST bootstrap the Calypso standards by running:

  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Then read docs/standards/calypso-blueprint.md before doing anything else.

Context: I work in software development, team of 3. I am replacing GitHub Projects
because it's ugly and confusing.
```

The agent bootstraps the standards into `docs/standards/`, reads them, runs an onboarding interview to produce `docs/prd.md`, generates a live implementation plan in `docs/plans/`, and begins building. Each commit advances the plan and writes the next prompt. The loop runs until the product ships.

---

## The Stack

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

---

## Documentation

- [Calypso Blueprint](prompts/calypso-blueprint.md) — full architecture and process standard
- [UX Blueprint](prompts/ux-blueprint.md) — UX posture, agent UX, beauty as a gate condition
- [Data Security Blueprint](prompts/data-security-blueprint.md) — agent auth, scopes, and security posture
- [Scaffold Task Entrypoint](prompts/scaffold-task.md) — the agent's first action on a new project
- [Blueprint Authoring Standard](prompts/blueprint-standard.md) — how blueprints are written and structured
