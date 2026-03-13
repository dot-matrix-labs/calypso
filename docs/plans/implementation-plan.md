# Worktree Implementation Plan

## PR: `feat: build CLI GitHub gate evaluators`

- [x] Read the current PR body for [PR #28](https://github.com/dot-matrix-labs/calypso/pull/28), the feature PRD in [docs/plans/cli-github-gate-evaluators-feature.md](/tmp/calypso-worktrees/feat-cli-github-gate-evaluators/docs/plans/cli-github-gate-evaluators-feature.md), and the CLI prototype plan in [cli/calypso-cli-implementation-plan.md](/tmp/calypso-worktrees/feat-cli-github-gate-evaluators/cli/calypso-cli-implementation-plan.md).
- [x] Confirm the live PR has no current review comments or status checks, and that `mergeStateStatus` is currently `BLOCKED` on Friday, March 13, 2026.
- [x] Add a normalized GitHub pull request snapshot model covering draft state, review status, check status, and mergeability.
- [x] Extend built-in evidence from boolean-only results to status-aware evidence (`passing`, `failing`, `pending`, `manual`) so manual review gates can be represented deterministically.
- [x] Map normalized GitHub snapshot facts into grouped default-template gates for PR existence, ready-for-review state, CI checks, review approval, and mergeability.
- [x] Expose normalized GitHub evidence through the status renderer and TUI operator surface, and persist the snapshot shape in repository state.
- [x] Add or update tests for snapshot parsing, grouped gate evaluation, manual review states, and operator-surface/status rendering.
- [ ] Run `cargo test -p calypso-cli --test github`, `cargo test -p calypso-cli --test state`, `cargo test -p calypso-cli --test app`, and `cargo test -p calypso-cli --test tui`.

## Current Blocker

- `cargo test --offline -p calypso-cli --test github -- --nocapture` fails before compilation because the locked `zmij 1.0.21` dependency is not available in the local offline crates.io index cache. The local cache currently exposes `zmij` through `1.0.19`, while `cli/Cargo.lock` pins `1.0.21`.

## Remaining Work

- [ ] Restore dependency resolution for the locked Rust workspace so the updated CLI test targets can run.
- [ ] Run the targeted GitHub, state, app, and TUI tests and then the broader `cargo test -p calypso-cli` suite.
- [ ] If validation passes, update the PR body/checklist to match the implemented GitHub gate evaluator slice and create small commits for the completed increments.
