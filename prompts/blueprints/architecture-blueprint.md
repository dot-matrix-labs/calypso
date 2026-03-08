
# Architecture Blueprint

## Stack

* Language: TypeScript only; no other languages permitted.
* Runtime: Bun (server and build tasks).
* UI: React (latest stable).
* Styling: Tailwind CSS (vanilla CSS, no processors).
* State Management: React hooks or minimal context; no heavy state libraries.
* Testing: Vitest (unit), Playwright (browser/E2E).

## Build & Separation

* Browser code: `/apps/web` → React + Tailwind, browser-only bundle.
* Server code: `/apps/server` → Bun + Node ESM.
* Packages: `/packages/ui`, `/packages/core`, `/packages/services`, `/packages/integrations`.
* Strict separation of browser vs server runtime code.
* CI/CD pipelines enforce separate builds.

## Data & Integration Guidelines

* Prefer REST APIs for all business integrations.
* Define universal application types in TypeScript for all API inputs/outputs.
* Avoid GraphQL, WebSockets, or Protobufs unless system requires massive users or low-latency real-time.
* Keep types minimal and explicit to prevent casting, mutation, or hidden conversions.
* AI agents may generate type-safe interfaces automatically from API definitions.
* All API contracts are versioned and type-checked against production responses.

## Core Services

* Ingestion / integration services (REST API clients).
* Core business logic / domain services.
* UI modules, editors, or workspaces.
* Export / external integration modules.
* Authentication and authorization modules.

## Repository Structure

```text
/apps
  /web       # browser bundle
  /server    # Bun server
/packages
  /ui
  /core
  /services
  /integrations
/tests
  /unit
  /integration
  /e2e
/docs
  architecture.md
  product.md
  roadmap.md
  dependencies.md
```

---

## Dependency Policy

**Principle:** Hyper minimalism, which prevents software bloat and ensures long-term maintainability. Dependencies are a trade-off. We do not clone everything, but use discretion to determine when to buy vs DIY. Conciseness and removing boilerplate is important for humans, but not for AI agents; they can focus on resilient code with fewer assumptions and constraints, and tree shake just the needed functions from what would previously been a dependency supply chain.

**Threshold for Adding a Dependency**

1. Critical functionality not feasible internally within reasonable effort.
2. Mature, minimal footprint, well-maintained package.

**Strategy**

* Use discretion when considering external packages.
* **Buy (Import) Example:** Complex external integrations (e.g., Stripe SDK), highly specialized libraries with strict compliance requirements, or massive well-tested utility libraries where DIY is error-prone.
* **DIY (Clone/Re-implement) Example:** Simple utility functions (e.g. basic date formatting instead of `date-fns`), small UI components, or trivial helpers where an AI agent can cleanly generate a fully tested, tree-shaken internal version without bloating context.
* Lock versions and review dependency trees regularly.
* Document all dependencies in `docs/dependencies.md`, including risk/benefit justification.
* Avoid cascading dependencies.
