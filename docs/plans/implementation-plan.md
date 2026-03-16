# Worktree Implementation Plan

## Issue #152: After `calypso init`, state machine audit reports zero errors

- [x] Fix `calypso-release-request.yaml`: remove `job:` and `step:` references from checks
- [x] Fix `calypso-release-request.yaml`: mark proposed checks (`docker-image-built`, `docker-image-pushed-to-ghcr`, `smoke-test-passed`) with `status: proposed`
- [x] Fix `calypso-release-request.yaml`: update `needs:` in `build-release-bundle` to `[build]`
- [x] Fix `calypso-release-request.yaml`: update `github_actions.current.jobs` to `[build, publish]`
- [x] Add noop example GHA workflow files to calypso-blueprint (`rust-quality.yml`, `rust-unit.yml`, `rust-integration.yml`, `rust-e2e.yml`, `rust-coverage.yml`, `release-cli.yml`)
- [x] Commit and push calypso-blueprint changes on `feat/align-workflows-to-calypso-cli`
- [x] Add `status` field to `CheckConfig` in `blueprint_workflows.rs`
- [x] Update `sm_audit.rs` to skip checks with `status: proposed`
- [x] Add `format_errors` method to `StateMachineAudit`
- [x] Update `init.rs`: add `include_str!` constants for new workflow files
- [x] Update `init.rs`: extend `scaffold_github_actions` to write all 9 workflows
- [x] Update `init.rs`: extend `refresh_workflows` to write all 9 workflows
- [x] Add `real_init_state_machine_audit_passes` integration test
- [x] Fix `scaffold_github_actions_writes_three_workflow_files` (3 → 9)
- [x] Fix `scaffold_github_actions_skips_existing_workflow_files` (2 → 8)
- [x] Fix `refresh_workflows_overwrites_all_three_files` (3 → 9)
- [x] Bump calypso-blueprint submodule pin
