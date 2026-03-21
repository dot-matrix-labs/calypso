---
name: create-pr
description: Legacy PR helper. Prefer the pr-sync skill and deterministic PR scripts.
user_invocable: true
---

# Create PR

This is a compatibility wrapper.

Preferred split:

- command: `pull-request`
- skill: `pr-sync`
- scripts: `pr-status.sh`, `merge-ready.sh`, `mark-pr-ready.sh`

## Inputs

The user provides: $ARGUMENTS

If empty, infer the issue from the current branch or PR when that inference is straightforward.

## Must do

- Use repository-compliant PR rules.
- Prefer deterministic PR scripts.

## Must not do

- Do not generate a summary-heavy PR body if the repo requires a single closing reference.
- Do not reintroduce obsolete PR-ready semantics here.
