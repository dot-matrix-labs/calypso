# Calypso Replan Audit

Use this command to run deterministic compliance checks before any replanning.

## Must do

- Audit all open issues for required headings and checkbox sections.
- Audit issue titles and bodies for forbidden plan-order metadata.
- Audit PRs for one-PR-one-issue compliance.
- Audit PR bodies so they contain only a single closing reference.
- Audit merged PRs so their linked issue is closed.

## Deterministic flow

```bash
.agents/scripts/replan/audit-issues.sh
.agents/scripts/replan/audit-prs.sh
.agents/scripts/replan/normalize-issue-template.sh
.agents/scripts/replan/normalize-pr-body.sh
```

Do not continue into ranking or plan rewriting until these audits are clean or
all straightforward fixes have been applied.
