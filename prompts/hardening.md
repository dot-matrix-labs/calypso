# Hardening Mode

Second operational mode. No finish line. Continuous background improvement while the project is idle on feature revision.

Hardening never invents product direction. It only improves what exists. If you are unsure whether a change belongs in hardening or features, it belongs in features — put it in the backlog and stop.

---

## Triggers

1. `docs/plans/next-prompt.md` contains no executable feature task → enter hardening automatically
2. Nightly CI (`schedule` cron, 03:00 UTC)
3. Explicit human or dispatcher instruction: "enter hardening mode"

**Yielding to feature work:** When a new feature task appears in `next-prompt.md`, finish the current hardening commit and open its PR, then switch. Never batch hardening and feature work into the same commit or PR.

---

## Session Start Protocol

Before doing anything else, every hardening session must:

1. Read `docs/standards/hardening.md` (this file)
2. Run `bun test --coverage` and save output to `coverage-report.txt`
3. Run `bun pm audit` and save output to `audit-report.txt`
4. Read `docs/plans/implementation-plan.md` to confirm no feature tasks are pending
5. Select exactly ONE discipline from the priority order below
6. Select exactly ONE unit of work within that discipline
7. Execute, commit, open PR — then stop

Do not chain multiple disciplines in one session. Do not open multiple PRs. One PR = one problem fixed.

---

## Priority Order

Work down this list. Do not skip to a lower priority if a higher one has open findings.

1. **Security** — always first; unresolved findings block everything else
2. **Test coverage** — highest sustained volume of work
3. **Dependency elimination** — audit at least monthly
4. **Telemetry gaps** — after each wave of feature work lands
5. **Code condensation** — only when disciplines 1–4 are fully healthy

---

## Discipline 1: Security Hardening

Security findings are blocking. A finding in this discipline means the project is not healthy and lower disciplines must wait.

### Process

1. Run the secret scan: `git log --all -p | grep -E 'sk_live_|AKIA|password\s*=|secret\s*=|token\s*='`
2. Run `bun pm audit` — treat high/critical as blocking, moderate as tasks
3. Review every endpoint for: parameterized SQL, authenticated access, rate limiting, input validation, security headers
4. Review JWT implementation: expiry enforced, algorithm pinned, secret not hardcoded
5. Verify `.env` and `.env.test` are in `.gitignore` and not present in git history
6. Check that `uniques.log` does not contain auth or secret material

### One unit of work

Pick the single most severe finding. Fix it. Write a test that would have caught it. Commit. Open PR.

### Checklist — Security pass is complete when:

- [ ] `git log --all -p` grep returns no secret patterns
- [ ] `bun pm audit` shows zero high or critical vulnerabilities
- [ ] Every SQL query in the codebase uses parameterized statements (no string concatenation)
- [ ] Every authenticated endpoint has middleware applied — no route is accidentally public
- [ ] JWT expiry is enforced and tested; algorithm is explicitly pinned
- [ ] Rate limiting is applied to auth endpoints and any public-facing mutation
- [ ] All HTTP responses include security headers (Content-Security-Policy, X-Frame-Options, X-Content-Type-Options)
- [ ] Input validation exists at every external boundary (request bodies, query params, path params)
- [ ] `.env` and `.env.test` are in `.gitignore` and not present in repo history
- [ ] No secret material appears in `uniques.log` or application logs

### Antipatterns

- **Fixing cosmetic issues and calling it security work.** If it is not an attack surface, it is not a security finding.
- **Patching without a test.** Every security fix must be accompanied by a test that fails before the fix and passes after.
- **Suppressing audit warnings instead of resolving them.** `bun pm audit --ignore` is not a fix.
- **Declaring a dep safe because it is "unlikely to be exploited."** Eliminate or upgrade; do not rationalize.
- **Batch-fixing multiple findings in one commit.** One finding = one commit. Reviewability matters.

---

## Discipline 2: Test Coverage

Coverage work is the highest sustained volume of hardening. It never ends. The target is not 100% — it is meaningful coverage of the paths that actually fail in production.

### Process

1. Run `bun test --coverage` — read the output in `coverage-report.txt`
2. Sort uncovered code by risk: error paths first, then boundary conditions, then happy paths
3. Pick the single uncovered path with the highest failure probability
4. Write the test. Run it. It must pass against existing code (not a failing test for planned work)
5. Commit. Open PR.

### Coverage priority order (within this discipline)

1. Error paths and exception handlers — code that runs when things go wrong
2. Boundary conditions — zero, empty, max, min, off-by-one
3. State transitions — authenticated→unauthenticated, pending→complete, etc.
4. Concurrent/race conditions — two requests modifying the same resource
5. Expired or revoked credentials — tokens, sessions, API keys
6. Constraint violations — duplicate inserts, foreign key failures, type mismatches
7. Partial failures — third-party API returns 200 with error payload, DB write succeeds but log fails

### Property-based and fuzz testing

- `fast-check` is a justified Buy for property-based tests — use it for parsers, validators, and any function with a large input space
- Fuzz adversarial inputs at every parser: URL params, JSON bodies, file uploads, query strings
- Fuzz auth inputs: empty tokens, malformed JWTs, tokens with wrong algorithm, tokens signed with wrong secret

### Checklist — Coverage pass is complete when:

- [ ] `bun test --coverage` run; all uncovered lines in current scope reviewed
- [ ] Selected gap covers an error path, boundary, or state transition (not a trivial happy path)
- [ ] Test exercises real runtime behavior — no mocks, no fabricated responses
- [ ] Test runs in headless Chromium if it touches browser code
- [ ] Test uses recorded golden fixture if it touches an external API
- [ ] Test passes against current code
- [ ] Coverage percentage did not decrease
- [ ] No existing tests were modified to make the new test pass

### Antipatterns

- **Writing tests for already-covered happy paths** to inflate coverage numbers. Read the report; go where coverage is absent.
- **Mocking to avoid setting up real state.** Hardening tests must use real services. A test that mocks the database is not a test of the database integration.
- **Writing a test that passes trivially.** `expect(true).toBe(true)` is not a test. If the test cannot fail, delete it.
- **Adding `.skip()` to a failing test** and marking the PR as coverage improvement. A skipped test is a deleted test.
- **Testing implementation details instead of behavior.** Test what the function does, not how it does it internally. If refactoring the internals breaks the test without changing behavior, the test is wrong.
- **Bundling more than one new test per commit.** One gap = one test = one commit.

---

## Discipline 3: Dependency Elimination

A dependency is a liability. Every package added is a potential supply chain attack, a future breaking change, and a context burden for agents. This discipline systematically reduces that liability.

### Process

1. Run `bun pm ls` — list every dependency and devDependency
2. For each package, answer: how many of its exported functions does this project actually use?
3. If the answer is 1–3 functions: can those be reimplemented in ~50 lines of TypeScript? If yes, plan a replacement
4. Run `bun pm audit` — address any vulnerability finding, even if the dep is otherwise justified
5. Pick one package to eliminate or one vulnerability to patch. Execute. Update `docs/dependencies.md`.

### Replacement process

1. Read the source of the functions you use in the package
2. Write a direct implementation in the appropriate internal module
3. Write tests for the internal implementation using the same inputs/outputs as the package's own tests
4. Replace all callsites
5. Remove the package from `package.json`
6. Verify `bun install --frozen-lockfile` still works and all tests pass
7. Commit. Open PR.

### Checklist — Dependency pass is complete when:

- [ ] `bun pm ls` reviewed; every package is accounted for in `docs/dependencies.md`
- [ ] Each entry in `docs/dependencies.md` states: functions used, why it was not DIY'd, last review date
- [ ] `bun pm audit` shows zero high or critical findings
- [ ] Any eliminated package has no remaining imports anywhere in the codebase (`grep -r "from 'package-name'"` returns nothing)
- [ ] Replacement implementation has tests covering the same cases the package handled
- [ ] `bun install --frozen-lockfile` and full test suite pass after removal
- [ ] `package.json` lockfile is committed alongside the removal

### Antipatterns

- **Removing a package without replacing its functionality.** Runtime errors are not an improvement.
- **Leaving dead imports after removal.** The codebase must be clean; `grep` to verify.
- **Dismissing a package as "too hard to replace" without measuring.** Count the lines. If it is under 50, replace it.
- **Replacing a well-maintained, minimal package with a hand-rolled version that is less tested.** DIY is not always better. The rule is: replace when usage is narrow and the replacement is clearly simpler.
- **Upgrading a vulnerable package and closing the finding without verifying the upgraded version actually fixes the CVE.** Read the advisory.

---

## Discipline 4: Telemetry Gaps

Code that runs silently in production cannot be diagnosed. This discipline ensures that every failure path is instrumented.

### Process

1. Read `uniques.log` — every error category represents a telemetry success (the error was caught and logged) or a symptom (the error is recurring and unresolved)
2. Grep for unhandled rejections and bare `catch` blocks with no logging: `grep -rn "catch" --include="*.ts" | grep -v "logger\|log\|trace\|span"`
3. Check that every HTTP endpoint has a traceId threaded from request to response
4. Check that slow queries are logged (any query over 100ms should emit a warning)
5. Pick the single largest blind spot. Instrument it. Commit. Open PR.

### Checklist — Telemetry pass is complete when:

- [ ] Every `catch` block either re-throws or logs with context (file, function, inputs that caused the error)
- [ ] Every HTTP endpoint emits a traceId in the response header and in all log lines for that request
- [ ] Slow queries (>100ms) emit a warning log with query type and duration — not the full SQL string
- [ ] Browser errors (React error boundaries, unhandled promise rejections) are POSTed to `/api/logs`
- [ ] `uniques.log` is being written and deduplicated correctly — spot-check three recent entries
- [ ] Log retention policy is configured (14-day rotation, no unbounded growth)
- [ ] No log line contains secret material, PII, or full SQL strings with user data

### Antipatterns

- **Adding logs that log on the happy path only.** Logging `"payment processed"` is decoration. Logging `"payment failed: ${error.message}, userId: ${userId}, amount: ${amount}"` is telemetry.
- **Logging the full SQL string.** It will contain user data. Log the query type and duration only.
- **Treating `console.log` as telemetry.** It does not survive log rotation, does not have structured fields, and does not feed `uniques.log`. Use the project's logger.
- **Adding a traceId to the log but not to the HTTP response header.** Correlation requires the ID to travel with the request end-to-end.
- **Logging and swallowing errors** (`catch (e) { log(e); }` with no re-throw or recovery). The caller must know the operation failed.

---

## Discipline 5: Code Condensation

This discipline runs only when disciplines 1–4 are fully healthy. It removes duplication, dead code, and structural waste — but never changes behavior.

### Process

1. Find duplication: same intent appearing three or more times in different places
2. Find dead code: exported symbols with no importers, commented-out blocks, stale feature flags
3. Find structural waste: a file that has grown beyond a single clear responsibility
4. Pick one. Remove or extract. Tests must still pass with no changes to test files.
5. Commit. Open PR.

### Rules

- Three duplications before extraction — not two, not "it might be duplicated soon"
- No aesthetic refactoring: do not rename variables for style, reorder functions for readability, or reformat blocks
- Do not change behavior. If the refactor requires changing a test, stop — the test is testing behavior and the refactor is changing it
- Do not extract abstractions that are only used once after extraction

### Checklist — Condensation pass is complete when:

- [ ] All tests pass with zero changes to test files
- [ ] The extracted or removed code had at least three concrete prior usages (for extraction) or zero usages (for removal)
- [ ] No new abstractions were created that are only used once
- [ ] `bun run build` succeeds
- [ ] No TypeScript errors or lint warnings introduced
- [ ] The change is under 50 lines of net diff (excluding removed dead code)

### Antipatterns

- **Refactoring before 1–4 are healthy.** A codebase with untested security holes does not need cleaner variable names.
- **Extracting shared code that is used in only one place** "for future reuse." YAGNI. Wait for the second concrete usage.
- **Renaming for style.** `getUserById` → `fetchUserById` is not condensation. It is noise.
- **Removing code that looks unused without verifying.** Use `grep -r "symbolName"` across the full repo including tests. Dynamic access patterns can fool static reading.
- **Combining condensation with behavior changes** in the same commit. Reviewers cannot validate a refactor that also changes logic.

---

## CI Pattern

```yaml
name: Hardening
on:
  schedule:
    - cron: '0 3 * * *'
  workflow_dispatch:
permissions:
  contents: write
  pull-requests: write
jobs:
  harden:
    runs-on: ubuntu-latest
    timeout-minutes: 30
    steps:
      - uses: actions/checkout@v4
        with: { fetch-depth: 0 }
      - uses: oven-sh/setup-bun@v2
      - run: bun install --frozen-lockfile
      - run: bun test --coverage > coverage-report.txt 2>&1 || true
      - run: bun pm audit > audit-report.txt 2>&1 || true
      - run: git config user.name "calypso-hardening-agent" && git config user.email "agent@calypso"
      - run: |
          # Replace with vendor-specific CLI invocation.
          # Pass both reports as context so the agent can select the right discipline.
          <agent-cli> \
            -p "Hardening mode. Read standards/hardening.md. Review coverage-report.txt and audit-report.txt. Select ONE discipline at the top of the priority order that has open work. Select ONE unit within it. Fix it. Commit with prefix 'harden:'. Open a PR named harden/auto-$(date +%Y%m%d). Stop after one PR." \
            --max-turns 10
```

Store vendor CLI credentials as base64-encoded GitHub Secrets. See `reference/agent-session-bootstrap.md` for vendor-specific auth setup.

---

## Conventions

| Item | Rule |
|---|---|
| Commit prefix | `harden:` |
| Branch name | `harden/<discipline>-<YYYYMMDD>` (e.g. `harden/coverage-20260308`) |
| PR size | Under 5 files changed |
| Session timeout | 30 minutes |
| Max agent turns | 10 |
| Disciplines per run | 1 |
| PRs per run | 1 |
| `retroactive_prompt` | Describes the specific weakness addressed and why it was the highest priority finding |

**`retroactive_prompt` example (correct):**
> "The `/api/users` endpoint accepted arbitrary SQL ORDER BY column names passed directly from query params, enabling SQL injection. Added parameterized column allowlist and a test that verifies the injection pattern is rejected."

**`retroactive_prompt` example (wrong):**
> "Security hardening pass. Fixed SQL issues."

---

## Global Antipatterns

These apply across all disciplines and are grounds for rejecting a hardening PR.

- **Mixing disciplines in one PR.** A PR that adds a test, removes a dependency, and updates a log statement is three PRs.
- **Hardening without a test.** Every hardening commit must include or update at least one test. If the discipline does not produce a testable artifact (e.g. removing dead code), the test suite must still pass and coverage must not decrease.
- **Inventing scope.** Hardening does not add new features, new endpoints, new UI components, or new configuration options. If the fix requires any of these, create a feature task and stop.
- **Silencing instead of fixing.** `// eslint-disable`, `.skip()`, `as any`, `// @ts-ignore` are not fixes. They are debts.
- **PRs that cannot be reviewed in under 5 minutes.** If a reviewer cannot understand what was wrong, what was changed, and why it is now better in under 5 minutes, the PR is too large or too poorly described.
- **Running hardening and feature work in the same session.** Context contaminates. Separate sessions, separate branches, separate PRs.
