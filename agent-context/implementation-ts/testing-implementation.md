# Testing — Calypso TypeScript Implementation

<!-- last-edited: 2026-03-10 -->

CONTEXT MAP
  this ──implements──▶ blueprints/testing-blueprint.md
  this ◀──referenced by── index.md

> Implements: Testing Blueprint (`agent-context/blueprints/testing-blueprint.md`)

The principles, threat model, and patterns in that document apply equally to other stacks. This document covers the concrete realization using Vitest, Playwright, and GitHub Actions.

---

## Test Categories

| Category | Tool | Runtime | Location |
|---|---|---|---|
| Unit tests | Vitest | Bun (CLI) | `/tests/unit` |
| API integration tests | Vitest + golden fixtures + deployed container | Kubernetes (e.g. kind) + Bun (CLI) | `/tests/integration` |
| React component tests | Vitest + Playwright | Headless Chromium | `/tests/e2e` (component) |
| Full-page user story tests | Playwright | Headless Chromium | `/tests/e2e` (pages) |

## CI Workflow Structure

```
.github/workflows/
  test-unit.yml
  test-integration.yml
  test-component.yml
  test-e2e.yml
```

Each workflow:
1. Installs Bun and container tooling (e.g., Docker, kind)
2. Installs dependencies
3. Runs lint + format check
4. (For Integration): Builds container & deploys to local cluster
5. Runs its specific test suite
6. Reports pass/fail independently

Merge is blocked unless all four workflows pass.

## Golden Fixture Format

Fixtures are stored in `.env.test` (for credentials) and `/tests/fixtures/` (for recorded responses). The fixture recorder is a Bun script that:
1. Reads API credentials from `.env.test`
2. Makes real HTTP requests to external services
3. Writes response bodies and headers to JSON files in `/tests/fixtures/`
4. Logs schema changes compared to existing fixtures

`.env.test` is committed to the repository (it contains test-only credentials, not production secrets).

## Browser Test Configuration

All browser tests use Playwright in headless mode. Configuration:
- No browser runner or reporter — command-line test execution only
- Screenshot capture for visual evaluation
- No GUI, no display server, no `DISPLAY` environment variable

## Dependency Justification

| Package | Reason | Buy or DIY |
|---|---|---|
| Vitest | Test runner with native ESM and TypeScript support; integrated with Bun | Buy |
| Playwright | Headless browser automation; no viable DIY alternative | Buy |
| ESLint | Linting with ecosystem plugins; infeasible to replicate | Buy |
| Prettier | Deterministic formatting; agent-generated formatter would diverge | Buy |

---

## Antipatterns (TypeScript/Web-Specific)

- **JSDOM as a browser.** Running component tests in JSDOM or Happy DOM because they are faster than headless Chromium. These are DOM simulations, not browsers. They lack layout, rendering, real event dispatch, and network behavior. A component that passes in JSDOM and fails in Chromium is a component that fails in production.
