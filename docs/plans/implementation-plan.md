# Worktree Implementation Plan

## Issue #154: Replace high-level gh subcommands with explicit gh api REST calls

- [x] Add `resolve_owner_repo()` helper that parses origin remote URL
- [x] Add `parse_owner_repo_from_url()` for HTTPS and SSH URL parsing
- [x] Replace `gh repo create` with `gh api --method POST orgs/{org}/repos` in `init.rs`
- [x] Replace `gh auth status` with `gh api /` in `doctor.rs`
- [x] Replace `gh repo view --json` with direct git remote parsing in `doctor.rs`
- [x] Replace `gh pr view <n> --json` with 3 REST calls in `github.rs` (pulls + reviews + check-runs)
- [x] Add `derive_review_decision()` for computing review decision from review list
- [x] Add REST deserialization types (RestPullRequest, RestReview, RestCheckRun, etc.)
- [x] Replace `gh pr list --head --json` with `gh api repos/{o}/{r}/pulls?head=…` in `runtime.rs`
- [x] Replace `gh pr view --json number,url` with REST call in `app.rs`
- [x] Replace `gh pr create --draft` with `gh api --method POST` in `feature_start.rs`
- [x] Replace `gh pr view <branch>` with REST call in `feature_start.rs`
- [x] Replace `gh pr edit` with `gh api --method PATCH` in `feature_start.rs`
- [x] Update all test fixtures to match new `gh api` command shapes
