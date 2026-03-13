# CLI Hook Policy Enforcement Feature

## Summary

Extend the methodology and runtime so Calypso can evaluate and enforce hook-driven workflow policy: implementation-plan freshness, next-prompt presence, PR checklist requirements, and repository workflow obligations described in the product spec and template model.

## Problem

The shipped template model already captures state-machine rules and evaluators, but the implementation plan still calls out missing support for hook rules, doctor checks, and workflow requirements in the state-machine model. Without those policy surfaces, the prototype cannot enforce one of the core product claims: that repository process rules are explicit, deterministic, and visible as gates instead of tribal knowledge.

## User Outcome

An operator can see whether the repository satisfies Calypso workflow obligations, including planning-doc hygiene and required workflow automation, and can advance a feature only when those policy gates are satisfied.

## Scope

- Extend the template schema to represent hook rules and workflow requirements.
- Add built-in evaluators for plan presence/freshness, next-prompt presence, and required GitHub workflow files.
- Surface policy results as grouped gates alongside feature implementation gates.
- Expose failing policy evidence in both CLI and TUI-facing APIs.

## Non-Goals

- No automatic hook installation in this slice.
- No mutation of GitHub workflow files beyond evaluation/reporting.
- No expansion into release/deployment policy enforcement yet.

## Functional Requirements

1. The methodology schema must encode hook and workflow requirements as first-class policy gates.
2. Built-in evaluators must be able to inspect repository files deterministically without provider involvement.
3. Policy failures must block advancement the same way other feature gates do.
4. The default shipped template set must include at least one concrete policy rule for planning docs and one for GitHub workflow presence.

## Acceptance Criteria

- A repository missing required planning files or workflow files shows failing policy gates with explicit evidence.
- A compliant repository passes the policy gate group without manual intervention.
- Template validation rejects malformed hook-rule definitions clearly.
- Existing template-loading tests still pass after the schema extension.

## Implementation Notes

- Add the policy model to `cli/src/template.rs` and keep evaluator wiring close to the existing built-in keyword system.
- Prefer repository-relative path checks and deterministic timestamps over shelling out to git when file inspection is enough.
- Keep policy-gate output compact so it remains legible in the TUI.

## Test Plan

### Unit Tests

- schema parsing for valid and invalid hook-rule definitions
- evaluator results for present/missing implementation plan, next prompt, and workflow files
- gate rollup when policy and implementation gates disagree

### Integration Tests

- load the default embedded template set and verify policy rules are registered
- evaluate a temporary repository with and without the required files

### Regression Checks

- ensure existing template and state bootstrap tests keep passing
- verify policy gates appear in the same grouped-gate API the TUI already consumes
