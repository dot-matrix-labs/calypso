---
name: replan
description: Legacy wrapper for the replan command. Audit issue and PR compliance, then rewrite the Plan for strict sequential execution.
user_invocable: true
---

# Replan

This is a compatibility wrapper.

Preferred split:

- command: `replan`

## Must do

- Audit all open issues for template compliance before reprioritizing.
- Audit PRs so each PR closes exactly one issue and the PR body contains only the
  closing reference.
- Replan for strict sequential execution only.
- Keep ordering metadata in the `Plan` issue, not in individual issue titles or
  issue bodies.

## Must not do

- Do not plan parallel work.
- Do not use `batch-*` labels or issue title metadata to encode ordering.
- Do not leave non-compliant issue or PR formatting in place if the correction is
  straightforward.

## Preferred replacement

- Command: `replan`
