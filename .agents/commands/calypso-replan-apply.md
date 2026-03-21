# Calypso Replan Apply

Use this command to apply a structured sequential plan after evaluation is done.

## Must do

- Rewrite the `Plan` issue from structured data only.
- Keep planned entries as plain issue references.
- Sync `Dependencies` and `Dependents` sections on planned issues.
- Keep all ordering metadata in the `Plan` issue only.

## Deterministic flow

```bash
.agents/scripts/replan/validate-plan-json.sh {plan-json-file}
.agents/scripts/replan/apply-plan.sh {plan-json-file}
.agents/scripts/replan/sync-dependents.sh {plan-json-file}
```

Do not hand-edit the `Plan` issue body when the structured scripts can apply it.
