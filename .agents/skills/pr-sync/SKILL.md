---
name: pr-sync
description: Keep one issue-linked PR aligned with repository rules using deterministic checks.
user_invocable: false
---

# PR Sync

Use this skill when a PR already exists and needs to be aligned with issue and
repository rules.

## Must do

- Verify the PR closes exactly one issue.
- Keep the PR body to the single closing reference only.
- Verify the linked issue checklist is complete before marking ready.
- Use deterministic scripts for readiness checks.

## Must not do

- Do not invent extra PR body structure if the repo rule requires a single closing reference.
- Do not add progress notes, summaries, or duplicated issue text to the PR body.
- Do not mark ready or merge while checks are pending or failing.

## Deterministic helpers

```bash
.agents/scripts/auto/pr-status.sh {pr-number}
.agents/scripts/auto/needs-rebase.sh {pr-number}
.agents/scripts/auto/merge-ready.sh {pr-number}
.agents/scripts/auto/mark-pr-ready.sh {pr-number}
```
