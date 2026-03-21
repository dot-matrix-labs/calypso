---
name: develop
description: Pick a task from the Plan tracking issue, create an isolated worktree and branch, implement the feature, then open a PR via /create-pr.
user_invocable: true
---

# Develop

Pick the selected issue, prepare its dedicated branch/worktree/PR deterministically,
implement it in that isolated worktree, and stay on that issue until the PR is merged.
This skill enforces the 1:1:1:1:1 invariant (1 issue : 1 branch : 1 PR : 1 subagent :
1 worktree).

## Inputs

The user provides: $ARGUMENTS

If $ARGUMENTS is empty, fetch the Plan tracking issue and select the next eligible
issue from the earliest open batch. Do not ask the user when the next issue is
straightforward from the plan and dependency state.

Prefer the deterministic selector first:

```bash
.agents/scripts/dev-loop/plan-next-issue.sh
```

If the caller already selected an issue from the Plan, continue with that issue even
if other lower-priority PRs are already open.

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
   If any dependency is open, return to the Plan and choose the next eligible issue
   instead of stopping, unless the caller explicitly pinned this exact issue.
4. Read the issue's Behaviour, Acceptance Criteria, and Test Plan sections carefully.
   These define "done".
5. If the next step is still unclear, situate the issue in the Plan first and read
   the relevant parts of `calypso-blueprint/` before asking the user.

---

## Phase 2: Deterministic prep before development

Before research or coding begins, prepare and verify the issue with:

```bash
.agents/scripts/dev-loop/ensure-issue-worktree.sh {issue-number}
.agents/scripts/dev-loop/verify-issue-prep.sh {issue-number}
```

Preparation is not optional. Do not begin implementation until verification says:

- the issue has a dedicated worktree
- the issue has a dedicated branch with issue-aligned semantics
- the branch exists on remote and tracks it
- the PR exists
- newly created issue branches were based on the latest `origin/main`

Use the returned worktree path and branch as the only execution target for the issue.

---

## Phase 3: Implement in isolated worktree

Launch a subagent with `isolation: "worktree"` to do the actual implementation in
the verified worktree.

The subagent prompt MUST include:
- The full issue body (behaviour, acceptance criteria, test plan)
- The branch name to work on
- The PR number (so the subagent can push to the correct branch)
- Instructions to commit and push regularly so CI provides feedback
- Instructions to follow all project conventions (bun toolchain, pt-BR UI text, etc.)

### Subagent instructions template

```
You are implementing GitHub issue #{issue-number}: {issue-title}

Branch: {branch-name} (already pushed to remote with PR #{pr-number})
Worktree: {worktree-path}

## Issue specification

{full issue body}

## Instructions

1. Read AGENTS.md and understand project conventions before writing code.
2. Implement the feature according to the Behaviour and Acceptance Criteria sections.
3. Write tests according to the Test Plan section.
4. Run type-check, lint, format, and tests before each commit.
5. Commit and push regularly — CI runs on every push.
6. Use deterministic scripts to check remote branch and PR status instead of inferring them.
7. Fix CI and mergeability issues as they appear.
8. Keep working until the issue is complete, the PR is ready, and the PR can be merged without human help.

## Conventions
- Use bun, never npm/npx/yarn
- All UI text in pt-BR
- Follow the existing code patterns in the codebase
```

---

## Phase 4: Verify and finalize

The development thread owns the issue through merge.

Use deterministic status scripts throughout:

```bash
.agents/scripts/dev-loop/pr-status.sh {pr-number}
.agents/scripts/dev-loop/issue-status.sh {issue-number}
.agents/scripts/dev-loop/remote-branch-status.sh {branch-name}
.agents/scripts/dev-loop/merge-ready.sh {pr-number}
```

Responsibilities:

1. Push small increments continuously.
2. Resolve CI failures as they appear.
3. Update issue checklist items and stage when implementation evidence supports it.
4. Mark the PR ready when repository gates allow it.
5. Merge the PR when `merge-ready.sh` reports ready and repository policy allows it.
6. Confirm the linked issue closes after merge.

---

## Phase 5: Merge before moving on

**CRITICAL: Do NOT start the next feature until this one is fully merged.**

The skill is complete only when:

1. The issue checklist is complete.
2. The PR checks are green.
3. The PR is ready.
4. The PR is merged.
5. The issue is closed.

If any step fails, fix it before proceeding. Do not hand off a half-finished PR to the human.

---

## Rules this skill enforces

- **Sequential development only** — finish one feature completely (CI green, acceptance criteria done, merged) before starting the next. NEVER develop features in parallel.
- **1:1:1:1:1 invariant** — one issue, one branch, one PR, one subagent, one worktree
- **Deterministic prep before coding** — branch, worktree, remote, and PR must be verified before implementation starts
- **New branches start from latest main** — when a new issue branch is created it must be based on current `origin/main`
- **Dependencies must be closed** — do not start work on an issue with open dependencies
- **Subagent isolation** — implementation happens in a worktree, never the main checkout
- **Regular pushes** — the subagent commits and pushes frequently for CI feedback
- **`gh` CLI only** — all GitHub operations use the gh CLI
- **Self-service first** — read docs and codebase to answer your own questions. Only escalate to the user if you cannot find the answer after thorough research.
- **Low-risk autonomy first** — if the next issue or next step is obvious from the Plan, PR, and dependency state, proceed without clarification
- **Own the issue through merge** — the development thread does not stop at “ready for review”; it merges when the repository gates allow it
