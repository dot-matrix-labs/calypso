
# Calypso Blueprint for Web Applications

## 0. Development Prerequisites

**Environment & Setup**

* Continuous development, testing, and operation must occur natively on a bare-metal Linux host in the cloud (e.g., DigitalOcean). 
* **Local Development Exception:** Users *are allowed* to perform the initial "Scaffold" phase (repository setup, documentation, initial source code) on their local development machines (e.g., Mac or Windows). However, if you (the AI agent) detect that the active environment is a local Mac or Windows machine, you must interject and occasionally "nag" the user, reminding them that to develop correctly and continuously they must move to a remote Linux host.
* Development tasks must be executed by an AI agent (e.g., Claude, Gemini, Codex, or whichever agent is used) running inside a `tmux` session on the remote host to ensure persistence.
* The development host also serves as the live demo/preview server.
* Port `31415` (the designated Calypso dev port) must be exposed on the host to access the live development preview.

**Linux Host Dependencies**
Before bootstrapping a Calypso project, the bare-metal Linux host must have the following system dependencies installed:
1. `git`: Version control.
2. `gh` (GitHub CLI): Must be installed and fully authenticated to allow the agent to manage pull requests and Nightshift workflows.
3. `tmux`: Required to persist the agent's session remotely.
4. `bun`: The core JavaScript runtime.
5. **The AI Agent CLI:** The specific CLI tool for the chosen agent (e.g., Claude Code, Cursor server, Gemini CLI).
6. **Playwright OS Dependencies:** Operating system libraries required to run headless Chromium for browser testing (typically installed via `bunx playwright install-deps`).

---

## 1. Architecture

**Stack**

* Language: TypeScript only; no other languages permitted.
* Runtime: Bun (server and build tasks).
* UI: React (latest stable).
* Styling: Tailwind CSS.
* State Management: React hooks or minimal context; no heavy state libraries.
* Testing: Vitest (unit), Playwright (browser/E2E).

**Build & Separation**

* Browser code: `/apps/web` → React + Tailwind, browser-only bundle.
* Server code: `/apps/server` → Bun + Node ESM.
* Packages: `/packages/ui`, `/packages/core`, `/packages/services`, `/packages/integrations`.
* Strict separation of browser vs server runtime code.
* CI/CD pipelines enforce separate builds.

**Data & Integration Guidelines**

* Prefer REST APIs for all business integrations.
* Define universal application types in TypeScript for all API inputs/outputs.
* Avoid GraphQL, WebSockets, or Protobufs unless system requires massive users or low-latency real-time.
* Keep types minimal and explicit to prevent casting, mutation, or hidden conversions.
* AI agents may generate type-safe interfaces automatically from API definitions.
* All API contracts are versioned and type-checked against production responses.

**Core Services**

* Ingestion / integration services (REST API clients).
* Core business logic / domain services.
* UI modules, editors, or workspaces.
* Export / external integration modules.
* Authentication and authorization modules.

**Repository Structure**

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

## 2. Dependency Policy

**Principle:** Hyper minimalism, which prevents software bloat and ensures long-term maintainability. Dependencies are a trade-off. We do not clone everything, but use discretion to determine when to buy vs DIY.

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

## 3. Process
0. **Scaffold:** Create the workspace with the [Nightshift](https://github.com/dot-matrix-labs/nightshift) methodology (adding context, reasoning ledgers, and git-hook quality gates) and the Calypso Blueprint below.
1. **Collect Specifications:** The AI agent must generate an `.md` document containing comprehensive onboarding interview questions for the Product Owner to extract requirements. An explicit template prompt is provided to instruct the agent on generating these questions. The agent then writes a canonical Product Requirements Doc to `docs/prd.md` (per Nightshift documentation rules) based on the answers. The Product Owner/Manager will own and update this document moving forward.
2. **Prototype:** mock data, minimal UI, basic flows, no persistence.
3. **Demoware:** partial integrations, realistic UI, stable demo workflows.
4. **Alpha:** full persistence, authentication, core business logic.
5. **Beta:** external integrations, performance, reliability, metrics.
6. **V1:** production-ready stability, observability, backups.

---

## 4. Testing Philosophy

**Core Principles**

* **Never mock** anything: no APIs, databases, DOM, or external services.
* **Always test on the environment the code will run in:** Linux for server and browser testing; no Mac/Windows shortcuts.
* Browser code tested only in headless Chromium (Vitest + Playwright).
* API tests must use **recorded "golden" fixtures of real production requests/responses**. To enable this without a human, the AI must explicitly develop a test tool which generates these "golden" fixtures by executing real network requests against external services. It must not mock, estimate, or hallucinate these fixtures.
* CI/CD enforces passing tests in production-like environments.

**Client-Side Test Categories**

1. **Unit Tests:**

   * Pure logic or modules that do **not require an API server**.
   * Validate algorithms, transformations, utility functions.

2. **API Integration Tests:**

   * Validate REST API calls against production-recorded fixtures.
   * Ensure TypeScript types match actual production schemas.

3. **React Component Tests:**

   * Test individual React components **in a headless browser**.
   * Validate rendering, props handling, state updates, styling, and interactions in isolation.

4. **Full-Page User Story Tests:**

   * Click-through flows covering navigation, forms, and workflows.
   * Ensures all components and integrations work together end-to-end.

**Implementation Notes**

* Unit tests are fast and deterministic; run locally in CI.
* Component and full-page tests **always run in headless Chromium**.
* API integration tests intercept HTTPS calls using recorded fixtures; never invent responses.
* Tests **validate real runtime behavior**, not mocks or simulated environments.


---

## 5. CI/CD Environment

**Platform**

* GitHub as VCS and CI/CD host.
* GitHub Actions as workflow engine.
* [Nightshift methodology](https://github.com/dot-matrix-labs/nightshift) ensures **sloppy or unreviewed code cannot enter CI** via git-hook "Nags".

**Workflow Design Principles**

* **One workflow per test suite**:

  * Unit
  * API Integration
  * React Component
  * Full-Page User Story
* Avoid multiple jobs in a single workflow — granularity allows precise failure diagnosis.
* Each workflow file contains all setup and teardown needed for that suite.

**Code Quality Checks**

* Linting (e.g., ESLint) and formatting (e.g., Prettier) are enforced **before tests run**.
* Nightshift rules: code cannot be merged if it violates lint or formatting.
* Tests are gated: failing lint/format or failed test suite **blocks merge**.

**Test Execution**

* Each workflow runs on Linux runners.
* Browser tests use headless Chromium via Playwright + Vitest.
* API integration tests use **recorded HTTPS fixtures**.
* CI mirrors **exact production environment**: no mocks, no Mac/Windows shortcuts.
* AI agents scaffold GitHub Actions YAML automatically for each test suite.

**Deployment Integration**

* Milestone-based deployment: Alpha → Beta → V1.
* Each deployment workflow uses CI validation: only pass-tested code is deployed.
* Logging, monitoring, backups enforced during Beta stage.

**Enforcement Rules for AI Agents**

* Always generate a separate `.github/workflows/*.yml` file per test suite.
* Include linting/formatting steps in each workflow.
* Do not merge or deploy code without CI passing all workflows.
* Follow Nightshift methodology to reject unreviewed or sloppy code.

---


## 6. Deployment

**Target Environment**

* Bare metal deployment targeting Linux natively. Avoid Docker.
* Applications are strictly kept alive natively using `systemd`.
* Environment variables are specified using `.env` files.
* Test environment variables (including `FIXTURES`) are safely pushed to the repository in `.env.test` for CI execution.
* Bun serves server APIs and static assets.
* Reverse proxy optional; CDN optional.
* Deployment must remain simple; avoid unnecessary complexity.

**CI/CD**

* Separate build pipelines for browser and server.
* Deploy per milestone (Alpha/Beta/V1).
* Logging, monitoring, backups enforced at Beta stage.
* Observability of errors, external integrations, and user actions.

**Constraints**

* Maintain single-stack coherence (TypeScript + Bun + React + Tailwind).
* Follow the dependency discretion policy (Buy vs DIY).
* No polyglot microservices unless explicitly required.
* No direct database calls from browser code.
* AI agents should operate autonomously without requiring human-in-the-loop approval, relying on Nightshift Nags and test validations for safety.

---

## 7. Logging & Telemetry

**The Unified Full-Stack Logger (DIY)**

Because no single `npm` package perfectly satisfies our requirements for LLM-friendly output without excessive configuration, building the native logger is a required architectural step.

* **Browser-to-Server Handoff:** Browser errors (React error boundaries, unhandled rejections, DOM crashes) must be explicitly caught and POSTed back to the Bun server's `/api/logs` endpoint.
* **Distributed Traces:** Every request/interaction must generate a unique `traceId`. This trace must seamlessly follow the user from the browser click down to the database query, allowing perfect chronological reconstruction of any workflow.
* **LLM-friendly `uniques.log`:** In addition to a standard chronological stdout/file log, the server must maintain a `uniques.log` file.
  * This file acts as a Set of errors, deduplicating repetitive alerts.
  * An AI agent inspecting the system should only need to read `uniques.log` to see the *categories* of errors currently afflicting the system, without wasting its token context window scrolling through thousands of identical "Timeout" errors.
* **Retention Policy:** Logs should be rotated (e.g., daily) and kept for a maximum of 14 days on the bare-metal server to prevent disk exhaustion, unless explicitly offloaded to a cold storage solution like S3.


