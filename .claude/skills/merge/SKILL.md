---
name: merge
description: Merge ready PRs in dependency order — verify CI gates, rebase if behind, merge, update tracking issue.
user_invocable: true
model: opus
---

# Merge

Merge ready PRs in dependency order. No unnecessary questions — just verify gates, merge, and report.

## Setup

```bash
REPO=$(gh repo view --json nameWithOwner -q .nameWithOwner)
```

## Step 1: Discover the queue

```bash
gh pr list --repo $REPO --state open \
  --json number,title,body,headRefName,mergeable,isDraft,statusCheckRollup
```

For each non-draft PR, extract `Closes #N` from the body. Classify:

- **Ready** — all CI green, all issue checkboxes checked, not conflicted
- **Blocked** — CI failing, unchecked boxes, or merge conflict (skip, report why)

Only Ready PRs enter the queue.

## Step 2: Order by dependency

1. `Depends-on: #N` in PR body → that PR merges first
2. `_(requires #N)_` in linked issue body → dependency PR merges first
3. Earlier phase in the Plan tracking issue → merges first
4. Tie-break: older creation date first

No user confirmation needed for a single-PR queue. For multiple PRs, show the
order and proceed immediately unless there's ambiguity.

## Step 3: For each PR in order

### 3a. Gate check (re-fetch, don't use cached state)

```bash
gh pr checks {number} --repo $REPO
gh issue view {issue} --repo $REPO --json body -q .body  # count unchecked boxes
```

Fail fast: skip if any CI check is not SUCCESS, or if any `- [ ]` remains in the issue.

### 3b. Rebase if behind main

```bash
git fetch origin
git checkout {branch}
git rebase origin/main
git push --force-with-lease origin {branch}
```

Wait for CI after rebase. If CI fails, re-trigger once (close+reopen). If it fails
again, skip and move on.

### 3c. Merge

```bash
gh pr merge {number} --repo $REPO --merge --delete-branch
```

Merge commits only — no squash, no rebase-merge.

### 3d. Post-merge

1. Confirm issue auto-closed: `gh issue view {issue} --repo $REPO --json state`
2. Update Plan tracking issue: change `- [ ] #{issue}` → `- [x] #{issue}` in its body.
3. Check remaining queued PRs for new conflicts; rebase if needed.
4. Move to next PR.

## Step 4: Report

- PRs merged (issue numbers and URLs)
- PRs skipped and why
- Plan tracking issue updated

## Hard rules

- CI must be green — no exceptions
- All linked issue checkboxes must be checked — no exceptions
- Merge commits only (`--merge`)
- Never force-push without `--force-with-lease`
- Never close issues manually — `Closes #N` handles it
- 1 issue : 1 branch : 1 PR — if broken, stop and report
