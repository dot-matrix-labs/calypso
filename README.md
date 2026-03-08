# Calypso

**Dancing with the SaaS-pocalypse**

Calypso is a framework and methodology for running a **synthetic software house** — a team of forward-deployed AI agents operating as an internal software development organization. Purpose-built to help organizations replace expensive SaaS vendors with bespoke, in-house software, one product at a time.

Software rarely fails due to code quality; it fails because of unclear requirements, chaotic architecture, and hype-driven choices. Calypso provides blueprints, architecture-first constraints, staged maturity protocols, and stack consistency to ensure AI-generated software is maintainable, predictable, and scalable from prototype to production.

## Quickstart

```text
Agent, I want to build a project tracking app with Calypso.

CRITICAL: Before beginning, bootstrap the Calypso standards:

  curl -sSL https://raw.githubusercontent.com/dot-matrix-labs/calypso/main/scripts/bootstrap-standards.sh | bash

Then read docs/calypso-ontology.ttl to determine which standards to load.

Context: I work in software development with a team of 3 developers. I'm replacing GitHub Projects.
```

## A Calypso for Every Developer

### 1. Community Edition (DIY)
Free and open-source. All prompts, standards, and reference architectures to bootstrap your own AI-native development studio.

### 2. Hosted (Not-SaaS)
Full operational stack without assembly. **Pass-through billing** — actual costs of infrastructure and API tokens, zero markup.

### 3. Enterprise Engagements
White-glove engagements for complex digital transformations. Embedded with your team to customize the framework and oversee your synthetic software house.

## Documentation Structure

### Decision Tree
- [`calypso-ontology.ttl`](calypso-ontology.ttl) — RDF graph that tells agents what to read based on their current state

### Standards (operational — loaded per the ontology)
- [`standards/stack.md`](standards/stack.md) — tech stack, environment, dependencies
- [`standards/process.md`](standards/process.md) — phases, planning docs, plan-stub-TDD workflow
- [`standards/testing.md`](standards/testing.md) — testing philosophy, CI/CD
- [`standards/deployment.md`](standards/deployment.md) — bare metal, systemd, database
- [`standards/git-discipline.md`](standards/git-discipline.md) — Git-Brain metadata and all git hooks
- [`standards/security.md`](standards/security.md) — secrets, auth, headers, audit logging
- [`standards/telemetry.md`](standards/telemetry.md) — production observability and self-healing
- [`standards/hardening.md`](standards/hardening.md) — continuous background improvement
- [`standards/multi-agent.md`](standards/multi-agent.md) — parallel agent coordination
- [`standards/documentation.md`](standards/documentation.md) — fractal documentation structure

### Reference (one-shot or human-facing)
- [`reference/scaffold-task.md`](reference/scaffold-task.md) — project initialization checklist
- [`reference/product-owner-interview.md`](reference/product-owner-interview.md) — requirements extraction
- [`reference/agent-session-bootstrap.md`](reference/agent-session-bootstrap.md) — vendor-specific agent configuration
- [`reference/faq.md`](reference/faq.md) — design decision rationale
- [`reference/prd.md`](reference/prd.md) — Calypso framework PRD
- [`reference/onboarding.md`](reference/onboarding.md) — customer engagement strategy
