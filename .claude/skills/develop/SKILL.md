---
name: develop
description: Pick a task from the Plan tracking issue, create an isolated worktree and branch, implement the feature, then open a PR via /create-pr.
user_invocable: true
model: opus
---

# Develop

Pick a task from the Plan tracking issue, implement it in an isolated worktree, and
deliver it as a PR. This skill enforces the 1:1:1:1:1 invariant (1 issue : 1 branch :
1 PR : 1 subagent : 1 worktree).

## Inputs

The user provides: $ARGUMENTS

If $ARGUMENTS is empty, fetch the Plan tracking issue and show the user the next
unstarted task. Ask which task to work on.

```bash
gh issue list --repo {tasks-repo} --search "Plan" --state open --json number,title
gh issue view {plan-issue-number} --repo {tasks-repo} --json body -q .body
```

---

## Setup

Before running any `gh` issue commands, detect the tasks repository:

```bash
TASKS_REPO=$(gh repo view --json nameWithOwner -q '(.owner.login) + "/" + (.name) + "-tasks"')
```

---

## Phase 1: Select and understand the task

1. Identify the target issue from the Plan tracking issue.
2. Fetch the full issue body:
   ```bash
   gh issue view {issue-number} --repo {tasks-repo} --json title,body,state -q '.title,.body'
   ```
3. Verify all dependencies (issues listed in the Dependencies section) are closed.
   If any dependency is open, tell the user and stop.
4. Read the issue's Behaviour, Acceptance Criteria, and Test Plan sections carefully.
   These define "done".

---

## Phase 2: Create branch, push to remote, and open draft PR

**CRITICAL: The branch MUST be on remote with a draft PR before any implementation
begins. This ensures CI runs on every subsequent push.**

### Step 1: Derive the branch name

Use the pattern: `feat/{issue-number}-{short-kebab-description}`

Example: `feat/12-staff-permissions`

### Step 2: Create the branch from main

```bash
git checkout main
git pull origin main
git checkout -b {branch-name}
```

### Step 3: Push the branch to remote

```bash
git push -u origin {branch-name}
```

### Step 4: Create a draft PR

Create a draft PR immediately so CI is wired up:

```bash
gh pr create \
  --draft \
  --title "{issue-title}" \
  --body "$(cat <<'EOF'
## Summary

Implements #{issue-number}.

## Status

🚧 Work in progress

## Test plan

See #{issue-number} for acceptance criteria and test plan.

🤖 Generated with [Claude Code](https://claude.com/claude-code)
EOF
)"
```

Report the PR URL to the user before continuing.

---

## Phase 3: Implement in isolated worktree

Launch a subagent with `isolation: "worktree"` to do the actual implementation.

The subagent prompt MUST include:
- The full issue body (behaviour, acceptance criteria, test plan)
- The branch name to work on
- The PR number (so the subagent can push to the correct branch)
- Instructions to commit and push regularly so CI provides feedback
- Instructions to follow all project conventions (bun toolchain, pt-BR UI text, etc.)

### Subagent instructions template

```
You are implementing GitHub issue #{issue-number}: {issue-title}

Branch: {branch-name} (already pushed to remote with draft PR #{pr-number})

## Issue specification

{full issue body}

## Instructions

1. Read CLAUDE.md and understand project conventions before writing code.
2. Implement the feature according to the Behaviour and Acceptance Criteria sections.
3. Write tests according to the Test Plan section.
4. Run type-check, lint, format, and tests before each commit.
5. Commit and push regularly — CI runs on every push.
6. When done, push final changes. Do NOT mark the PR as ready — that happens in review.

## Conventions
- Use bun, never npm/npx/yarn
- All UI text in pt-BR
- Follow the existing code patterns in the codebase
```

---

## Phase 4: Verify and finalize

After the subagent completes:

1. Check CI status on the PR:
   ```bash
   gh pr checks {pr-number} --repo {tasks-repo}
   ```

2. If CI fails, investigate and fix (or report to user).

3. Update the PR description with a proper summary of what was implemented.

4. Update the PR from draft to ready:
   ```bash
   gh pr ready {pr-number}
   ```

5. Update the issue stage to "In Review":
   ```bash
   # Fetch current body, replace Stage line, update
   ```

6. Report to the user:
   - PR URL and status
   - CI status
   - Any items from the acceptance criteria that could not be completed

---

## Phase 5: Merge before moving on

**CRITICAL: Do NOT start the next feature until this one is fully merged.**

After Phase 4 completes successfully:

1. Verify ALL acceptance criteria checkboxes are checked on the issue.
2. Verify ALL CI jobs are green on the PR.
3. Run `/merge-queue` to merge the PR.
4. Confirm the issue is closed.
5. Only THEN may you pick the next task from the Plan.

If any step fails, fix it before proceeding. Do NOT skip ahead to another feature.

---

## Rules this skill enforces

- **Sequential development only** — finish one feature completely (CI green, acceptance criteria done, merged) before starting the next. NEVER develop features in parallel.
- **1:1:1:1:1 invariant** — one issue, one branch, one PR, one subagent, one worktree
- **Branch on remote before coding** — draft PR exists before implementation starts, so CI runs on every push
- **Dependencies must be closed** — do not start work on an issue with open dependencies
- **Subagent isolation** — implementation happens in a worktree, never the main checkout
- **Regular pushes** — the subagent commits and pushes frequently for CI feedback
- **`gh` CLI only** — all GitHub operations use the gh CLI
- **Self-service first** — read docs and codebase to answer your own questions. Only escalate to the user if you cannot find the answer after thorough research.
