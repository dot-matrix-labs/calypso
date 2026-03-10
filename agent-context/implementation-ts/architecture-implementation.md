
# Architecture — Calypso TypeScript Implementation

> This document is the Calypso TypeScript reference implementation for the [Architecture Blueprint](../blueprints/architecture-blueprint.md). The principles, threat model, and patterns in that document apply equally to other stacks. This document covers the concrete realization using TypeScript, Bun, React, and Tailwind CSS.

---

## Stack

| Layer | Choice |
|---|---|
| Language | TypeScript (only) |
| Runtime | Bun (server and build) |
| UI Framework | React (latest stable) |
| Styling | Tailwind CSS (vanilla, no processors) |
| State Management | React hooks and minimal context (no heavy state libraries) |
| Unit Testing | Vitest |
| Browser/E2E Testing | Playwright |
| API Style | REST |

## Repository Structure

```text
/apps
  /web          # React + Tailwind browser bundle
  /server       # Bun API server (Node ESM)
/packages
  /ui           # Shared UI components
  /core         # Business logic, domain types, shared TypeScript types
  /services     # API clients, service adapters
  /integrations # Third-party SDK wrappers
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

## Build Separation

- `/apps/web` builds a browser-only bundle. No server imports resolve.
- `/apps/server` builds a Bun server binary. No browser/DOM imports resolve.
- CI/CD runs separate build pipelines for each app.

## Data & Integration Guidelines

- REST APIs for all business integrations.
- Universal TypeScript types in `/packages/core` for all API inputs/outputs.
- Avoid GraphQL, WebSockets, or Protobufs unless the product requires massive concurrency or sub-second real-time.
- All API contracts versioned and type-checked against production responses.

## Core Service Categories

| Service Area | Location |
|---|---|
| Ingestion / integration (REST clients) | `/packages/services`, `/packages/integrations` |
| Core business logic / domain | `/packages/core` |
| UI modules, editors, workspaces | `/packages/ui`, `/apps/web` |
| Export / external integration | `/packages/integrations` |
| Authentication and authorization | `/apps/server` (middleware) |

## Dependency Policy

**Threshold:** Both must be true: (1) critical functionality not feasible internally, (2) mature, minimal footprint, well-maintained.

| Package | Reason | Buy or DIY |
|---|---|---|
| Stripe SDK | Payment processing with PCI compliance; infeasible to DIY | Buy |
| `date-fns` | Basic date formatting; agent generates a focused internal version | DIY |
| Playwright | Headless browser automation; no viable internal alternative | Buy |
| Small UI components | Agent generates tested, tree-shaken versions | DIY |

All dependencies documented in `docs/dependencies.md` with risk/benefit justification. Versions locked. Transitive trees reviewed regularly.

---

## Antipatterns (TypeScript-Specific)

- **Type duplication across boundaries.** Defining the same API response type in both `/apps/web` and `/apps/server` instead of importing from `/packages/core`. The types drift within days. The client renders stale fields; the server sends new ones. The mismatch is silent until a user reports broken UI.

- **Dependency by default.** Installing an npm package for every solved problem without evaluating whether the agent can build and test an internal version in minutes. The codebase accumulates dozens of packages, each with transitive trees the agent must understand and maintain. The `node_modules` directory becomes the largest part of the project.

- **Implicit build coupling.** Server and client share a build step or a single `tsconfig.json` with no overrides. A change to the server's TypeScript configuration silently affects the client build. Builds should be independent and separately configurable from day one.
