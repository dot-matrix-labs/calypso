# Calypso Merge

Use this command to merge a PR only when deterministic checks say it is ready.

### Must do

- Check readiness with the shared scripts first.
- Merge only when checks are green, checklist is complete, and the PR is mergeable.

### Must not do

- Do not merge on judgment alone.
- Do not bypass repository rules.

## Deterministic flow

```bash
.agents/scripts/auto/merge-ready.sh {pr-number}
.agents/scripts/auto/mark-pr-ready.sh {pr-number}
.agents/scripts/auto/merge-pr.sh {pr-number}
```

If the PR is not ready, keep working the issue instead of merging.
