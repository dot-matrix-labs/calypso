# Calypso CLI Implementation Plan

## Goal

Build a motivating prototype of `calypso-cli` in Rust that proves the core product loop:

- enforce the Calypso state machine locally
- supervise agent execution through Codex CLI
- inspect the GitHub status relevant to the current feature branch
- present agents, sessions, and grouped content gates in a terminal UI

This prototype is intentionally narrow. It does not attempt to implement the full product specification.

The prototype should assume the canonical Calypso information architecture:

- one orchestrator per repository
- one feature equals one branch, one worktree, and one pull request
- agent work is scheduled by the orchestrator against those feature units

## Prototype assumptions

- The implementation language is Rust.
- The design should be as close to pure Rust as practical.
- Dependencies should be minimized aggressively.
- A stretch goal is to keep core domain logic compatible with `no_std` in v1, even if the executable itself is not `no_std`.
- Codex CLI is the only assumed provider for the prototype.
- Codex CLI is assumed to already be installed and authenticated.
- The `gh` CLI is a hard dependency for GitHub-backed prototype workflows.
- Users may authenticate `gh` however they prefer.
- Calypso `doctor` is responsible for checking whether `gh` is installed and authenticated correctly.
- The prototype does not implement broad convenience features such as key custody, Kubernetes setup, studio preview environments, release automation, or database digital twins.

## Product slice in scope

The prototype should implement only the minimum slice necessary to validate the product thesis:

1. Repository-local orchestrator state.
2. A shipped YAML methodology template set representing the default Calypso methodology.
3. Feature-to-branch-to-worktree-to-PR mapping for the current branch.
4. Relevant GitHub status inspection for the current feature branch or pull request.
5. A narrow local setup and doctor flow for required dependencies and GitHub repository context.
6. Agent session supervision through Codex CLI.
7. A TUI showing:
   - the current feature unit
   - active agent sessions for that feature
   - session identifiers
   - streamed agent output
   - a way for the user to add follow-up content or answers
   - grouped content gates for the current feature branch

## Explicitly out of scope for the prototype

- Multi-provider support beyond Codex CLI.
- HTTPS callback serving.
- Browser operator surface.
- Studio mode and embedded preview UI.
- Kubernetes deployment, audit, or doctor workflows.
- Database-state subcommands and digital-twin workflows.
- Native secure-element key custody and broader key-management workflows.
- Release and deploy automation beyond reading status relevant to the current feature branch.

## Engineering constraints

### Rust-first architecture

- Prefer the standard library and direct Rust implementations over framework-heavy crates.
- Introduce a library only when it removes substantial risk or complexity.
- Separate the code into a small core domain layer and thin environment adapters.
- Keep core state-machine logic, gate evaluation, and transcript/event models free of OS-specific assumptions where possible.

### Minimal dependencies

Prefer a short dependency list with clear justification:

- CLI argument parsing: minimal crate or hand-rolled parser
- terminal I/O and event handling: `crossterm` by default
- TUI rendering and layout: hand-rolled on top of `crossterm` unless complexity proves that a higher-level framework is justified
- structured local serialization: use JSON for machine-written orchestration state and keep YAML for human-authored methodology templates
- Process execution: std facilities unless a crate meaningfully improves streaming control

Everything else should default to hand-written Rust unless there is a strong reason not to.

### `no_std` stretch goal

The executable will likely require `std`, but the following areas should be designed so they could move toward `no_std` compatibility later:

- state-machine definitions
- gate and status models
- agent outcome models
- repository and feature domain types
- deterministic transition logic

The prototype should not distort delivery speed just to force `no_std`, but it should avoid needless coupling between core logic and the host environment.

## Prototype behavior

### Provider model

- Implement only a Codex CLI adapter.
- Assume Codex CLI is present on `PATH`.
- Assume Codex CLI auth is already valid.
- Focus on launching a session, capturing streamed output, tracking session identity, and sending follow-up input.

### GitHub access model

- Use `gh` as the required GitHub integration surface for the prototype.
- Do not implement custom GitHub auth flows in the motivating prototype.
- Help the user validate or establish the expected local repository remote configuration for GitHub-backed workflows.
- GitHub status inspection only needs to cover what the feature state machine cares about for the current branch or PR.

### Local doctor scope

The motivating prototype should include a narrow `doctor` path that checks:

- required executables for the prototype
- repository context and Git remote assumptions
- `gh` availability and authentication state
- Codex CLI availability

The doctor output should be short, actionable, and suitable for first-run setup.

### State-machine scope

Implement only the feature-branch workflow state needed for supervised development, for example:

- `new`
- `implementation`
- `waiting_for_human`
- `ready_for_review`
- `blocked`

The prototype does not need the full release or deployment model yet.

The prototype should model one active feature unit as:

- current feature
- current branch
- current worktree
- current pull request

Those should be treated as one bound orchestration object rather than separate concerns.

### Gate model

The prototype should execute a shipped default YAML methodology template set.

That template set should be separated into:

- state-machine rules
- agent/task definitions
- prompt definitions

The state-machine rules should include grouped gates for the current feature branch, such as:

- specification gates
- implementation gates
- validation gates
- merge-readiness gates

Each gate must have a deterministic status source where possible, even if some statuses are initially manual.

The motivating prototype should start with a small but concrete default gate set rather than only abstract gate groups.

Template resolution should follow this rule:

- if methodology YAML exists in the repository root or current execution path, use it
- otherwise, use the embedded default template set

Any repository-authored methodology YAML must be parsed and validated for coherence before it is accepted.

### Concurrency model

The prototype should preserve the standard Calypso scheduling rules:

- one orchestrator per repository
- one feature-oriented agent session per worktree is always safe
- additional sessions for the same feature are allowed only for tasks that do not contend for the same mutable workspace area

The motivating prototype only needs to execute one provider-backed agent session at a time, but its domain model should leave room for safe parallel sessions later.

### TUI scope

The TUI is the main interface for the prototype. It should show:

- current repository and feature branch context
- active agent sessions
- provider session IDs
- live streamed output per agent
- an input path for the user to answer or add content
- grouped gates and their current status
- the currently blocking gate or issue, if any

The TUI does not need advanced layout or styling. Clarity and reliability matter more than visual ambition.

The TUI layer should remain a thin presentation boundary so the prototype can move to a richer framework later if `crossterm` proves insufficient.

## Proposed phases

## Phase 0: Reset and scaffold

- [x] Create a Rust crate layout for `calypso-cli`.
- [x] Choose a minimal crate structure: executable plus small core library.
- [x] Set up formatting, linting, and test commands.
- [x] Define build-time version metadata injection for semantic version, 6-character Git hash, build time, and Git tag information.
- [x] Define the baseline GitHub workflow set for the repository:
  - Rust lint, format, and build
  - unit, integration, and end-to-end tests
  - code coverage reporting with a 99% or greater line-coverage target
  - release and executable publishing
- [ ] Add a short architecture note explaining why each dependency exists.

Completed scaffold notes:

- Initial Cargo project exists with `build.rs`, library crate, and binary entrypoint.
- Version and help output are implemented via test-first development.
- Current verified behavior includes semantic version plus 6-character Git hash, build time, and Git tag output.

## Phase 1: Core domain model

- [ ] Define domain types for repository state, feature state, agent session, gate group, gate status, and agent outcome.
- [ ] Define the canonical feature unit mapping: feature = branch = worktree = pull request.
- [ ] Define a minimal YAML-backed feature workflow state machine model.
- [ ] Define deterministic transition checks.
- [ ] Define basic scheduling metadata for safe agent concurrency.
- [ ] Add tests for state transitions and gate grouping logic.

Immediate next step:

- Write failing tests for the first JSON-backed orchestration state types and persistence boundaries before implementing them.

## Phase 1.5: YAML methodology template

- [ ] Define the YAML schema for state-machine rules, transitions, gate groups, gates, and approval rules.
- [ ] Define the YAML schema for agent/task definitions.
- [ ] Define the YAML schema for prompt definitions keyed by task name.
- [ ] Define the default shipped feature template set for the motivating prototype.
- [ ] Embed the default template set into the executable at build time.
- [ ] Include hook rules, doctor checks, and workflow requirements in the state-machine rules model.
- [ ] Define reserved built-in evaluator keywords for deterministic Rust-backed checks.
- [ ] Validate template loading and schema errors clearly.

## Phase 2: Local persistence

- [ ] Implement repository-local state storage.
- [ ] Store orchestration state with the managed repository/project.
- [ ] Persist feature branch state, gate state, and tracked agent sessions.
- [ ] Store orchestration state as JSON and keep the serialization boundary localized.
- [ ] Keep file formats simple and stable.
- [ ] Add tests for load, save, resume, and corruption handling.

## Phase 3: Git and branch context

- [ ] Detect current repository and current branch.
- [ ] Map the current branch to Calypso feature context and its bound worktree and pull request identity.
- [ ] Read enough Git information to support the prototype state machine.
- [ ] Validate the expected local GitHub remote context for the repository.
- [ ] Add tests using fixture repositories where practical.

## Phase 3.5: Gate evaluation runtime

- [ ] Implement a deterministic runtime that evaluates gate state from the YAML state-machine rules.
- [ ] Implement template resolution: repository-local YAML first, embedded default second.
- [ ] Reject repository-authored YAML that fails coherence validation.
- [ ] Resolve task execution from the agent/task catalog and prompt definitions.
- [ ] Map built-in evaluator keywords to deterministic Rust functions.
- [ ] Map gate status sources to Git, `gh`, local documents, doctor checks, built-in evaluators, and agent outcomes.
- [ ] Compute blocking gates and available transitions from evaluated evidence.
- [ ] Add tests for template-driven gate evaluation.

## Phase 4: GitHub status inspection

- [ ] Implement the narrowest GitHub integration needed to inspect branch or PR status.
- [ ] Use `gh` for the required GitHub status and pull-request inspection paths.
- [ ] Surface only the statuses relevant to gates and merge readiness.

## Phase 4.5: Local doctor

- [ ] Implement a narrow `doctor` command for prototype prerequisites.
- [ ] Check repository context, `gh` install/auth state, and Codex CLI availability.
- [ ] Check that required GitHub workflow files are present in the repository.
- [ ] Emit actionable fixes for missing or invalid local setup.

## Phase 4.75: Hook and checklist integration

- [ ] Implement evaluation for hook-driven rules such as merge drift and PRD-to-implementation-plan reconciliation.
- [ ] Map grouped gate state to PR checklist semantics for the motivating prototype.
- [ ] Ensure tag-push exemption is represented in the hook model.

## Phase 5: Codex CLI adapter

- [ ] Launch Codex CLI as a subprocess.
- [ ] Capture streamed output.
- [ ] Track session identifiers when available.
- [ ] Support follow-up user input into the active session.
- [ ] Normalize terminal outcomes into Calypso agent status.

## Phase 6: TUI

- [ ] Build a terminal interface for agent supervision.
- [ ] Show the current feature unit and active sessions, including session IDs and streamed output.
- [ ] Show grouped gates for the current feature branch.
- [ ] Allow the user to add content or answer an active session.
- [ ] Show the current blocking gate or merge issue clearly.

## Phase 7: End-to-end prototype loop

- [ ] Start in a repository on a feature branch.
- [ ] Load or initialize Calypso feature state.
- [ ] Inspect relevant GitHub status.
- [ ] Run one Codex-backed agent session.
- [ ] Let the user respond through the TUI.
- [ ] Update gate state and feature state deterministically.

## Success criteria for the motivating prototype

- [ ] Running `calypso-cli` in a feature branch loads or creates workflow state for the bound feature/branch/worktree/pull-request unit.
- [ ] The tool can display grouped gates for the branch and mark their status deterministically where possible.
- [ ] The tool can launch Codex CLI, stream output, and display the provider session ID when available.
- [ ] The user can enter follow-up content from the TUI.
- [ ] The tool can inspect the GitHub status relevant to the current branch or PR.
- [ ] The `doctor` command can report whether repository context, `gh`, Codex CLI, and required workflow files are ready for the prototype workflow.
- [ ] The tool can clearly show whether the feature branch is blocked, waiting for human input, or ready for review.
- [ ] `-v` or `--version` prints semantic version, 6-character Git hash, build time, and available Git tag information.
- [ ] `-h` or `--help` exposes version information visibly.

## Risks

- Codex CLI session behavior may not expose stable identifiers in a way that is easy to normalize.
- `gh` integration may expose output-shape and environment differences that require defensive handling.
- TUI complexity can expand quickly if the first layout is too ambitious.

## Recommended build order

1. Phase 0
2. Phase 1
3. Phase 1.5
4. Phase 2
5. Phase 3
6. Phase 3.5
7. Phase 4
8. Phase 4.5
9. Phase 4.75
10. Phase 5
11. Phase 6
12. Phase 7

## Prototype note

This plan is intentionally narrower than the product specification. It exists to validate the core Calypso thesis before implementing broader capabilities.
