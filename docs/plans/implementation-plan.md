# Worktree Implementation Plan

## PR: `feat: implement cli feature-start workflow`

- [x] Read PR #27, the feature proposal in [docs/plans/archive/cli-feature-start-workflow-feature.md](docs/plans/archive/cli-feature-start-workflow-feature.md), [cli/spec.md](/tmp/calypso-worktrees/feat-cli-feature-start/cli/spec.md), and [cli/calypso-cli-implementation-plan.md](/tmp/calypso-worktrees/feat-cli-feature-start/cli/calypso-cli-implementation-plan.md).
- [x] Add a first-class `feature-start` command surface to `calypso-cli`.
- [x] Implement deterministic semantic branch derivation from a user-provided feature identifier.
- [x] Enforce clean-`main` base-state validation before mutating git state by default.
- [x] Create the new branch from `main`, create a dedicated worktree under a caller-provided base path, push the branch, create a draft pull request, and seed `.calypso/repository-state.json` in the new worktree.
- [x] Roll back local branch/worktree state on worktree, push, PR-create, or state-bootstrap failures, and return explicit recovery guidance when remote PR state may remain.
- [x] Cover the slice with unit tests for branch naming, base-state rejection, rollback behavior, partial-failure reporting, and success-path state seeding.
- [x] Add an integration test that exercises real git branch/worktree creation with a fake `gh` boundary and validates the seeded repository state.
- [ ] Run `cargo fmt --check` and `cargo test -p calypso-cli`.

## Remaining Work

- [ ] Resolve the sandbox-specific Rust compiler `Invalid cross-device link (os error 18)` failure that occurs while writing `.rmeta` artifacts during `cargo test -p calypso-cli --offline`.
- [ ] Re-run `cargo test -p calypso-cli` once artifact writes succeed and fix any code or test failures surfaced by a full run.
- [ ] Update PR #27 body/checklist to reflect the implemented feature slice after tests pass.
