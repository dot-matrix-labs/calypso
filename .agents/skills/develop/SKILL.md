---
name: develop
description: Legacy wrapper for the develop flow. Use deterministic selection/prep, then hand off to develop-issue.
user_invocable: true
---

# Develop

This is a compatibility entrypoint.

Preferred split:

- use the `develop` command for orchestration
- use deterministic prep scripts for branch/worktree/PR setup
- use the `develop-issue` skill for implementation through merge

## Inputs

The user provides: $ARGUMENTS

If empty:

1. Select the next issue with `.agents/scripts/dev-loop/plan-next-issue.sh`.
2. Prepare it with `.agents/scripts/dev-loop/ensure-issue-worktree.sh`.
3. Verify it with `.agents/scripts/dev-loop/verify-issue-prep.sh`.
4. Continue with `develop-issue`.

## Must do

- Use deterministic selection and prep first.
- Continue with `develop-issue` only after prep passes.

## Must not do

- Do not use this legacy wrapper as the place to duplicate orchestration logic.

## Preferred replacement

- Command: `develop`
- Skill: `develop-issue`
