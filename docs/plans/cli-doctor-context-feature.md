# CLI Doctor Context Feature

## Summary

Extend `calypso-cli doctor` so it validates the full local prototype context: required executables, GitHub auth state, remote configuration, feature binding assumptions, and provider readiness for Codex-backed workflows.

## Problem

The implementation plan makes `doctor` the first-run dependency and repository-context validator, but the current prototype does not yet provide a concise, actionable readiness report for the exact assumptions the product requires. Without that, operators discover missing `gh` auth, invalid remotes, or unavailable provider binaries only after entering feature workflows.

## User Outcome

An operator can run `calypso-cli doctor` and get a short pass/fail report that tells them whether this repository is ready for GitHub-backed, Codex-supervised feature work and exactly what to fix if it is not.

## Scope

- Check required binaries for the prototype, at minimum `gh` and `codex`.
- Validate the repository remote shape expected by GitHub-backed flows.
- Confirm `gh` authentication and repository access are usable.
- Confirm the current branch/worktree context can be mapped into a Calypso feature unit.
- Emit concise remediation messages for every failing check.

## Non-Goals

- No attempt to install missing tools automatically.
- No Kubernetes, secrets, or deployment doctor coverage in this slice.
- No provider support beyond Codex CLI.

## Functional Requirements

1. `doctor` must return a stable result model with individual checks, statuses, and remediation text.
2. The command must distinguish local configuration failures from external auth failures.
3. A clean repository on `main` with valid `gh` and `codex` availability must pass all prototype checks.
4. The output must stay terse enough for terminal-first use while still being machine-testable.

## Acceptance Criteria

- Missing `gh` or missing `codex` produces a failing doctor result with a single clear remediation message per tool.
- An invalid or non-GitHub remote configuration is reported explicitly.
- Failed GitHub auth is detected before feature commands depend on it.
- Existing CLI and doctor tests continue to pass, with new coverage for mixed pass/fail result sets.

## Implementation Notes

- Keep the doctor result model separate from terminal formatting so both TUI and plain CLI surfaces can reuse it.
- Favor deterministic checks that do not depend on the current feature being already bootstrapped.
- Reuse repository-discovery code instead of re-implementing git-root lookup.

## Test Plan

### Unit Tests

- binary lookup success and failure for `gh` and `codex`
- remote validation for valid GitHub SSH/HTTPS remotes and invalid remotes
- doctor summary rendering for pass, partial fail, and total fail cases

### Integration Tests

- simulate authenticated and unauthenticated `gh` command outputs
- verify repository discovery plus doctor aggregation on a temporary git repository

### Regression Checks

- ensure doctor output stays stable enough for scripted consumption
- confirm doctor failures do not prevent basic read-only CLI commands from running
