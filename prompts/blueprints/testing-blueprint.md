
# Testing Blueprint

## Testing Philosophy

**Core Principles**

* **Never mock** anything: no APIs, databases, DOM, or external services.
* **Always test on the environment the code will run in:** Linux for server and browser testing; no Mac/Windows shortcuts.
* Browser code tested only in headless Chromium (Vitest + Playwright). No GUI, no display server, no live browser window — ever.
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

## CI/CD Environment

**Platform**

* GitHub as VCS and CI/CD host.
* GitHub Actions as workflow engine.

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
* Tests are gated: failing lint/format or failed test suite **blocks merge**.

**Test Execution**

* Each workflow runs on Linux runners.
* Browser tests use headless Chromium via Playwright + Vitest. (Do not use browser runner/reporters, just command line tests)
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
