---
name: merge
description: Merge one ready PR using deterministic readiness checks and repository rules.
user_invocable: false
---

# Merge

Merge one ready PR. Use deterministic scripts for readiness and merge actions.

## Deterministic helpers

```bash
.agents/scripts/auto/merge-ready.sh {pr-number}
.agents/scripts/auto/mark-pr-ready.sh {pr-number}
.agents/scripts/auto/merge-pr.sh {pr-number}
```

## Must do

- Check merge readiness before acting.
- Mark ready only when repository gates allow it.
- Merge only when deterministic readiness says yes.

## Must not do

- Do not merge on intuition.
- Do not bypass CI or checklist gates.
- Do not use this skill to choose work ordering; that belongs to `auto`.

## Stop only when

- the PR is merged
- or the deterministic readiness output says why it cannot merge yet
