
# Testing Blueprint

> [!IMPORTANT]
> This blueprint defines the testing strategy for AI-agent-built software: what to test, how to test it, and how continuous integration enforces correctness at every commit.

---

## Vision

Tests are the only proof that software works. Code reviews catch style issues and obvious logic errors; type systems catch shape mismatches; linters catch formatting. But only a test that executes the code on the target platform, with real inputs, and asserts real outputs can demonstrate that the software does what it claims to do. Everything else is inference.

In agent-built software, testing carries an additional burden: the agent that wrote the code is not the agent that will maintain it. A future agent — or the same agent in a new session — has no memory of the original intent. The test suite is the executable specification. It encodes what the code must do, under what conditions, and with what results. Without it, every future agent must reverse-engineer intent from implementation, which is how bugs become features and regressions become permanent.

The testing strategy for agent-built software rejects mocking as a foundational practice. A mock replaces a real dependency with a fiction authored by the same developer who wrote the code under test. The mock confirms the developer's assumptions, not reality. When the real dependency behaves differently — and it will — the test passes and the production system fails. Instead, tests run against the real runtime, the real browser engine, and recorded fixtures of real API responses. The cost of ignoring this blueprint is a test suite that provides false confidence: green checkmarks that correspond to nothing in production.

---

## Threat Model

| Scenario | What must be protected |
|---|---|
| Code passes tests on macOS but fails on Linux in production | Environment parity — tests must run on the same OS and runtime as production |
| Mock-based test passes but real API behavior differs | Test validity — tests must exercise real behavior, not developer assumptions |
| Agent generates test fixtures by guessing API response shapes | Fixture accuracy — fixtures must be recorded from real API calls, never fabricated |
| Browser test passes in Node/JSDOM but fails in real Chromium | Browser fidelity — component and E2E tests must run in a real browser engine |
| Test suite passes but critical user workflow is untested | Coverage completeness — test categories must cover unit, integration, component, and end-to-end |
| CI allows merge despite failing tests | Merge gating — no code merges without all test suites passing |
| Flaky test is ignored or disabled instead of fixed | Test reliability — every test must be deterministic; flaky tests are bugs |
| Agent writes tests after implementation, confirming existing behavior rather than specifying intent | Test-first discipline — test stubs are written before features, encoding expected behavior |
| A single CI workflow failure gives no information about which suite failed | Failure diagnosis — each test suite runs in its own workflow for precise identification |

---

## Core Principles

### Never mock anything

A mock is a lie agreed upon between the test author and the test runner. It replaces a real dependency with a controlled fiction, guaranteeing that the test confirms the author's assumptions rather than reality. Instead of mocks, use the real dependency: the real database, the real browser engine, the real file system. For external APIs that cannot be called on every test run, use recorded fixtures captured from real production traffic — not fabricated response objects.

### Test on the target, not a simulation

Code that will run on Linux must be tested on Linux. Code that will render in a browser must be tested in a browser — a real headless browser engine, not a DOM simulation. The delta between the test environment and the production environment is the space where bugs hide. Eliminate the delta entirely.

### Tests are written before code

Test stubs are created during the scaffold phase, before any feature code exists. Each stub encodes expected behavior as a failing test. The agent's job during implementation is to make the tests pass — not to write code and then write tests that confirm what the code already does. Test-first development prevents the agent from building features that cannot be verified.

### Each test suite is independently runnable

A test suite must contain all setup, teardown, and fixture loading it needs. It must not depend on another suite having run first, on shared global state, or on manual environment preparation. Any developer or CI runner can execute any suite in isolation and get a meaningful result.

### Failure must be precise

When a test fails, the failure message must identify which suite, which test, and which assertion failed — without requiring the reader to parse log output or cross-reference multiple files. CI workflows are organized one-per-suite so that a red check immediately names the broken category.

---

## Design Patterns

### Pattern 1: Golden Fixture Recording

**Problem:** Integration tests need realistic API responses, but calling live external APIs on every test run is slow, flaky, rate-limited, and may have side effects. Mocking the responses by hand produces fixtures that reflect the developer's imagination, not reality.

**Solution:** Build a fixture recording tool that makes real HTTP requests to external APIs and serializes the full request/response pairs to disk. These "golden" fixtures are committed to the repository and replayed during tests. The fixtures are periodically refreshed by re-running the recorder against the live API. Tests that use golden fixtures compare actual behavior against known-real behavior.

**Trade-offs:** Fixtures become stale if the external API changes and the recorder is not re-run. Stale fixtures cause tests to pass while production fails. Mitigation: schedule periodic fixture refresh runs in CI and alert on schema drift.

### Pattern 2: Suite-Per-Workflow CI

**Problem:** A monolithic CI workflow that runs all tests in sequence provides a single pass/fail signal. When it fails, the developer must read logs to determine which suite broke. Parallelism within the workflow is complex and fragile.

**Solution:** Create one CI workflow file per test suite. Each workflow is self-contained: it installs dependencies, sets up the environment, runs its suite, and reports results. The merge gate requires all workflows to pass. A red check on "Component Tests" immediately tells the developer where to look.

**Trade-offs:** More workflow files to maintain. Shared setup steps (installing dependencies, building the project) are duplicated across workflows. The duplication is acceptable because it guarantees independence — a change to the component test setup cannot break the unit test workflow.

### Pattern 3: Headless Browser Testing

**Problem:** Browser code must be tested in a browser, but development environments for AI agents have no display server, no GUI, and no way to open a visible browser window.

**Solution:** All browser tests run in headless mode using a browser automation framework. The framework launches a real browser engine (not a DOM simulation) with rendering, layout, JavaScript execution, and network behavior identical to a visible browser — but without a display. Visual output is captured as screenshots for inspection by humans or vision-capable models.

**Trade-offs:** Headless mode cannot test certain display-dependent behaviors (scroll position on physical monitors, GPU-accelerated animations). These edge cases are rare and acceptable to exclude from automated testing.

### Pattern 4: Lint-Then-Test Pipeline

**Problem:** Tests that run against malformatted or lint-violating code produce noise. A test failure caused by a syntax error is not a test failure — it is a code quality failure. Mixing the two signals wastes debugging time.

**Solution:** Every CI workflow runs linting and formatting checks before test execution. If lint or format checks fail, the workflow stops immediately without running tests. The developer fixes code quality issues first, then re-runs. This ordering ensures that test failures always represent behavioral issues, not stylistic ones.

**Trade-offs:** Adds a few seconds to every CI run. The time cost is negligible compared to the clarity gained by separating code quality from behavioral correctness.

---

## Plausible Architectures

### Architecture A: Single-Host Full Suite (solo agent, early-stage)

```
┌──────────────────────────────────────────────────────┐
│  Development Host (Linux)                            │
│                                                      │
│  ┌────────────┐  ┌────────────┐  ┌───────────────┐  │
│  │ Unit Tests │  │ Integration│  │ Browser Tests │  │
│  │ (test      │  │ Tests      │  │ (headless     │  │
│  │  runner)   │  │ (fixtures) │  │  engine)      │  │
│  └─────┬──────┘  └─────┬──────┘  └───────┬───────┘  │
│        │               │                 │           │
│        └───────────────┼─────────────────┘           │
│                        ▼                             │
│              Test Runner CLI                         │
│              (single command, all suites)            │
└──────────────────────────────────────────────────────┘

CI mirrors this exactly on Linux runners.
```

**When appropriate:** Early-stage projects with a single agent. All tests run on the same host. Fast feedback loop.

**Trade-offs:** No parallelism. Full suite runs sequentially. Acceptable when total test time is under five minutes.

### Architecture B: Parallel CI Suites (team, mid-stage)

```
┌─────────────────────────────────────────────────┐
│  CI Platform (triggered on push/PR)             │
│                                                 │
│  ┌──────────────────┐  ┌────────────────────┐   │
│  │ Workflow: Unit   │  │ Workflow: API      │   │
│  │ lint → test      │  │ lint → test        │   │
│  └────────┬─────────┘  └────────┬───────────┘   │
│           │                     │               │
│  ┌────────┴─────────┐  ┌───────┴────────────┐   │
│  │ Workflow:        │  │ Workflow:           │   │
│  │ Component        │  │ Full-Page E2E      │   │
│  │ lint → browser   │  │ lint → browser     │   │
│  │ test             │  │ test               │   │
│  └──────────────────┘  └────────────────────┘   │
│                                                 │
│  Merge gate: ALL workflows must pass            │
└─────────────────────────────────────────────────┘
```

**When appropriate:** Projects with enough tests that sequential execution is too slow. Each suite runs on its own runner in parallel. Failure is immediately localized.

**Trade-offs:** Higher CI cost (four runners instead of one). Setup duplication across workflows. Worth it when total test time exceeds five minutes.

### Architecture C: Fixture Refresh Pipeline (external API dependencies)

```
┌──────────────────────────────────────────────────┐
│  Scheduled CI Job (weekly or on-demand)          │
│                                                  │
│  ┌────────────────────────────────────────────┐  │
│  │  Fixture Recorder                          │  │
│  │  1. Call live external APIs                │  │
│  │  2. Serialize request/response pairs       │  │
│  │  3. Compare against existing fixtures      │  │
│  │  4. Alert on schema drift                  │  │
│  │  5. Commit updated fixtures                │  │
│  └────────────────────────────────────────────┘  │
│                                                  │
│  Integration tests use committed fixtures        │
│  (never call live APIs during test runs)         │
└──────────────────────────────────────────────────┘
```

**When appropriate:** Projects with external API dependencies. Fixtures are the bridge between "never mock" and "don't hit live APIs on every test run."

**Trade-offs:** Requires API credentials in CI for the recorder job. Fixtures may not cover edge cases that only appear under specific conditions. Supplement with manual fixture capture for known edge cases.

---

## Reference Implementation — Calypso TypeScript

> The following is the Calypso TypeScript reference implementation. The principles and patterns above apply equally to other stacks; this section illustrates one concrete realization using Vitest, Playwright, and GitHub Actions.

### Test Categories

| Category | Tool | Runtime | Location |
|---|---|---|---|
| Unit tests | Vitest | Bun (CLI) | `/tests/unit` |
| API integration tests | Vitest + golden fixtures | Bun (CLI) | `/tests/integration` |
| React component tests | Vitest + Playwright | Headless Chromium | `/tests/e2e` (component) |
| Full-page user story tests | Playwright | Headless Chromium | `/tests/e2e` (pages) |

### CI Workflow Structure

```
.github/workflows/
  test-unit.yml
  test-integration.yml
  test-component.yml
  test-e2e.yml
```

Each workflow:
1. Installs Bun
2. Installs dependencies
3. Runs lint + format check
4. Runs its specific test suite
5. Reports pass/fail independently

Merge is blocked unless all four workflows pass.

### Golden Fixture Format

Fixtures are stored in `.env.test` (for credentials) and `/tests/fixtures/` (for recorded responses). The fixture recorder is a Bun script that:
1. Reads API credentials from `.env.test`
2. Makes real HTTP requests to external services
3. Writes response bodies and headers to JSON files in `/tests/fixtures/`
4. Logs schema changes compared to existing fixtures

`.env.test` is committed to the repository (it contains test-only credentials, not production secrets).

### Browser Test Configuration

All browser tests use Playwright in headless mode. Configuration:
- No browser runner or reporter — command-line test execution only
- Screenshot capture for visual evaluation
- No GUI, no display server, no `DISPLAY` environment variable

### Dependency Justification

| Package | Reason | Buy or DIY |
|---|---|---|
| Vitest | Test runner with native ESM and TypeScript support; integrated with Bun | Buy |
| Playwright | Headless browser automation; no viable DIY alternative | Buy |
| ESLint | Linting with ecosystem plugins; infeasible to replicate | Buy |
| Prettier | Deterministic formatting; agent-generated formatter would diverge | Buy |

---

## Implementation Checklist

### Alpha Gate

- [ ] Vitest configured and running for unit tests; at least one passing test
- [ ] Playwright installed with OS dependencies; headless Chromium launches
- [ ] Golden fixture recorder built; at least one external API fixture recorded from live traffic
- [ ] Integration tests run against recorded fixtures, not mocks or fabricated data
- [ ] Component tests run in headless Chromium via Playwright, not JSDOM
- [ ] Full-page E2E test suite has at least one passing test covering a core workflow
- [ ] All four CI workflows created (unit, integration, component, E2E)
- [ ] Each CI workflow includes lint/format check before test execution
- [ ] Merge gate configured: all workflows must pass before merge
- [ ] Test stubs exist for all planned feature areas (failing is expected; missing is not)

### Beta Gate

- [ ] No mocks in any test file; grep for `mock`, `jest.fn`, `vi.fn` returns zero results in test code
- [ ] Fixture refresh pipeline runs on schedule; schema drift alerts configured
- [ ] All four test suites run in under five minutes total in CI
- [ ] Zero flaky tests; any intermittent failure is treated as a bug and fixed immediately
- [ ] Test coverage measured and reported (not gated, but visible)
- [ ] Component tests cover all critical UI components with at least one interaction test each

### V1 Gate

- [ ] Full-page E2E tests cover all user-facing workflows documented in the PRD
- [ ] Golden fixtures refreshed within the last 30 days; no stale fixtures
- [ ] CI passes on every commit to main for the last 50 commits (no broken-window tolerance)
- [ ] Test suite execution time monitored; degradation triggers investigation
- [ ] New features cannot be merged without corresponding test coverage in the appropriate suite
- [ ] Test documentation in `docs/` describes how to run each suite locally and how to add new tests

---

## Antipatterns

- **Mock everything.** Replacing every external dependency with a mock so tests run fast and pass reliably. The tests validate the mock's behavior, not the system's. When the real dependency changes, the tests still pass and production breaks.

- **JSDOM as a browser.** Running component tests in JSDOM or Happy DOM because they are faster than headless Chromium. These are DOM simulations, not browsers. They lack layout, rendering, real event dispatch, and network behavior. A component that passes in JSDOM and fails in Chromium is a component that fails in production.

- **Fabricated fixtures.** Writing API response fixtures by hand based on documentation or memory instead of recording them from live traffic. Handwritten fixtures reflect the developer's understanding of the API, which is always incomplete. The golden fixture recorder exists specifically to prevent this.

- **Monolithic CI workflow.** Running all test suites in a single workflow and scanning logs to find which suite failed. A failed unit test and a failed E2E test produce the same red check, requiring manual log inspection to diagnose. Separate workflows make failure obvious.

- **Tests after features.** Writing tests after the feature is implemented to confirm it works. These tests encode the current behavior, not the intended behavior. Bugs in the implementation become encoded as expected behavior. Test stubs are written first; the feature makes them pass.

- **Ignored flakiness.** Re-running a failed test "because it is flaky" instead of investigating the root cause. Flaky tests are tests with hidden dependencies on timing, ordering, or shared state. Every flaky test is a bug — in the test, in the code, or in the environment.

- **Test environment shortcuts.** Running tests on macOS locally "just to check" before pushing to CI. The tests pass on macOS. They fail on Linux in CI. The developer blames CI instead of recognizing the environment delta. Tests run on the target platform or they don't count.
