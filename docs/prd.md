# Calypso Product Requirements Document

<!-- last-edited: 2026-03-30 -->

## 1. Product Summary

Calypso is a daemon for running and monitoring engineering workflows.

Its core job is to execute workflows expressed in a universal format, persist their state, supervise agentic work, evaluate deterministic gates, and keep long-running automation resumable and inspectable.

Calypso is not the coding agent. It is the process runtime that decides what workflow is active, what state it is in, which step is allowed to run next, and whether execution should continue, pause, retry, or escalate.

The product has two primary surfaces:

- `calypso`: a headless CLI for launching, inspecting, debugging, and controlling workflow execution
- `calypso-web`: a supporting web app for visualizing daemon state, workflow diagrams, status checks, and steering stuck agentic jobs

## 2. Product Goals

### Primary goals

- Run engineering workflows continuously as explicit state machines.
- Use one workflow definition format across local execution and git-forge execution environments such as GitHub Actions.
- Support workflows that combine agentic steps and deterministic programmable steps.
- Persist workflow state so the daemon can recover cleanly after restart, failure, or operator interruption.
- Keep the default runtime headless and scriptable.
- Provide clear observability into current workflow, current state, recent transitions, pending actions, and blocking conditions.
- Provide a browser-based control surface for inspection and operator steering without making the daemon itself interactive.
- Make stuck or ambiguous agentic work recoverable through explicit steering rather than silent failure.

### Product thesis

Engineering automation should be modeled as a deterministic state machine with explicit escape hatches for agentic work, not as a pile of ad hoc scripts or a permanently interactive operator session.

## 3. Design Principles

1. **Daemon-first**
   - Calypso is designed to run unattended.
   - Human involvement is an exception path for review, approval, or steering.

2. **One workflow model**
   - A workflow should mean the same thing whether it runs locally or on a forge runner.
   - Source and execution locality must not require different authoring models.

3. **State machines over implicit control flow**
   - Every workflow must have explicit states, transitions, terminal conditions, and recovery paths.
   - Runtime progress is derived from state, not from log scraping or best-effort inference.

4. **Deterministic orchestration over agent autonomy**
   - Calypso decides when an agent may run and what outcome is required.
   - Agents perform bounded work inside the workflow, not open-ended orchestration.

5. **Headless by default**
   - Core operations must be scriptable, automatable, and CI-safe.
   - Interactive debugging exists only as a diagnostic aid.

6. **Observable and steerable**
   - Long-running workflows must expose enough state for operators to understand what is happening and intervene when needed.

## 4. Primary Users

### Platform / automation engineer

Defines workflows, runs the daemon, and integrates Calypso with local environments and git-forge automation.

### Engineering lead

Uses workflow state, diagrams, and status views to understand progress and unblock stuck jobs.

### Operator

Monitors the daemon, inspects failures, triggers retries or steering actions, and uses debugging tools when diagnosing execution issues.

## 5. Core Product Model

### 5.1 Daemon

The daemon is the long-running Calypso runtime. It loads workflow definitions, watches for triggers, executes states, persists progress, and resumes incomplete work after interruption.

### 5.2 Workflow

A workflow is the canonical unit of automation in Calypso.

Each workflow must define:

- an identifier
- an entry condition or trigger
- explicit states
- transitions between states
- terminal outcomes
- execution-locality metadata where relevant

The workflow format must be universal across:

- local execution
- git-forge-hosted execution such as GitHub Actions

Calypso may load workflows from embedded defaults and repository-local files, but they must resolve into one shared model.

### 5.3 Step types

Each workflow state executes one of a small number of step types.

#### Agentic steps

Agentic steps invoke AI tooling, typically through vendor CLI sessions or equivalent headless interfaces.

Examples:

- implement a change
- summarize results
- review generated output
- answer a structured subtask

Agentic steps must produce structured outcomes that Calypso can map back into state transitions.

#### Deterministic steps

Deterministic steps are programmable checks or actions with no model judgment in the loop.

Examples:

- CI checks passing or failing
- presence of required artifacts
- status check polling
- branch or PR state validation
- time-based waits
- retry windows
- file or state reconciliation

#### Control steps

Calypso must also support non-work steps used to shape the state machine, such as:

- loop
- branch
- wait
- terminal

### 5.4 Execution locality

Workflow definitions must be able to express where a step runs:

- locally under the daemon
- on a git forge's action runner
- through delegated execution while preserving the same logical workflow state

Execution locality is runtime metadata, not a separate workflow language.

### 5.5 Runtime state

Calypso must persist enough state to resume execution safely, including:

- active workflow
- current state
- transition history
- pending human input or steering actions
- pending deterministic checks
- agent run metadata
- last known terminal or interruption reason

### 5.6 Steering

Some agentic jobs will get stuck, fail ambiguously, or require human correction. Calypso must model these cases explicitly.

Steering actions may include:

- provide clarification
- retry current step
- skip to an allowed recovery path
- abort the workflow
- force a transition with recorded operator intent

## 6. User Outcomes

### 6.1 Unattended workflow execution

An operator can start Calypso in headless mode and allow it to execute workflows without an attached interactive terminal.

### 6.2 Mixed agentic and deterministic automation

A single workflow can move between deterministic checks and agentic work without leaving the state-machine model.

### 6.3 Cross-environment workflow portability

The same workflow definition can be understood and rendered consistently whether a step runs on the local machine or in forge automation.

### 6.4 Workflow observability

An operator can inspect current state, recent transitions, pending checks, and the overall workflow diagram from `calypso-web`.

### 6.5 Recovery from stuck work

When an agentic step stalls or produces an unusable result, the operator can steer the run through `calypso-web` or CLI control commands rather than restarting blindly.

## 7. Functional Requirements

### 7.1 Workflow definition and loading

Calypso must:

- load workflow definitions from supported sources into one canonical model
- validate workflow structure before execution
- reject malformed graphs and invalid transitions
- expose enough metadata for CLI and web visualization

### 7.2 Workflow execution engine

Calypso must:

- execute workflow state machines deterministically
- support long-running daemon operation
- support single-pass and continuous scheduling modes
- persist progress after each meaningful transition
- resume interrupted workflows from persisted state

### 7.3 Agentic step supervision

Calypso must:

- invoke agentic work through headless AI CLI sessions or equivalent non-interactive adapters
- capture machine-readable outcomes
- enforce timeouts, retries, and escalation paths
- distinguish recoverable step failure from fatal runtime failure

### 7.4 Deterministic step execution

Calypso must:

- evaluate deterministic checks programmatically
- poll or subscribe to external status where needed
- map outcomes directly into workflow transitions
- keep deterministic logic inspectable and testable without model involvement

### 7.5 Headless CLI

The CLI must be usable without a TUI or persistent prompt loop.

It must support:

- starting the daemon
- listing and selecting workflows
- inspecting current state
- printing logs or state snapshots
- triggering allowed control actions
- debugging execution

Interactive operation is not the primary mode. The main explicit exception is debug tooling such as `--step`, which may advance phases of the state machine one step at a time for diagnosis.

### 7.6 `calypso-web`

`calypso-web` must provide a supporting browser surface for the running daemon.

It must support:

- visual workflow diagrams
- current workflow and state display
- status checks and blocking condition views
- transition and event inspection
- a chat or steering interface for stuck agentic jobs

`calypso-web` is an observer and steering layer over daemon state, not a separate orchestration engine.

### 7.7 Logging and observability

Calypso must emit structured execution data suitable for local inspection and automation, including:

- workflow start and stop events
- state transitions
- agent invocation boundaries
- deterministic check outcomes
- interruption reasons
- steering actions

### 7.8 Failure handling

Calypso must treat the following as first-class states or outcomes:

- deterministic check failure
- agentic step failure
- timeout
- interruption
- retry
- escalation for human steering
- terminal success
- terminal abort

## 8. Non-Functional Requirements

### 8.1 Headless operation

Core functionality must work in non-interactive environments, including local background execution and CI-compatible contexts.

### 8.2 Recoverability

Workflow progress must survive process restart and machine interruption without losing the logical state of the run.

### 8.3 Determinism

Workflow control flow must remain deterministic even when individual steps are agentic.

### 8.4 Inspectability

Operators must be able to explain why the daemon is in its current state by reading persisted state, logs, or `calypso-web`.

### 8.5 Portability

Workflow definitions should be portable across supported execution environments without requiring different authoring conventions for each one.

## 9. Success Criteria

The product requirements are satisfied when Calypso can demonstrate all of the following:

- run as a headless daemon without requiring a TUI
- execute workflows defined in a universal format
- mix agentic and deterministic states within one workflow
- persist and resume workflow execution across interruption
- represent local and forge-run execution within one coherent workflow model
- expose live workflow visualization and steering through `calypso-web`
- support explicit debug stepping without making interactive stepping the default operating mode

## 10. Deprecated Assumptions and Explicit Non-Goals

The following ideas from earlier PRD iterations are deprecated and are now explicit non-goals unless they are strictly required to support workflow execution:

- Calypso as a primarily interactive TUI product
- Calypso as a developer workstation UI that requires continuous terminal interaction for normal operation
- Calypso as a broad end-to-end software methodology product centered on PR choreography, document rituals, or repository bootstrapping
- Calypso as a Kubernetes platform, deployment control plane, or infrastructure management product
- Calypso as a database operations product, including digital-twin management as a core product pillar
- Calypso as a key-management, TLS, webhook, or certificate-management product
- Calypso as a studio or preview-environment product
- Calypso as a general-purpose autonomous agent framework with unconstrained agent behavior
- Calypso as a forge-specific workflow engine tied only to GitHub semantics
- Calypso as an IDE-first or editor-integration-first product

In short: the product is now defined as a headless workflow daemon with a supporting visualization and steering app, not as an interactive developer cockpit or a broad platform for every adjacent engineering concern.
