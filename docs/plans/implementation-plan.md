# Worktree Implementation Plan

## Issue #155: Add merge-queue GitHub Actions workflow to scaffold

- [x] Create `.github/workflows/merge-queue.yml` (merge_group + workflow_dispatch)
- [x] Create `calypso-blueprint/examples/github-workflows/merge-queue.yml`
- [x] Add `WORKFLOW_MERGE_QUEUE` const and include in scaffold/refresh lists in `init.rs`
- [x] Update `ruleset.json` with `merge-queue` required status check
- [x] Update init tests: counts (9→10), assertions, YAML validity test
- [x] Commit and push calypso-blueprint submodule changes
- [x] Fix MSRV (1.88.0) rustfmt formatting

## Issues #150, #167: Working TUI surfaces, workflow navigator, headless tracing

- [x] Create GH issue #167 for loading feature state into AppShell
- [x] Implement WorkflowNavigator from WorkflowInterpreter entry points
- [x] Load feature state into AppShell on startup (SM + Agents tabs)
- [x] Add periodic state refresh (2s) in event loop
- [x] Wire WorkflowNavigator into AppShell SM tab
- [x] Change headless default verbosity to Debug
- [x] Add pre-step and post-step transition tracing in headless driver loop
- [x] Add tests (11 new navigator tests, 1 new headless test)
