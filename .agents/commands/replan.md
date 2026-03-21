# Replan

Use this command to rewrite the `Plan` tracking issue for strict sequential
execution.

This is a command, not a free-form brainstorming prompt. It owns the ordering
rules and the compliance checks that must run before replanning.

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

1. Find the open `Plan` tracking issue.
2. Audit all open issues for template compliance and plan-metadata violations.
3. Audit open PRs for one-PR-one-issue compliance and minimal-body compliance.
   Fix straightforward PR body violations before continuing.
4. Load every issue referenced by the Plan into context.
5. Evaluate dependencies:
   - explicit feature dependencies from issue `Dependencies`
   - code and subsystem dependencies implied by the issue scope
6. Rank issues:
   - dependencies first
   - then higher technical risk or unknowns first
   - then the simplest deterministic tie-breaker
7. Rewrite the `Plan` issue in strict sequential order with plain issue links.
8. Update issue `Dependencies` and `Dependents` sections when the plan ordering
   changes.

## Plan output rules

- The `Plan` issue may contain ordering structure.
- Planned items must be plain issue references, not checkboxes.
- The `Plan` is the only place where ordering metadata belongs.
- Individual issues must remain free of plan-step metadata.

## Preferred implementation split

- command: `replan`
- skill: `replan` only as a compatibility wrapper
