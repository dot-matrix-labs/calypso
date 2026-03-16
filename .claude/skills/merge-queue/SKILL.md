---
name: merge-queue
description: Manage the merge queue — verify gates, rebase, merge PRs in dependency order, and update tracking issues.
user_invocable: true
model: opus
---

# Merge Queue

Merge ready PRs in dependency order, verifying CI gates pass before each merge.

## Inputs

The user provides: $ARGUMENTS

If $ARGUMENTS is empty, scan for all open PRs that are marked ready (not draft).

---

## Phase 1: Discover mergeable PRs

```bash
gh pr list --repo sduvignau/calypso-tasks --state open --json number,title,isDraft,headRefName,body
```

Filter to non-draft PRs. For each PR, extract the issue number from `Closes #N` in the body.

---

## Phase 2: Build dependency graph

For each issue linked to a PR:

```bash
gh issue view {issue-number} --repo sduvignau/calypso-tasks --json body -q .body
```

Parse the Dependencies section. Build a dependency graph to determine merge order.
PRs whose dependencies are all closed (or have no dependencies) can be merged first.

---

## Phase 3: Merge in order

For each PR in dependency order:

### Step 1: Verify CI passes

```bash
gh pr checks {pr-number}
```

If CI is failing, skip this PR and report to the user.

### Step 2: Rebase on main

```bash
gh pr update-branch {pr-number} --rebase
```

Wait for CI to re-run after rebase. Check again:

```bash
gh pr checks {pr-number}
```

### Step 3: Merge

```bash
gh pr merge {pr-number} --squash --delete-branch
```

### Step 4: Verify issue closed

```bash
gh issue view {issue-number} --repo sduvignau/calypso-tasks --json state -q .state
```

If the issue didn't auto-close, close it manually:

```bash
gh issue close {issue-number} --repo sduvignau/calypso-tasks
```

---

## Phase 4: Update tracking issue

After all merges, update the Plan tracking issue to reflect the new state. No
format changes needed — the issue links will automatically show as closed.

Report to the user:
- Which PRs were merged (with URLs)
- Which PRs were skipped and why
- Current state of the Plan

---

## Rules

- **Dependency order** — never merge a PR before its dependencies are closed
- **CI must pass** — never merge with failing checks
- **Squash merge** — keep main history clean
- **Delete branch after merge** — no stale branches
- **`gh` CLI only** — all GitHub operations use the gh CLI
