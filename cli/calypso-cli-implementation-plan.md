# Calypso CLI Implementation Plan

## Goal

Recover the core Nightshift workflow and prove one thing first:

- headless mode can execute user-defined YAML state machines
- the runtime can loop through project-task workflows without stopping
- terminal logs clearly report states, transitions, steps, and failures
- the system can resume the loop until the user exits the program

This plan is now explicitly headless-first. Web interfaces, richer operator surfaces, and stricter end-state test philosophy are deferred until the basic state-machine runtime is real and stable.

## Immediate priority

We have been failing at the most basic requirement: creating a simple state machine that runs reliably.

The current top priority is therefore:

1. make Nightshift headless mode work with multiple user-authored YAML state machines
2. create several happy-path tests using mocks/fakes at the runtime boundaries
3. fuzz YAML inputs that could break parsing, validation, transition selection, or loop execution
4. ship example state machines that repeatedly iterate project tasks until the user exits

Everything else is secondary until this works.

## In scope now

- headless execution only
- user-authored YAML state-machine loading and validation
- deterministic step execution and transition resolution
- loop-oriented workflows that restart after finishing a task cycle
- terminal log reporting for:
  - state entered
  - step started
  - step completed
  - transition chosen
  - loop restart
  - terminal exit reason
- happy-case tests with mocking/fakes for subprocess, GitHub, and other external boundaries
- fuzzing and regression coverage for problematic YAML inputs

## Explicitly deferred

- web interfaces
- browser operator surfaces
- TUI-first workflow work
- broader doctor/audit expansion
- workspace refactor
- nonessential GitHub workflow integrity work
- broader release/deploy orchestration
- “proper” full-behavior test expansion beyond the recovery slice

## Reset phase model

## Phase 0: Keep the usable foundation

Completed or already-useful work should be preserved where it helps the headless runtime:

- repository-local state persistence
- template loading
- runtime/domain code already extracted into `crates/nightshift`
- existing signal, report, state, interpreter, and headless entry-point code
- existing tests that still match the new priority

Do not spend time expanding UI or architecture purity during this phase.

## Phase 1: Headless state-machine recovery

This is the only priority phase until complete.

### Phase 1.1: YAML contract and loader recovery

- define the minimum supported YAML state-machine schema for headless execution
- support multiple user-authored YAML files, not just the embedded default
- reject invalid or ambiguous configurations with precise errors
- validate:
  - missing states
  - duplicate state names
  - missing start state
  - missing target states
  - empty step lists where steps are required
  - invalid loop targets
  - unsupported action types
- ensure load errors are surfaced clearly in terminal logs

### Phase 1.2: Deterministic headless driver loop

- start from a selected initial state
- execute steps in order
- evaluate transitions deterministically
- move to the next state without TUI dependencies
- continue looping until:
  - the YAML reaches an explicit exit state
  - the user interrupts the process
  - a fatal validation/runtime error occurs
- support a cycle model where “done for now” transitions back to the top-level planning/task-selection state

### Phase 1.3: Terminal reporting and observability

- emit clear logs for every state transition and step boundary
- make headless logs readable in plain text first
- include enough structured data to support later JSON output
- log at minimum:
  - current state
  - current step
  - selected transition
  - iteration counter
  - blocking error
  - shutdown cause
- ensure logs make it obvious that the agentic loop is still progressing and has not stalled

### Phase 1.4: Happy-case tests with mocks/fakes

For this recovery slice, mocking is allowed and required at unstable boundaries so we can prove the state-machine behavior quickly.

- create happy-case tests for:
  - single-path YAML workflow
  - branching YAML workflow
  - looping workflow that restarts after a completed task cycle
  - workflow that waits for a mocked external result and then proceeds
  - workflow that exits cleanly on user interrupt
- use mocks/fakes for:
  - subprocess execution
  - provider responses
  - GitHub or external command boundaries
  - clock/timing where needed
- verify ordered log output and ordered state transitions

### Phase 1.5: YAML fuzzing and regression corpus

- fuzz YAML parsing and validation
- fuzz transition graphs and malformed edge targets
- fuzz pathological values:
  - deeply nested mappings/sequences
  - duplicate keys
  - unknown fields
  - huge scalar values
  - invalid UTF-8/encoding edge cases where applicable
  - recursive/alias-heavy YAML structures if the parser permits them
- save minimized crashing or confusing inputs as named regression fixtures
- ensure bad YAML never causes silent hangs or undefined loop behavior

### Phase 1.6: Example looping state machines

- add several example YAML workflows that demonstrate continuous operation until exit
- examples should include:
  - task intake -> choose task -> execute task -> report -> restart
  - review queue -> process item -> mark result -> restart
  - implementation loop -> inspect backlog -> do work -> summarize -> restart
- examples must be simple enough for users to modify safely
- examples must be covered by tests

## Phase 2: Headless runtime hardening

After Phase 1 works end-to-end:

- persist loop state cleanly across restarts
- improve signal handling and shutdown behavior
- add resumable iteration metadata
- separate user-facing errors from internal diagnostics
- add text and JSON log formatting once the text output is proven useful

## Phase 3: Real integration behavior

Only after the mocked happy paths are stable:

- replace selected mocks with real integration coverage where it adds confidence
- verify real subprocess/provider interactions
- verify repository-local YAML overrides in realistic repos
- verify headless mode under longer-running multi-iteration sessions

This is where “proper test behavior” starts to expand again, after the core loop is stable.

## Phase 4: Operator surfaces after headless works

- restore or improve TUI work only if it helps supervise the already-working headless loop
- keep UI thin and downstream of the runtime
- no UI work should redefine state-machine semantics

## Phase 5: Web interfaces

- design and build web interfaces after the headless runtime is reliable
- reuse the same runtime events and reporting model instead of inventing separate UI-only behavior

## Phase 6: Deferred issue-linked work

Existing issue-shaped roadmap items are moved later behind the core recovery work.

- workspace refactor previously tracked as `#121`
- prior headless-mode roadmap item previously tracked as `#122`
- doctor state-machine audit previously tracked as `#123`
- additional issue-backed roadmap items should remain deferred unless they directly unblock Phase 1

The point is to stop spending roadmap energy on peripheral issues before the basic state machine works.

## Issue-ready breakdown

These should become the next detailed issues, in this order.

### Issue 1: Define the minimum supported headless YAML state-machine spec

Problem:
Current YAML/state-machine behavior is too unclear and too fragile to build against.

Deliverables:

- one documented minimum schema
- one validation pass with actionable errors
- one small set of accepted built-in step/transition types

Acceptance criteria:

- invalid YAML fails fast with a specific message
- invalid graph structure never reaches execution
- at least one user-authored YAML file outside the embedded defaults loads successfully

### Issue 2: Make the headless driver run a basic YAML state machine end-to-end

Problem:
Nightshift still fails at the core runtime loop.

Deliverables:

- initial-state selection
- ordered step execution
- deterministic transition choice
- explicit loop behavior
- explicit exit behavior

Acceptance criteria:

- a simple two-state machine runs without UI dependencies
- a looped workflow can iterate more than once
- runtime exit reasons are visible in logs

### Issue 3: Add terminal logs for state entry, step execution, and transitions

Problem:
When the runtime fails or stalls, operators cannot tell where it stopped.

Deliverables:

- readable headless text logs
- consistent event names
- state/step/transition reporting at every boundary

Acceptance criteria:

- a human can follow one full loop from logs alone
- a failing transition includes the state and step context that caused it

### Issue 4: Create mocked happy-path coverage for core workflow shapes

Problem:
We do not yet have stable tests proving that normal state-machine execution works.

Deliverables:

- mocked/fake happy-path tests for straight-line, branching, looping, and interrupted flows
- assertions for transition order and log output

Acceptance criteria:

- the happy-path suite proves at least three distinct workflow shapes
- tests run without requiring live external services

### Issue 5: Fuzz YAML inputs and capture regression fixtures

Problem:
Malformed YAML can still cause parser, validator, or runtime failures that are hard to predict.

Deliverables:

- fuzz targets for parsing and graph validation
- regression fixtures for minimized failures
- guarantees against hangs on invalid input

Acceptance criteria:

- fuzzing produces no uncontrolled panics
- known-bad YAML samples stay covered by fixtures

### Issue 6: Ship example looping state machines users can copy

Problem:
We still lack simple examples that demonstrate the intended “keep going until exit” model.

Deliverables:

- multiple example YAML files
- one documented project-task iteration loop
- tests that execute the examples

Acceptance criteria:

- examples are valid under the minimum schema
- at least one example loops continuously until interrupted by the user

### Issue 7: Harden resume, shutdown, and iteration tracking

Problem:
A working loop is not enough if it loses context or exits unclearly.

Deliverables:

- persisted iteration metadata
- clean shutdown on interrupt
- restart/resume behavior for headless sessions

Acceptance criteria:

- interrupted runs persist enough state to explain what happened
- resumed runs do not duplicate or skip transitions silently

### Issue 8: Reintroduce broader surfaces and deferred roadmap items

Problem:
UI, audit, and refactor work should only proceed after the core loop is real.

Deliverables:

- re-evaluated TUI scope
- re-evaluated web UI scope
- re-sequenced issue backlog

Acceptance criteria:

- no deferred issue is brought forward unless the headless runtime is already stable

## Recommended build order

1. Issue 1
2. Issue 2
3. Issue 3
4. Issue 4
5. Issue 5
6. Issue 6
7. Issue 7
8. Issue 8

## Success criteria for the current plan

- Nightshift can run a user-authored YAML state machine in headless mode
- at least one example workflow loops through project tasks and starts over
- the loop continues until the user exits
- terminal logs clearly show states, transitions, and steps
- happy-case mocked tests cover the main workflow shapes
- YAML fuzzing catches malformed inputs without hangs or uncontrolled panics

## Current planning rule

If a proposed task does not directly help Nightshift execute looping YAML state machines in headless mode, it belongs in a later phase.
