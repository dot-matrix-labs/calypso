---
name: develop-issue
description: Implement one already-selected, already-prepared issue in its verified worktree and own it through merge.
user_invocable: false
---

# Develop Issue

Implement the selected issue in the verified worktree and keep ownership until the
linked PR is merged.

## Preconditions

Before invoking this skill, the caller must already have:

- selected the issue from the Plan
- prepared the issue with deterministic scripts
- verified the worktree, branch, remote tracking, and PR
- accepted any prep-created bootstrap commit as part of PR initialization

Use these scripts as the source of truth:

```bash
.agents/scripts/auto/verify-issue-prep.sh {issue-number}
.agents/scripts/auto/pr-status.sh {pr-number}
.agents/scripts/auto/issue-status.sh {issue-number}
.agents/scripts/auto/remote-branch-status.sh {branch-name}
.agents/scripts/auto/needs-rebase.sh {pr-number}
.agents/scripts/auto/rebase-issue-branch.sh {pr-number}
.agents/scripts/auto/merge-ready.sh {pr-number}
```

## Must do

- Work only on the selected issue.
- Implement acceptance criteria and test plan items in small increments.
- Push regularly so CI stays current.
- Resolve CI, mergeability, and checklist problems as they appear.
- Rebase the selected branch when deterministic checks say it is behind `origin/main`.
- Mark the PR ready and merge it when deterministic checks allow it.
- Confirm the linked issue closes after merge.

## Must not do

- Do not switch to another issue.
- Do not ask for review before the issue is actually ready.
- Do not leave the PR half-finished for a human to complete.
- Do not rely on intuition for CI or merge readiness when a script can answer it.

## Workflow

1. Read the issue body and understand Behaviour, Acceptance Criteria, and Test Plan.
2. Implement the smallest next missing piece.
3. Push.
4. Re-check:
   - PR status
   - issue checklist state
   - remote branch state
5. If `needs-rebase.sh` says the branch is behind `origin/main`, run `rebase-issue-branch.sh` and push.
6. Fix CI or mergeability issues immediately when they appear.
7. Update issue checklist items and stage when implementation evidence supports it.
8. When `merge-ready.sh` says the PR is ready:
   - run `mark-pr-ready.sh`
   - run `merge-pr.sh`
9. Confirm the issue is closed.

## Stop only when

- the PR is merged and the issue is closed
- or an external blocker remains after repo, plan, CI, and blueprint context have been exhausted
