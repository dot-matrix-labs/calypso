# CLI GitHub Gate Evaluators Feature

## Summary

Implement deterministic GitHub-backed gate evaluators for the current feature branch so `calypso-cli` can inspect pull request state, checks, review status, and merge readiness directly through `gh` and surface the results as grouped workflow gates.

## Problem

The product specification treats GitHub as a core control surface, but the current prototype stops at repository discovery and local state bootstrap. Without GitHub gate evaluators, the CLI cannot answer the most important operator questions: whether the current branch has a PR, whether CI is green, whether review is blocking, and whether the feature is ready to advance.

## User Outcome

An operator can run or open the CLI on a feature branch and immediately see:

- the pull request bound to the current feature, if one exists
- whether required CI checks are passing
- whether review or merge conflicts are blocking progress
- which grouped gate is currently failing and why

## Scope

- Add a GitHub status reader that shells out through `gh` for the current branch/PR.
- Implement built-in evaluators for PR presence, draft status, review status, check runs, and mergeability.
- Map those facts into grouped gates aligned with the default methodology template.
- Persist the normalized GitHub snapshot in repository-local state where useful.

## Non-Goals

- No support for non-GitHub forges.
- No background polling daemon.
- No release or deployment gate evaluation in this slice.

## Functional Requirements

1. The CLI must resolve the PR associated with the current feature branch.
2. GitHub facts must be normalized into deterministic gate statuses instead of ad hoc text blobs.
3. Gate evaluation must distinguish between blocking failures, missing data, and manual-review states.
4. The CLI must degrade cleanly when `gh` is unavailable or unauthenticated by returning actionable gate failures.
5. The TUI-facing API must expose both grouped gate status and the concrete GitHub evidence behind each status.

## Acceptance Criteria

- A feature branch with an open draft PR and failing CI shows blocked merge-readiness and validation gates.
- A feature branch with passing checks and approved review surfaces a ready gate state.
- Missing PR or missing `gh` auth returns deterministic, user-actionable failures rather than panics.
- Existing GitHub tests continue to pass after the new evaluators are introduced.

## Implementation Notes

- Build on the existing `cli/src/github.rs` surface instead of inventing a second GitHub adapter.
- Keep parsing strict and schema-focused so `gh` output changes fail loudly in tests.
- Store provider-specific details separately from normalized gate results.

## Test Plan

### Unit Tests

- map `gh` JSON snapshots into gate results for success, draft, failing-check, missing-review, and merge-conflict scenarios
- ensure missing or malformed fields produce explicit evaluator errors

### Integration Tests

- cover current-branch PR resolution using fixed `gh` fixture output
- verify grouped gate rendering input for specification, validation, and merge-readiness groups

### Failure-Mode Tests

- `gh` missing from `PATH`
- `gh` unauthenticated
- current branch has no open PR
- GitHub returns incomplete check-run data
