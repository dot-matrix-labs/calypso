---
name: new-feature
description: Legacy wrapper for the new-feature command. Prefer deterministic feature scripts plus the feature-evaluate skill.
user_invocable: true
---

# New Feature

This is a compatibility wrapper.

Preferred split:

- command: `new-feature`
- command: `feature`
- skill: `feature-evaluate`

## Must do

- Validate the request fields before any GitHub mutation.
- Use deterministic feature scripts for duplicate checks, issue rendering, issue
  creation, and Plan updates.
- Normalize requests and validate created issues and Plan entries with scripts.
- Use the evaluator skill only for architecture fit, dependency, and scope
  judgment.
- Keep the `Plan` issue as the only source of ordering metadata.

## Must not do

- Do not use the old phase-confirmation workflow.
- Do not create feature issues manually when the feature scripts can do it.
- Do not put checkboxes, phases, or step metadata into the `Plan`.

## Preferred replacement

- Command: `new-feature`
- Skill: `feature-evaluate`
