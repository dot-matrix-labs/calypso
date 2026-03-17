---
name: create-pr
description: Create a pull request that delivers exactly one GitHub issue. Verifies all acceptance criteria are complete before creating.
user_invocable: true
model: opus
---

# Create PR

Create a pull request for a completed feature. This skill verifies that acceptance
criteria are met before creating the PR.

## Inputs

The user provides: $ARGUMENTS

$ARGUMENTS should be the issue number. If empty, ask the user which issue this PR delivers.

---

## Setup

Before running any `gh` issue commands, detect the tasks repository:

```bash
TASKS_REPO=$(gh repo view --json nameWithOwner -q '(.owner.login) + "/" + (.name) + "-tasks"')
```

---

## Phase 1: Verify readiness

1. Fetch the issue:
   ```bash
   gh issue view {issue-number} --repo {tasks-repo} --json title,body -q '.title,.body'
   ```

2. Parse the Acceptance Criteria and Test Plan sections.

3. Check that the current branch has all the implementation committed:
   ```bash
   git status
   git diff main...HEAD --stat
   ```

4. Run the verification suite:
   ```bash
   bunx tsc --noEmit
   bun run lint
   bun run format
   bun --bun vitest run
   ```

5. If any check fails, fix the issue before proceeding.

---

## Phase 2: Create or update PR

If a draft PR already exists for this branch, update it instead of creating a new one.

```bash
gh pr list --head {branch-name} --json number,url
```

### If draft PR exists — update it

```bash
gh pr edit {pr-number} \
  --title "{issue-title}" \
  --body "$(cat <<'EOF'
## Summary

{1-3 bullet points describing what was implemented}

Closes #{issue-number}

## Acceptance criteria

{Copy from issue, with checkboxes checked for completed items}

## Test plan

{Copy from issue, with checkboxes checked for verified items}

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"

gh pr ready {pr-number}
```

### If no PR exists — create one

```bash
git push -u origin {branch-name}

gh pr create \
  --title "{issue-title}" \
  --body "$(cat <<'EOF'
## Summary

{1-3 bullet points describing what was implemented}

Closes #{issue-number}

## Acceptance criteria

{Copy from issue, with checkboxes checked for completed items}

## Test plan

{Copy from issue, with checkboxes checked for verified items}

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

---

## Phase 3: Report

Report to the user:
- PR URL
- CI status
- Any acceptance criteria that are not yet met

---

## Rules

- The PR body MUST include `Closes #{issue-number}` so the issue auto-closes on merge
- Every acceptance criterion must be verified before marking the PR ready
- `gh` CLI is the only GitHub surface
- One PR per issue (1:1:1:1:1 invariant)
