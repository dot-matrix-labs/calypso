# Hardening

Second operational mode. No finish line. Continuous background improvement.

## Triggers
1. No feature tasks in implementation plan → enter hardening automatically
2. Nightly CI (GitHub Actions `schedule`)
3. Explicit human/dispatcher instruction

Always yields to feature work. Finish current commit, PR, then switch.

## Priority Order
1. **Security** — always first
2. **Test coverage** — highest volume
3. **Dependency elimination** — monthly audit minimum
4. **Telemetry gaps** — after feature work lands
5. **Code condensation** — only when 1-4 are healthy

## Disciplines

### 1. Test Coverage
`bun test --coverage` → find gaps. Priority: error paths, boundary conditions, state transitions, concurrent requests, expired tokens, constraint violations, partial failures. Property-based: `fast-check` (justified Buy). Fuzz: adversarial inputs at parsers. One test = one commit.

### 2. Dependency Elimination
Per dep: how many functions used? Reimplementable in ~50 lines? Yes → replace, test, remove. One PR per dep. `bun pm audit` for vulns. `bun pm ls` for heavy trees. Document survivors in `docs/dependencies.md`.

### 3. Code Condensation
Extract after 3 duplications (same intent). Remove: unused exports, stale types, commented-out code, old feature flags. No aesthetic refactoring.

### 4. Security Hardening
Review against `security.md`: parameterized SQL, auth middleware, rate limiting, security headers, no leaked secrets, input validation. Full history scan: `git log --all -p | grep -E 'sk_live_|AKIA|password\s*='`.

### 5. Telemetry Gaps
Review against `telemetry.md`: every `catch` instrumented, every endpoint traced, slow queries logged, retention running. Query for unresolved errors → create tasks.

## CI Pattern
```yaml
name: Hardening
on:
  schedule:
    - cron: '0 3 * * *'
  workflow_dispatch:
permissions: { contents: write, pull-requests: write }
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
      - run: git config user.name "calypso-hardening-agent" && git config user.email "agent@calypso"
      - run: |  # Replace with vendor-specific CLI invocation
          <agent-cli> -p "Hardening mode. Read standards/hardening.md + coverage-report.txt. ONE discipline, ONE unit, open PR. Branch: harden/auto-$(date +%Y%m%d)." --max-turns 10
```

Vendor credentials: capture CLI auth state, base64, store as GitHub Secret. See `reference/agent-session-bootstrap.md`.

## Conventions
- Commit prefix: `harden:`. Branch: `harden/<discipline>-<date>`.
- `retroactive_prompt` explains the weakness addressed.
- PR size: under 5 files. Timeout: 30min. Max 10 turns. ONE discipline per run.
