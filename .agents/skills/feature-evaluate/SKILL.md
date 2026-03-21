---
name: feature-evaluate
description: Evaluate a feature request against PRD, blueprint, duplicates, and the current Plan, then emit structured issue JSON.
user_invocable: false
---

# Feature Evaluate

This skill is the only non-deterministic part of feature intake.

Input should come from the deterministic feature scripts:

```bash
.agents/scripts/feature/normalize-feature-request.sh
.agents/scripts/feature/validate-request.sh
.agents/scripts/feature/validate-feature-context.sh
.agents/scripts/feature/collect-context.sh
.agents/scripts/feature/check-duplicates.sh
```

## Must do

- Evaluate PRD alignment, blueprint fit, and Plan coherence.
- Respect `context_validation`; if deterministic context is not valid, do not
  proceed as if the repo state were trustworthy.
- Infer explicit dependencies needed before the feature can be implemented.
- Emit a structured issue payload suitable for deterministic validation and
  creation.
- Prefer the smallest clear scope that fits the request.
- Distinguish exact duplicates from likely follow-up or improvement candidates.
- Distinguish:
  - exact duplicate
  - likely overlap
  - improvement/follow-up to a prior closed issue

## Must not do

- Do not perform GitHub writes directly when a feature script can do it.
- Do not emit free-form markdown as the primary output.
- Do not encode Plan order metadata in the issue title or body.
- Do not ignore strong duplicate signals from the deterministic context.
- Do not create a new feature when the request is better modeled as improving an
  existing issue.

## Output contract

Emit JSON with this shape:

```json
{
  "title": "feat: example feature",
  "motivation": "Why this feature is needed.",
  "behaviour": "Observable user-facing behaviour.",
  "dependencies": [196],
  "scope": {
    "in": ["One", "Two"],
    "out": ["Three"]
  },
  "acceptance_criteria": ["Criterion one", "Criterion two"],
  "test_plan": ["Scenario one", "Scenario two"],
  "stage": "Specified"
}
```
