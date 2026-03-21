---
name: replan
description: Legacy wrapper for the replan command. Prefer deterministic replan scripts plus the replan-evaluate skill.
user_invocable: true
---

# Replan

This is a compatibility wrapper.

Preferred split:

- command: `replan`
- command: `replan-audit`
- command: `replan-apply`
- skill: `replan-evaluate`

## Must do

- Audit all open issues for template compliance before reprioritizing.
- Audit PRs so each PR closes exactly one issue and the PR body contains only the
  closing reference.
- Replan for strict sequential execution only.
- Keep ordering metadata in the `Plan` issue, not in individual issue titles or
  issue bodies.
- Prefer deterministic scripts for audits, collection, and apply steps.
- Use the normalizer scripts before evaluation when straightforward compliance
  fixes are possible.
- Validate evaluator output before applying it.

## Must not do

- Do not plan parallel work.
- Do not use `batch-*` labels or issue title metadata to encode ordering.
- Do not leave non-compliant issue or PR formatting in place if the correction is
  straightforward.

## Preferred replacement

- Command: `replan`
- Skill: `replan-evaluate`
