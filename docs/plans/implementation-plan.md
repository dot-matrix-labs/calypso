# Worktree Implementation Plan

## Issue #155: Add merge-queue GitHub Actions workflow to scaffold

- [x] Create `.github/workflows/merge-queue.yml` (merge_group + workflow_dispatch)
- [x] Create `calypso-blueprint/examples/github-workflows/merge-queue.yml`
- [x] Add `WORKFLOW_MERGE_QUEUE` const and include in scaffold/refresh lists in `init.rs`
- [x] Update `ruleset.json` with `merge-queue` required status check
- [x] Update init tests: counts (9→10), assertions, YAML validity test
- [x] Commit and push calypso-blueprint submodule changes
