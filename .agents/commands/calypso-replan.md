# Calypso Replan

Use this command to rewrite the `Plan` tracking issue for strict sequential
execution.

This command owns orchestration. Deterministic scripts handle compliance,
collection, and apply steps. The internal `replan-evaluate` skill handles only dependency and risk
judgment.

## Must do

- Read the open `Plan` tracking issue.
- Audit all open issues for template compliance before ranking work.
- Audit open PRs for repository compliance before ranking work.
- Evaluate feature and code dependencies across all planned issues.
- Break ties by prioritizing the issues with the highest technical risk or
  unknowns.
- Plan strictly one issue at a time. Parallel execution is forbidden.
- Rewrite the `Plan` issue so it is the single source of truth for ordering.

## Must not do

- Do not introduce phases, steps, batches, or concurrency metadata into issue
  titles or issue bodies.
- Do not add phase or batch labels to issues.
- Do not plan any parallel execution.
- Do not leave non-compliant issue or PR formatting unaddressed before rewriting
  the Plan.
- Do not put progress summaries or duplicate issue content into PR bodies.

## Compliance rules

Feature issues must follow the repository template:

- expected headings must be present
- `Acceptance criteria` must contain checkboxes
- `Test plan` must contain checkboxes
- issue titles may keep a normal scope prefix like `feat:` or `fix:`
- issue titles must not contain plan metadata such as `Phase`, `Batch`, `Step`,
  or similar ordering tags
- issue bodies must not contain plan-order metadata that needs maintenance

PRs must follow repository PR rules:

- one PR closes exactly one issue
- the PR body must contain only the issue closing reference, for example
  `Closes #123`
- merged PRs are expected to close their linked issue

## Command flow

1. Audit compliance:

```bash
.agents/scripts/replan/audit-issues.sh
.agents/scripts/replan/audit-prs.sh
.agents/scripts/replan/normalize-issue-template.sh
.agents/scripts/replan/normalize-pr-body.sh
```

2. Collect structured input:

```bash
.agents/scripts/replan/collect-plan-issues.sh
.agents/scripts/replan/collect-open-prs.sh
.agents/scripts/replan/rank-input.sh
```

3. Use the internal `replan-evaluate` skill to produce structured sequential ordering.
4. Apply the result:

```bash
.agents/scripts/replan/validate-plan-json.sh {plan-json-file}
.agents/scripts/replan/apply-plan.sh {plan-json-file}
.agents/scripts/replan/sync-dependents.sh {plan-json-file}
```

## Plan output rules

- The `Plan` issue may contain ordering structure.
- Planned items must be plain issue references, not checkboxes.
- The `Plan` is the only place where ordering metadata belongs.
- Individual issues must remain free of plan-step metadata.
