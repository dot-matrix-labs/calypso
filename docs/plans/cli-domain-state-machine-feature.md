# CLI Domain State Machine Feature

## Summary

Implement the Phase 1 core domain model for `calypso-cli` so the prototype can reason about repository state, feature state, gate groups, agent sessions, and deterministic state transitions without coupling that logic to terminal, filesystem, or provider adapters.

## Problem

The current CLI bootstrap and persistence work can discover repository context and load template/state files, but the product still lacks a first-class domain model for feature orchestration. Without explicit domain types and transition guards, later slices such as gate evaluation, TUI status rendering, and supervised agent scheduling will encode workflow policy in ad hoc adapter code.

## User Outcome

An operator can inspect a repository-local feature unit and get a deterministic answer for:

- what state the feature is in
- which gates are blocking advancement
- which transition is allowed next
- whether an agent session outcome should advance, block, or pause the feature

## Scope

- Add Rust domain types for repository state, feature unit, pull request binding, worktree binding, gate group, gate status, agent session, and terminal agent outcomes.
- Model the prototype feature lifecycle described in `cli/calypso-cli-implementation-plan.md`: `new`, `implementation`, `waiting_for_human`, `ready_for_review`, and `blocked`.
- Add transition validators that consume domain facts rather than shelling out directly.
- Expose a pure domain API that the TUI, runtime, and GitHub evaluators can call.

## Non-Goals

- No TUI redesign.
- No new `gh` integration work beyond what the domain types need for identifiers.
- No multi-feature scheduling or release/deployment state machines in this slice.

## Functional Requirements

1. `calypso-cli` must represent one bound feature unit as branch, worktree, and pull request metadata plus the current feature state.
2. Each workflow state must expose the valid next states and the facts required to reach them.
3. Gate groups must support grouped status calculation for specification, implementation, validation, and merge-readiness surfaces.
4. Agent terminal outcomes must normalize to the product contract: `OK`, `NOK`, or `ABORTED`.
5. Transition logic must remain deterministic and side-effect free.

## Acceptance Criteria

- State transitions are rejected with explicit reasons when required facts are missing.
- A persisted feature record can be loaded into the new domain types without lossy mapping.
- The TUI and other adapters can query grouped gate status through a stable API rather than reimplementing workflow logic.
- Unsupported transitions cannot be constructed by callers without going through validation helpers.

## Implementation Notes

- Keep this slice in `cli/src/state.rs` plus small adjacent domain modules if the file becomes crowded.
- Prefer enums and value objects over stringly typed state fields.
- Isolate serialization concerns at the edges so the domain model stays close to `no_std` compatibility.

## Test Plan

### Unit Tests

- valid transition matrix for every prototype feature state
- invalid transition rejection with human-readable reason strings
- gate grouping and status rollup for mixed pass/manual/blocking inputs
- agent outcome normalization for success, failure, and abort cases

### Integration Tests

- load persisted repository state into the domain model and round-trip it without semantic drift
- verify repository discovery plus state bootstrap produce a bound feature unit with the correct branch/worktree/PR mapping

### Regression Checks

- ensure existing state and TUI tests still pass after the domain types replace ad hoc workflow data
- confirm no adapter needs to parse raw state strings once the new API is wired in
