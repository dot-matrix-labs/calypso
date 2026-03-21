# Calypso Feature

Use this command to intake a new feature, evaluate fit, create a compliant issue,
and append it to the `Plan` issue.

This command owns orchestration. Deterministic scripts handle validation,
duplicate checks, issue rendering, issue creation, and Plan updates. The
internal `feature-evaluate` skill handles product, architecture, and dependency judgment.

## Must do

- Validate the request fields before any GitHub mutation.
- Load the current `Plan` and duplicate-candidate issues before evaluation.
- Use the evaluator skill only for architecture fit, dependencies, and risk.
- Create a compliant feature issue from structured data.
- Add the new feature issue to the `Plan` as a plain issue reference.

## Must not do

- Do not invent missing request fields when they are materially absent.
- Do not create duplicate feature issues when a strong existing match already
  exists.
- Do not add phase, batch, or step metadata to issues or the Plan.
- Do not put checkboxes into the `Plan` issue.

## Deterministic flow

1. Validate and collect context:

```bash
.agents/scripts/feature/normalize-feature-request.sh {feature-json-file}
.agents/scripts/feature/validate-request.sh {feature-json-file}
.agents/scripts/feature/validate-feature-context.sh {feature-json-file}
.agents/scripts/feature/collect-context.sh {feature-json-file}
.agents/scripts/feature/check-duplicates.sh {feature-json-file}
```

2. Use the internal `feature-evaluate` skill to produce structured issue data.
3. Validate and render the issue:

```bash
.agents/scripts/feature/validate-issue-json.sh {issue-json-file}
.agents/scripts/feature/render-issue-body.sh {issue-json-file}
```

4. Create the issue and update the `Plan`:

```bash
.agents/scripts/feature/create-issue.sh {issue-json-file}
.agents/scripts/feature/validate-created-issue.sh {created-issue-json-file}
.agents/scripts/feature/render-plan-entry.sh {created-issue-json-file}
.agents/scripts/feature/update-plan.sh {created-issue-json-file}
.agents/scripts/feature/validate-plan-entry.sh {created-issue-json-file}
```
