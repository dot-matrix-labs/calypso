# Feature

Use the `new-feature` command flow.

Preferred deterministic flow:

```bash
.agents/scripts/feature/validate-request.sh {feature-json-file}
.agents/scripts/feature/collect-context.sh {feature-json-file}
.agents/scripts/feature/check-duplicates.sh {feature-json-file}
.agents/scripts/feature/validate-issue-json.sh {issue-json-file}
.agents/scripts/feature/render-issue-body.sh {issue-json-file}
.agents/scripts/feature/create-issue.sh {issue-json-file}
.agents/scripts/feature/update-plan.sh {created-issue-json-file}
```

Use the `feature-evaluate` skill only for architecture fit, Plan coherence,
dependencies, and issue scope judgment.
