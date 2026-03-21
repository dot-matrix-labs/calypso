# Calypso Product Requirements Document

## 1. Overview

Calypso CLI is a Rust developer tool for orchestrating AI agents under a strict software-development protocol with deterministic guardrails.

Calypso is not itself the coding agent. It is the process authority that manages how external agents participate in development work, what stage they are allowed to act in, and how work advances through gated approvals.

The core product value is protocol enforcement:

- multi-stage local workflow gates
- GitHub-based review and approval gates
- explicit state transitions for features, releases, and deployments
- explicit visibility into database state and recovery artifacts
- supervised agent execution with narrow roles and structured outcomes
- repository bootstrap into the Calypso methodology through generated project scaffolding
- local repository environment setup and dependency verification

Calypso also integrates with supporting systems and convenience capabilities, including:

- agent providers such as Claude, Codex CLI, Gemini, OpenCode, Perplexity, Grok, Vercel AI-compatible HTTP endpoints, and self-hosted LLM gateways
- Git repositories, branches, worktrees, and pull requests
- repository, feature, release, and deployment state
- structured human approvals, clarifications, and overrides
- optional native key-management capabilities
- Kubernetes setup, audit, and doctor workflows
- local studio environments for interactive control and observability

The initial implementation may begin with a narrow provider slice, but this specification defines the product, not the milestone plan.

## 2. Product Goals

### Primary goals

- Provide a developer-facing tool for supervising AI agents through a strict staged workflow.
- Enforce deterministic repository-level and feature-level state machines with explicit approvals and gates.
- Make Git and GitHub the primary protocol surfaces for review, approval, and progress tracking.
- Initialize repositories into the Calypso framework with required documents, prompts, and Git hooks.
- Help developers set up local GitHub repository context and verify that required local tooling is installed correctly.
- Require early pull request creation and gate tracking as part of the standard workflow.
- Support narrow, role-specific agents with explicit permissions, scopes, and machine-readable outcomes.
- Support supervised question-and-answer loops between developers and agents.
- Persist explicit state for repository, feature, thread, release, deployment, and provider sessions.
- Provide explicit visibility into database environments, versions, and backups.
- Support database digital-twin workflows from backups for testing and validation.
- Enable parallel work through Git worktrees and subthreads.
- Provide operator surfaces for monitoring agents, interrupting work, and reviewing preview environments.

### Non-goals

- Replacing GitHub, CI, deployment systems, or secret-management platforms.
- Acting as a general autonomous agent framework with unconstrained agent behavior.
- Replacing upstream model providers with Calypso-native inference.
- Building a general-purpose workflow engine for non-software domains in v1.
- Designing custom IDE/editor integrations as part of the core product specification.
- Acting primarily as a general Kubernetes platform or infrastructure control plane.
- Making key management or Kubernetes diagnostics the primary product identity.

## 3. Design Principles

1. **Deterministic orchestration over agent autonomy**
   - Calypso owns state transitions and allowed operations.
   - Agents perform scoped work within the current step and role contract.

2. **Artifacts over assertions**
   - Progress is determined by files, Git state, pull request state, checklists, validations, transcripts, and structured agent outcomes.

3. **Structured outcomes**
   - Every agent invocation must terminate in a machine-readable result.
   - Terminal statuses are `OK`, `NOK`, or `ABORTED`.

4. **Narrow role contracts**
   - Roles define allowed inputs, outputs, scopes, and expected evidence.
   - Broad general-purpose agents are not the default interaction model.

5. **Worktree-first parallelism**
   - Parallel feature work and subthreads happen in isolated worktrees.
   - The orchestrator may allow multiple concurrent agents only when their work does not contend for the same mutable workspace responsibility.

6. **GitHub as a control surface**
   - Pull requests are created early and updated continuously.
   - Pull request metadata reflects workflow progress and gate status.
   - GitHub is part of the development protocol, not just an auxiliary integration.

7. **Provider abstraction**
   - Provider-specific quirks must be mapped into one Calypso orchestration contract.

8. **Human interruptibility**
   - Clarifications, approvals, denials, and overrides are part of the system design.

9. **Product-first architecture**
   - Concepts, states, and user outcomes should be specified before command layout, package structure, or storage implementation is frozen.

10. **Container-only runtime model**
   - Calypso-managed application services run as containers.
   - Bare-metal service deployment is outside the standard Calypso methodology.

11. **Convenience capabilities remain secondary**
   - Key management and Kubernetes diagnostics exist to support the workflow.
   - They do not redefine the primary identity of the product.

## 4. Primary Users

### Developer / operator

Runs Calypso locally to drive the development protocol, supervise agents, answer clarifications, inspect gates, and advance work through approvals.

### Tech lead / architect

Defines process rules, role contracts, architectural constraints, gate definitions, and review expectations.

### Release owner

Uses release and deployment state machines to manage rollout, promotion, validation, and rollback.

### Platform engineer

Configures supporting integrations such as providers, auth, secrets, studio environments, callback endpoints, TLS, and GitHub access.

## 5. Core Concepts

### 5.1 Repository

A Git repository managed by Calypso. A repository contains Calypso configuration, orchestration state, and process templates.

Calypso may also bootstrap repository-local framework assets such as workflow documents, prompt templates, and Git hooks required by the methodology.

### 5.2 Orchestrator

Calypso has one orchestrator per managed repository context.

The orchestrator owns:

- feature-to-branch-to-worktree mapping
- gate progression and state transitions
- scheduling of agent work
- concurrency decisions across and within worktrees
- reconciliation of local state with Git and GitHub state

### 5.3 Feature

A feature is the canonical unit of orchestrated work.

In the standard Calypso model:

- one feature maps to one branch
- one feature maps to one worktree
- one feature maps to one pull request
- one feature maps to one feature orchestration record

In shorthand: `feature = pull request = branch = worktree`.

Examples include:

- a new feature
- a bug fix
- a chore or upgrade
- a release-preparation task

### 5.4 Thread

A thread is a working session for an agent or subtask. Threads may be long-lived and may map to provider-specific session identifiers when supported.

### 5.5 Agent role

A narrow domain-specific worker such as:

- product
- architect
- engineer
- test-engineer
- merge-readiness
- documentation-merge
- pr-editor
- blueprint
- github-workflows-doctor
- release
- expect-db-migration-issues
- dba
- ship
- deploy

Each role has:

- allowed providers and tools
- allowed path or artifact scope
- required inputs
- required output schema
- allowed or suggested state transitions
- evidence requirements

Representative specialized roles in the Calypso methodology include:

- `merge-readiness`: checks compatibility with `main` and records merge issues
- `documentation-merge`: performs semantic merges for text-heavy documents so machine-authored updates remain intelligible and consistent with surrounding editorial intent
- `pr-editor`: treats the GitHub pull request as the canonical feature spec and quality-gate surface
- `blueprint`: reviews agent transcripts and behavior for drift from blueprint docs, implementation docs, or command policy
- `github-workflows-doctor`: validates that required GitHub workflow automation exists and triggers correctly
- `release`: checks gate completion, merge readiness, and later release-tag readiness on `main`
- `expect-db-migration-issues`: flags likely schema or migration drift risks early
- `dba`: validates schema changes, migration planning, and sufficient migration testing including digital-twin coverage
- `ship`: pushes commits and updates the active pull request state

### 5.6 Concurrency model

The orchestrator may schedule multiple agents in parallel, subject to workspace-safety rules.

The default concurrency rules are:

- one active feature-oriented agent per worktree is always acceptable
- multiple agents may operate for the same feature only when their tasks are unrelated and do not contend for the same mutable workspace area
- work that mutates source files in the same worktree should not run in parallel unless Calypso can prove the tasks are non-overlapping
- non-mutating or coordination-oriented tasks, such as reviewing GitHub state or updating orchestration state, may run alongside code-editing work when the orchestrator determines that they are safe

### 5.7 Provider

A provider is an external AI interface used by Calypso. Providers may be:

- local CLI tools
- HTTP APIs
- self-hosted gateways

Calypso treats providers as interchangeable adapters behind a common contract.

### 5.8 GitHub identity

A GitHub identity is the authenticated operator or service context Calypso uses for repository, pull request, CI, and deployment-related GitHub operations.

Calypso requires the `gh` CLI for GitHub-backed workflows.

Users may authenticate `gh` however they prefer. Calypso is responsible for detecting whether the GitHub auth state is usable for the current repository workflow.

### 5.9 Gate

A gate is a named checkpoint that must be satisfied before a feature, release, or deployment may advance. Gates are represented in structured state and, when applicable, mirrored in the pull request checklist.

Gates should also support grouping by workflow area, such as specification, implementation, validation, merge readiness, release, or database safety.

For document-heavy workflows, some gates may require semantic document reconciliation rather than line-based Git conflict resolution. Calypso should allow document-merge review to exist as a distinct gate when text changes need intelligent editorial merging.

The standard Calypso methodology should be shippable as a machine-readable state-machine template rather than only as hard-coded program logic.

### 5.10 Release

A release is a tracked promotion unit containing the intended code state, validation status, approval status, and rollout outcome.

### 5.11 Deployment

A deployment tracks environment-specific rollout state, target version, applied migration version, validation outcome, and rollback target.

### 5.12 Database state

Database state tracks the known status of managed databases across environments such as `demo` and `prod`.

This includes:

- known database environments
- current schema or migration version
- known backup locations and metadata
- available backup versions
- recovery and restore readiness

### 5.13 Database digital twin

A database digital twin is a runnable containerized database instance created from a known backup at a known version so it can be used for testing, validation, investigation, or replay workflows.

### 5.14 Studio environment

A studio environment is a local application runtime, typically Docker Compose-backed, with a Calypso overlay for chat, observability, and operator control.

### 5.15 Operator surfaces

Calypso is primarily operated through a local CLI, typically including an interactive terminal interface such as a TUI for watching agents, interrupting runs, answering clarifications, and issuing follow-up instructions.

The TUI is the primary interactive operator surface for day-to-day supervision of agents and workflow gates.

Calypso may also expose a browser-based control surface that mirrors similar operator capabilities through a locally served web interface.

The web control surface should be implemented as a WASM application and embedded into the Calypso release binary so it can be served without requiring separate frontend asset files alongside the binary.

It is intentionally a minimal operator interface, not a general web application shell. Dynamic asset loading is out of scope.

### 5.16 Studio mode

Studio mode is an interactive preview workflow in which Calypso presents both operator controls and a live application preview tied to a dedicated Git branch and Kubernetes-backed environment.

In studio mode:

- Calypso creates or uses a dedicated branch, typically named using a convention such as `studio/session-<id>`
- agents commit their changes to that branch through normal GitHub workflows
- the preview environment tracks that branch and rebuilds the relevant API and static-files services as commits arrive
- the operator can observe the resulting UI in a browser panel such as an embedded frame or equivalent preview surface
- the goal is a near-real-time human design and feedback loop

### 5.17 Edge and proxy layer

Calypso may expose narrowly scoped HTTP endpoints only where required for external callback or certificate-validation flows.

This edge layer is responsible for:

- receiving GitHub callbacks or webhooks required by Calypso workflows
- serving ACME or equivalent certificate-validation challenges when required
- terminating TLS where configured for those endpoints
- provisioning and renewing certificates, including Let's Encrypt flows when internet-reachable callback endpoints are used

### 5.18 Secure key management

Calypso is also responsible for generating and managing relevant cryptographic keys used by the product's GitHub, deployment, proxy, and certificate workflows.

Where the device supports a Secure Enclave, TPM, hardware security module interface, platform keystore, or equivalent secure element, Calypso should use it for key generation and protected key operations.

Keys should be generated and used in the strongest secure-element-backed mode available rather than assuming plaintext exportable keys by default.

This specification defines key custody for the native Calypso CLI runtime only. Browser-based key storage or shared browser/native custody models are out of scope for the current product specification.

### 5.19 Kubernetes environment

A Kubernetes environment is the default runtime target Calypso supports for managed enterprise application deployment.

Calypso does not replace Kubernetes. It uses Kubernetes as the underlying control plane and adds application-delivery workflow, policy checks, and diagnostics on top of it.

### 5.20 Container service model

Calypso assumes a containerized application topology for managed services.

The default logical service classes are:

- a static-files service serving HTML, JavaScript bundles, CSS, and related web assets over TLS
- an API service handling lightweight application logic, especially mutations before querying PostgreSQL
- one or more worker services handling asynchronous jobs and background processing

The API and worker services may be packaged from the same binary or image, with runtime role selection determined by configuration, arguments, or entrypoint behavior.

## 6. Product Scope

Calypso includes:

- a local CLI entrypoint
- an interactive terminal operator surface
- an optional locally served web operator surface
- strict staged workflow enforcement with local and GitHub-based approvals
- repository initialization into the Calypso framework
- local repository setup assistance for GitHub-backed workflows
- doctor workflows for local dependency and environment verification
- provider configuration and health validation
- native GitHub authentication and API integration
- provider adapter abstractions for CLI and HTTP agents
- supervised agent invocation and Q&A loops
- repository, feature, thread, release, and deployment state management
- database state and backup visibility
- Git and GitHub guardrails
- secure-element-backed key generation and secure key lifecycle management
- worktree lifecycle management
- role and gate orchestration
- transcript capture and structured event logging
- studio environment lifecycle and overlay integration
- studio-mode preview workflows tied to Git branches and Kubernetes environments
- callback endpoint and TLS management where required by GitHub or certificate workflows
- optional Kubernetes environment setup, audit, and doctor capabilities
- database inspection and digital-twin workflows
- optional native key-management capabilities
- container-oriented application topology assumptions for managed services

Calypso does not require:

- replacing the user's existing CI/CD platform
- replacing GitHub review workflows
- replacing application runtime tooling

Calypso does assume containerized service deployment for the managed application model. Running managed services directly on bare metal is out of scope for the default methodology.

## 7. Conceptual Architecture

```text
calypso
 ├─ operator interface
 │   ├─ cli
 │   ├─ tui
 │   ├─ web control surface
 │   └─ studio overlay
 ├─ orchestration core
 │   ├─ state machines
 │   ├─ role contracts
 │   ├─ validation and gates
 │   ├─ transcript and event model
 │   └─ approval and clarification handling
 ├─ provider adapters
 │   ├─ local cli adapters
 │   └─ http adapters
 ├─ scm integrations
 │   ├─ git
 │   ├─ worktrees
 │   └─ github
 ├─ environment integrations
 │   ├─ kubernetes setup
 │   ├─ kubernetes audit
 │   ├─ kubernetes doctor
 │   ├─ branch-backed studio previews
 │   └─ database digital twins
 ├─ database management
 │   ├─ environment inventory
 │   ├─ backup tracking
 │   ├─ version tracking
 │   └─ restore workflows
 ├─ service topology model
 │   ├─ static-files service
 │   ├─ api service
 │   └─ worker services
 ├─ security services
 │   ├─ key generation
 │   ├─ secure key storage
 │   └─ secure-element-backed signing and crypto
 ├─ edge runtime
 │   ├─ callback endpoints
 │   ├─ tls and certificates
 │   └─ webhook and challenge handling
 └─ runtime environments
     ├─ local repository context
     └─ studio environments
```

This architecture is conceptual. Concrete module layout is intentionally deferred.

## 8. Functional Requirements

### 8.1 Provider configuration

Calypso must:

- configure named providers
- support local CLI command definitions
- support HTTP endpoints, including Vercel AI-compatible request/response patterns
- support custom headers and auth strategies
- validate provider availability and authentication health
- store provider configuration separately from secrets
- expose provider capability metadata

### 8.2 Build and version metadata

Calypso must expose build metadata through standard CLI version and help surfaces.

At minimum, `-v` and `--version` should display:

- the semantic version
- the short Git commit hash used for the build
- the build timestamp
- any Git tag information available for the build

`-h` and `--help` should also expose version information in a visible way.

The versioning scheme should use semantic versioning with build metadata that includes a 6-character Git commit hash, for example:

- `1.2.3+abc123`

### 8.3 GitHub authentication and integration

Calypso requires the `gh` CLI for core GitHub workflows.

Calypso must support GitHub capabilities including:

- authenticating operator identity
- validating repository access
- creating and updating pull requests
- reading pull request status and review state
- updating pull request descriptions and metadata
- reading CI status and logs where repository permissions allow
- managing deployment-related repository credentials or keys where policy allows

Users may authenticate to GitHub through `gh` using any supported method they prefer. Calypso should verify the resulting auth state and report actionable setup guidance through `doctor` when GitHub access is missing or unusable.

### 8.4 Secure key generation and custody

Calypso must be able to generate and manage relevant cryptographic keys for supported workflows.

This includes keys used for:

- deployment access
- callback endpoint and TLS operations
- certificate management
- GitHub-related authenticated operations where key-based credentials are appropriate

Where supported by the device, Calypso must prefer generating keys inside the available secure element, such as Secure Enclave, TPM-backed keystore, hardware security module interface, or equivalent platform-protected key store.

Product requirements for key management include:

- generation using the strongest available secure element or platform-protected key store
- non-exportable key handling where supported
- auditable key creation, rotation, and revocation
- stable references from Calypso state to keys without storing raw private key material in repository state
- fallback behavior for systems without a supported secure element or protected platform key store
- explicit policy controls over which workflows may create keys automatically
- native CLI custody as the only in-scope key-management runtime for the current product

### 8.5 Provider capabilities

Each provider must declare capabilities such as:

- session support
- streaming support
- clarification-loop support
- structured-output support
- tool-calling support
- file-context support
- cancellation support

Capability declarations are product-level requirements. The final schema is an implementation decision.

### 8.6 Initial provider support

Claude is the first intended provider. The product must still be designed so additional local CLI and HTTP providers can be added without changing orchestration semantics.

### 8.7 Q&A interaction loop

Calypso must support a supervised request-response loop where:

- the operator submits an instruction to a role or thread
- the provider returns progress, an answer, a clarification request, or a terminal outcome
- the operator may answer clarifications
- Calypso resumes the same thread when the provider supports sessions, or reconstructs the context when it does not
- all exchanges are recorded in structured transcript form

### 8.8 Repository initialization

Calypso must support repository bootstrap that can:

- validate Git repository status
- detect relevant remote configuration
- help establish local GitHub repository linkage and expected remote configuration
- initialize Calypso metadata
- initialize default process templates
- initialize Calypso framework documents
- initialize prompt templates and role prompts
- install or update repository-local Git hooks required by the methodology
- initialize provider references
- establish repository identity and defaults
- validate GitHub connectivity when configured
- validate secure key capabilities available on the current device when relevant

Repository-local Git hooks may include non-blocking workflow checks. One important example is a push-time check that evaluates compatibility with `main`; if incompatibility or drift is detected, the push is still allowed, but reconciliation with `main` should be promoted to the first task in the active task list.

Hook-driven workflow rules may also update documentation obligations. For example, updates to the PRD should trigger an implementation-plan update requirement.

### 8.9 Local doctor workflows

Calypso must provide a doctor workflow for validating the local development environment.

This should include checks such as:

- whether required local executables are installed
- whether required repository configuration is present
- whether GitHub authentication is in a usable state
- whether repository remotes and branch expectations are configured as required for the workflow
- whether provider dependencies needed by the current repository are available

Doctor output should be actionable and prioritized so the user can fix setup issues without guesswork.

### 8.10 Worktree management

Calypso must:

- create semantic branches and associated worktrees
- track worktree locations and ownership
- support parallel subthreads when configured
- clean up worktrees safely
- prevent invalid reuse or unsafe deletion of active worktrees
- preserve the invariant that each active feature has exactly one associated branch, worktree, and pull request in the standard workflow model

### 8.11 Semantic branches

Branch names must be generated from configurable conventions. Typical patterns may include:

- `feat/<ticket>-<slug>`
- `fix/<ticket>-<slug>`
- `chore/<ticket>-<slug>`
- `agent/<feature>/<role>/<thread>`

The precise default convention is not fixed by this specification.

### 8.12 Early pull request creation

On feature start, Calypso must be able to:

- create a branch and worktree
- create a pull request as early as practical, typically as a draft
- seed the pull request description with the current gate checklist
- update pull request metadata as the feature progresses

The orchestrator should treat the feature, branch, worktree, and pull request as one bound unit for scheduling and state-tracking purposes.

### 8.13 Agent role execution

Calypso must:

- run an agent role for a feature step or subtask
- enforce role-specific scope and allowed actions at the orchestration layer
- collect outputs and evidence
- require a structured terminal payload with `OK`, `NOK`, or `ABORTED`
- preserve transcript and artifact references

Role completion should map to deterministic gates wherever possible. Depending on the role, the completion evidence may be a document update, pull request metadata change, GitHub workflow result, transcript review outcome, or a validated Calypso CLI operation.

### 8.14 Repository state tracking

Each repository must maintain structured state that tracks at least:

- repository identity
- configured providers
- authenticated GitHub context references
- secure key references and capability metadata
- active features
- active threads
- known worktrees
- release state
- deployment state
- database state references
- relevant versioning metadata

Calypso orchestration state should be stored with the repository or project being managed so the workflow can be resumed from local project state.

The specific file layout is intentionally deferred.

The serialization format for orchestration state should be JSON.

YAML is the default format for human-authored methodology templates, while JSON is the default format for machine-written runtime state. Runtime state storage must remain deterministic and crash-safe.

### 8.15 Feature state tracking

Each feature must track at least:

- feature identity and title
- feature type
- branch and worktree
- pull request reference
- current feature state
- associated threads and roles
- orchestrator scheduling metadata relevant to concurrency and ownership
- required gates
- grouped gate structure
- completed gates
- pending clarifications
- artifacts and transcript references
- timestamps

### 8.16 Release state tracking

Calypso must manage release progression with tracked state for:

- release identity
- candidate code version
- validation status
- approval status
- deployment status
- rollback state

### 8.17 Deployment state tracking

Calypso must manage environment deployment state including:

- target environment
- desired code version
- deployed code version
- desired migration version
- deployed migration version
- deployment state
- last deployment result
- rollback target
- deployment credentials or key references where relevant

Calypso may create and manage deployment-related keys or credentials when explicitly configured to do so.

### 8.18 Database state management

Calypso must support a database-focused workflow surface for inspecting and managing database state.

This includes visibility into:

- which databases exist for the managed application, typically including `demo` and `prod`
- where backups are stored
- the current database or migration version of each environment
- which historical versions have corresponding backups
- the restore-readiness of available backups

Calypso should treat database state as a first-class operational concern alongside release and deployment state.

### 8.19 Database digital twin

Calypso must support a digital-twin workflow that can take a backup from a known version and create a full runnable database container from it for testing or validation.

This workflow must support:

- selecting a backup by environment, version, timestamp, or equivalent identity
- restoring that backup into a runnable containerized database instance
- exposing enough metadata for tests and validation tools to target the restored instance
- preserving the provenance of the restored twin, including source environment, version, and backup reference

The digital twin is intended for safe testing and verification use, not as a replacement for production recovery procedures.

### 8.20 Studio support

Calypso must:

- launch and stop studio environments
- connect app services and overlay services
- stream agent responses to the overlay
- allow prompts and clarifications from the overlay
- expose current state, gates, and transcripts in a human-operable view

### 8.21 Operator interfaces

Calypso must support a local operator interface for supervising agents and workflows.

This should include:

- a CLI-first experience
- an interactive terminal view for watching agents, inspecting session identifiers, interrupting runs, answering clarifications, and adding follow-up content
- grouped gate views for the current feature branch
- an optional browser-based operator view exposing similar control and observability

The browser-based view may be served by the Calypso CLI for local use.

The browser-based operator surface should:

- be implemented in WASM
- be packaged into the release binary rather than shipped as separate static asset files
- require no external frontend bundle files to be present on disk for standard operation
- avoid dynamic asset loading
- provide a minimal chat-and-control interface with predefined UI elements rather than a rich client application

### 8.22 Studio mode

Calypso must support a studio mode intended for fast human feedback on branch-scoped application changes.

Studio mode must support:

- creating or attaching to a dedicated studio branch, typically named using a policy such as `studio/session-<id>`
- associating that branch with a preview environment
- presenting both agent controls and a live application preview in the operator surface
- updating the preview environment as new commits land on the studio branch
- rebuilding at least the API service and static-files service in response to relevant branch changes
- supporting a near-real-time design and review loop between human operators and agents

The exact watch, rebuild, and preview mechanism is an implementation decision, but the product must support the workflow semantics.

### 8.23 Reverse proxy and TLS

Calypso must support only the minimal HTTP and TLS surface needed for its own external integrations.

This includes:

- receiving GitHub callbacks or webhook deliveries required by configured workflows
- serving ACME or equivalent challenge responses for certificate issuance and renewal
- managing TLS certificates
- supporting certificate issuance and renewal workflows, including Let's Encrypt where applicable

TLS, certificate storage, renewal policy, and ACME challenge handling are product requirements. Specific library and process choices are implementation decisions.

### 8.24 Kubernetes support

Kubernetes is the default first-class deployment environment Calypso supports.

Calypso must support Kubernetes-oriented workflows including:

- environment discovery and connectivity validation
- setup of Calypso-required application prerequisites
- audit of cluster and namespace readiness against Calypso policy
- doctor workflows for diagnosing deployment, ingress, certificate, secret, and rollout issues
- inspection of workload health, events, logs, service status, and ingress status
- validation of environment prerequisites such as ingress controllers, certificate management, DNS assumptions, storage classes, and required permissions
- mapping feature, release, and deployment state to Kubernetes environment state

Calypso should support application delivery on Kubernetes without attempting to replace Kubernetes scheduling, reconciliation, or cluster lifecycle management.

### 8.25 Container service topology

Calypso must support a containerized service topology for managed applications.

The default supported topology includes:

- a static-files service for browser-delivered assets over TLS
- an API service for lightweight application logic and PostgreSQL-facing mutations and queries
- worker services for background processing

Calypso should support deployments where:

- API and worker roles are separate containers derived from the same binary or image
- service role is selected by configuration, arguments, environment, or entrypoint
- health, readiness, rollout, and diagnostics are evaluated per service class

Bare-metal process deployment is not required by the default Calypso methodology.

## 9. State Machines

Calypso state machines should be definable through configuration, with YAML as the default human-authored format.

### 9.1 Feature state machine

Calypso must define a feature lifecycle with explicit, validated transitions.

Representative states include:

- `new`
- `prd-review`
- `architecture-plan`
- `scaffold-tdd`
- `architecture-review`
- `implementation`
- `qa-validation`
- `release-ready`
- `done`
- `blocked`
- `aborted`

The exact v1 state set may be narrower, but the product model must support explicit feature-stage progression rather than a single generic `in_progress` state.

### 9.2 Release state machine

Representative states include:

- `planned`
- `in-progress`
- `candidate`
- `validated`
- `approved`
- `deployed`
- `rolled-back`

### 9.3 Deployment state machine

Per environment, representative states include:

- `idle`
- `pending`
- `deploying`
- `deployed`
- `failed`
- `rolling-back`
- `rolled-back`

Every transition must be attributable to an actor, validation, or explicit override.

### 9.4 State-machine templates

Calypso should ship with a default methodology template representing the standard workflow for feature development, release, and deployment.

Repositories may adopt that template directly or customize it.

The default methodology should be separable into at least:

- state-machine rules
- agent and task catalog
- task-to-prompt mappings

The state-machine rules template should define, at minimum:

- states
- transitions
- gate groups
- gates
- evidence requirements
- approval requirements
- concurrency constraints where relevant

Each gate definition should support fields such as:

- `id`
- `group`
- `description`
- `required`
- `kind`
- `evidence`
- `pass_condition`
- `block_on_fail`
- `role_owner`

The default template model should also support sections such as:

- `feature_unit`
- `doctor_checks`
- `hook_rules`
- `workflow_requirements`
- `artifact_policies`

Additional useful per-gate fields may include:

- `status_source`
- `recheck_trigger`
- `applies_to`
- `blocking_scope`
- `auto_open_task_on_fail`
- `pr_checklist_label`
- `allow_parallel_with`
- `timeout_policy`
- `waiver_policy`

The agent and task catalog should define, at minimum:

- agent identifiers
- task identifiers
- task ownership
- task descriptions
- allowed providers or execution modes
- whether a task is agent-driven or backed by a built-in deterministic evaluator

The task-to-prompt mapping should define prompt templates by task identifier rather than embedding large prompt text directly in state-transition rules.

The default Calypso template set should define a battle-tested baseline methodology rather than an empty schema. Representative default gates include:

- `doctor-clean`
- `feature-unit-bound`
- `workflow-files-present`
- `pr-canonicalized`
- `prd-impl-plan-reconciled`
- `blueprint-policy-clean`
- `merge-drift-reviewed`
- `rust-quality-green`
- `test-matrix-green`
- `dba-review-green`
- `db-forward-compat-green`
- `release-artifact-build-green`
- `post-deploy-health-green`

Some parts of the state machine should be able to refer to built-in deterministic Rust functions through reserved keywords rather than agent prompts.

Examples include checks such as:

- whether the pull request is merged
- whether required CI jobs are failing
- whether required workflow files are present
- whether the current branch is compatible with `main`

Repositories should be able to reference these built-ins from the state-machine rules template using well-defined reserved identifiers.

The default methodology template set should be embedded into the Calypso executable at build time.

If the repository root, or the path where the user is executing Calypso, contains repository-authored methodology YAML, that local YAML should take precedence over the embedded default.

Any repository-authored methodology YAML must be parsed and validated for coherence before it is used. At minimum, coherence validation should check:

- referenced states exist
- referenced transitions are valid
- referenced gates exist
- referenced tasks exist
- referenced prompts exist where required
- built-in evaluator keywords are valid
- concurrency and approval references are internally consistent

If repository-authored methodology YAML is absent, Calypso should fall back to the embedded default template set.

Calypso may also support materializing the embedded default template set into the repository on request, but repository-local copies are optional rather than required.

## 10. Pull Request and GitHub Guardrails

### Git requirements

Calypso must be able to:

- inspect repository status
- create and manage branches
- create and manage worktrees
- collect diffs and changed files for validation
- create or coordinate commits when required by the workflow
- support repository-local Git hooks that surface workflow issues without necessarily blocking developer progress

For example, Calypso may install a non-blocking push hook that checks merge compatibility with `main` and, when needed, marks merge or rebase reconciliation as the highest-priority next task.

Git push hook behavior applies to commit pushes and branch updates; it does not apply to tag pushes.

### GitHub requirements

Calypso must be able to:

- verify that `gh` is installed and authenticated for the current repository workflow
- create pull requests
- update pull request descriptions
- read and write relevant issue and pull request metadata
- reflect gate status in pull request checklists or equivalent structured metadata
- comment with run summaries when configured
- inspect CI and status checks
- read CI logs where repository permissions allow
- manage deployment-related repository credentials or keys when configured and permitted

The GitHub pull request should be treated as the canonical surface for feature specification, quality-gate visibility, and completion tracking.

Calypso should also be able to determine whether required GitHub workflow files exist and whether they trigger correctly for events such as pull requests targeting `main`.

Required GitHub workflow automation should include at least:

- a Rust quality workflow that checks linting, formatting, and build correctness
- a test workflow that runs unit, integration, and end-to-end tests
- a coverage workflow that reports code coverage and enforces the repository coverage target
- a release workflow that builds and publishes the executable artifact

### Database requirements

Calypso must be able to:

- inspect known database environments and their current versions
- inspect known backup inventories and backup locations
- associate backups with database versions and timestamps
- create containerized digital twins from selected backups
- surface database-state information in a dedicated workflow area rather than burying it inside generic deployment output

One important quality gate before merge to `main` is validating that the incoming software version, together with its database migrations, is compatible with the last known database state.

Calypso should support or require CI workflows that restore the last known database state, apply the candidate migrations, and verify that the candidate software version operates correctly against that migrated state before merge approval is granted.

### Kubernetes environment requirements

Calypso must be able to:

- validate access to target clusters and namespaces
- validate that required static-files, API, and worker service roles are present and correctly configured
- inspect workloads, rollout state, events, logs, services, ingress resources, and certificates
- detect policy or environment drift relevant to Calypso-managed applications
- bootstrap or validate required supporting components for Calypso-managed deployments
- diagnose failed or degraded deployments through a doctor workflow
- support branch-scoped preview environments used by studio mode

Calypso is not required to provide general-purpose cluster provisioning or replace native Kubernetes operators.

### Pull request checklist behavior

A new feature pull request must include sections such as:

- feature summary
- participating roles
- gate checklist
- risks
- deployment notes
- rollback notes

Representative gate items may include:

- PRD validated
- implementation plan updated
- PRD changes reconciled into the implementation plan
- merge compatibility with `main` reviewed and outstanding issues documented
- required GitHub workflows configured and firing for pull requests to `main`
- Rust lint, format, and build workflows passing
- unit, integration, and end-to-end test workflows passing
- code coverage report published and coverage target met
- architecture constraints reviewed
- implementation complete
- tests passing
- every executable line covered by tests, unless explicitly waived by repository policy
- migration review completed by DBA-oriented checks
- candidate software version validated against the last known database state and required migrations
- release notes prepared
- demo deployment validated

The checklist content must be configurable by repository policy.

## 11. Agent Contract

Every agent run must produce a structured outcome containing at least:

- role name
- feature identifier
- thread identifier
- status: `OK | NOK | ABORTED`
- summary message
- clarification list
- artifact list
- suggested next state, if any
- transcript reference

Representative outcome:

```json
{
  "role": "architect",
  "featureId": "feat-auth-refresh",
  "threadId": "thread_01",
  "status": "OK",
  "summary": "Implementation plan updated and architecture constraints satisfied.",
  "clarifications": [],
  "artifacts": [
    "docs/implementation-plan.md"
  ],
  "suggestedNextState": "scaffold-tdd"
}
```

Providers may emit richer intermediate events, but the terminal contract must be normalized into the Calypso outcome model.

## 12. Configuration Model

Calypso requires configuration for:

- provider registry
- branch naming conventions
- role definitions
- gate definitions
- agent task definitions
- task prompt definitions
- pull request templates
- state machine definitions
- GitHub repository settings
- studio templates

YAML should be the default format for repository-authored methodology templates.

Methodology YAML should normally be separated into:

- state-machine rule files
- agent/task definition files
- prompt definition files

Representative repository layout might include:

```text
.calypso/
  repository.json
  config.json
  state-machines/
    default-feature.yml
    default-release.yml
    default-deploy.yml
  agents/
    default-agents.yml
  prompts/
    default-prompts.yml
  providers/
  features/
  roles/
  transcripts/
```

This layout is illustrative rather than normative.

## 13. Error Model

Calypso must distinguish and normalize at least:

- provider authentication failures
- subprocess spawn failures
- malformed provider output
- transport failures
- Git failures
- GitHub API failures
- invalid state transitions
- missing clarification answers
- state corruption
- studio lifecycle failures

Every error must map to:

- a machine-readable code
- a user-readable summary
- a recoverability classification where possible

## 14. Non-Functional Requirements

### Reliability

- State writes must be crash-safe.
- Core commands should be idempotent where feasible.
- Resume after interruption must be supported from persistent state and transcripts.
- Retries should be supported for transient provider and GitHub failures.
- Certificate renewal and callback-endpoint reconfiguration must be recoverable without corrupting managed environment state.
- Runtime checks and doctor workflows should distinguish failures in static-files delivery, API readiness, and worker execution.
- Studio-mode update loops should distinguish GitHub commit ingestion, image rebuild failures, deployment failures, and preview readiness failures.
- Kubernetes setup, audit, and doctor workflows must degrade safely when cluster state is partially unavailable.
- Database-state inspection and digital-twin workflows must preserve backup provenance and fail safely when backups are incomplete, unavailable, or incompatible.
- Pre-merge database compatibility checks must preserve traceability between the candidate software version, the source backup state, the migration set, and the validation result.

### Security

- Secrets must never be written into repository state files.
- Secret resolution should support environment variables, OS keychains, or external secret managers.
- Logs, transcripts, and overlay streams must redact secrets.
- Optional allowlists should be supported for executables and HTTP domains.
- GitHub credentials, deployment keys, and TLS private keys must be stored and handled as high-sensitivity secrets.
- Certificate issuance and deployment-key creation must be auditable.
- Secure-element-backed key generation and non-exportable key use should be preferred wherever the device supports them.
- When no supported secure element or protected platform key store is available, fallback key handling must be explicit and policy-controlled.

### Portability

- Calypso should support macOS and Linux in v1.
- Windows support is deferred unless runtime and toolchain compatibility is validated.

Secure-element support may vary by platform and device; Calypso should expose capability detection rather than assume uniform availability.

### Observability

- Structured logs must be available for provider activity, state transitions, Git operations, GitHub operations, and studio lifecycle events.
- Correlation identifiers should connect repository, feature, thread, and provider sessions.
- An event stream should be available to both CLI and studio surfaces.
- Kubernetes diagnostics should preserve links between Calypso state, cluster resources, rollout events, and operator-visible failures.
- Runtime diagnostics should preserve service-class distinctions between static-files, API, and worker containers.
- Studio-mode diagnostics should preserve links between branch commits, rebuilds, deployments, and preview state.
- Database diagnostics should preserve links between environments, versions, backups, and restored digital twins.

### Extensibility

- New providers must be addable through adapter registration.
- New gates and state machines must be configurable without redesigning the core product model.
- Proxy backends, certificate providers, and GitHub auth modes should be swappable behind stable interfaces.

### Concurrency and runtime model

Calypso CLI uses a synchronous, thread-per-concern concurrency model with channel-based coordination. There is no async runtime.

The rationale:

- The CLI multiplexes a bounded number of I/O sources: agent subprocess streams, a local HTTP server, outbound HTTP calls, TUI input, and file-system events. At peak this is tens of concurrent streams, not thousands.
- OS threads are adequate for this scale. An async runtime solves connection-count and I/O-multiplexing problems that do not apply to a single-operator developer tool.
- A synchronous model produces readable stack traces, straightforward control flow, and no hidden executor state. Every thread has a clear purpose.
- Avoiding an async runtime eliminates a large transitive dependency tree and keeps the binary lean.

The concurrency primitives are:

- `std::thread` for dedicated I/O concerns such as subprocess stream readers, the HTTP server listener, and file-watch dispatchers.
- `std::sync::mpsc` or `crossbeam-channel` for funneling events from worker threads into a central coordination loop.
- Standard synchronization types (`Mutex`, `Arc`) where shared mutable state is unavoidable.
- Blocking HTTP via a minimal client (candidates include `minreq` and `ureq`) for outbound provider, GitHub, and Kubernetes API calls.
- A blocking, thread-per-connection HTTP server for the locally served web control surface, webhook receiver, and ACME challenge responder.

The TUI event loop on the main thread acts as the central select point, draining channels from all worker threads on each tick.

If a future workload genuinely requires high-fanout I/O multiplexing beyond what threads and channels can cover, the preferred migration path is `smol` rather than `tokio`, to preserve the minimal-dependency constraint.

## 15. Security and Trust Model

- Calypso is trusted to orchestrate work, but not to bypass repository policy.
- Agents are treated as untrusted workers whose outputs require structured validation.
- Human operators may approve, deny, or override transitions according to repository policy.
- Repository policy may restrict allowed providers, executable paths, HTTP domains, and role scopes.
- Repository policy may also restrict GitHub operations, deployment-key creation, and certificate issuance.
- Kubernetes access should be scoped to the minimum cluster and namespace permissions required for Calypso workflows.

## 16. Acceptance Criteria

The product specification is satisfied when an implementation can demonstrate the following capabilities without contradicting the model defined above:

- configure and validate at least one provider
- verify through `doctor` that `gh` is installed and authenticated for the required GitHub workflow
- generate and manage relevant keys, preferring device secure elements or protected platform key stores where available
- start a feature with a semantic branch, worktree, and early pull request
- seed and track pull request gates
- inspect pull request and CI state through the required `gh` integration
- run at least one role agent through a supervised Q&A loop
- persist repository and feature state in structured local records
- track release and deployment state
- inspect database environments, versions, backup inventory, and backup locations
- validate before merge that the candidate software version and its migrations work against the last known database state
- launch a studio environment and relay agent interaction through an overlay channel
- provide a CLI/TUI operator interface and an optional browser operator view for supervising agents
- receive required GitHub callbacks and support certificate-validation flows with TLS where needed
- setup, audit, and doctor a Kubernetes environment used for a Calypso-managed application
- validate and diagnose a containerized application topology composed of static-files, API, and worker services
- create a runnable database digital twin from a known backup for testing or validation
- support a branch-backed studio mode with near-real-time preview updates for API and static-files changes
- support extension to additional providers without changing orchestration semantics

## 17. Open Product Decisions

The following decisions remain intentionally open at product-spec stage:

1. What exact interaction model should be used for each CLI provider: one-shot, session-oriented, or both?
2. What is the canonical source of gate completion: local state, pull request checklist state, validator outputs, or a reconciliation model?
3. How strongly are role permissions enforced in v1: advisory, validated post hoc, or actively sandboxed?
4. What is the canonical command surface for operators, and which concepts are exposed directly versus abstracted?
5. What repository file layout best balances portability, resilience, and simplicity?
6. Should pull requests be draft by default for all feature types or policy-driven?
7. How should release and deployment state integrate with existing CI/CD systems?
8. Should studios be per repository, per branch, or per feature?
9. How much studio UI is required in v1 versus overlay API only?
10. Which secret backends are required for cross-platform support in the first release?
11. Which GitHub auth model should be primary in v1: device flow, browser OAuth, GitHub App, or multiple?
12. What deployment-key lifecycle should Calypso own directly versus delegate to external infrastructure?
13. Which external callback flows are required in v1: GitHub webhooks only, GitHub App callbacks, ACME challenges, or all of them?
14. Which certificate issuance modes are required in v1: local development certs, Let's Encrypt, or pluggable issuers?
15. What database engines and backup formats must the first release support for state inspection and digital-twin restore workflows?
16. Which secure-element and platform key-store facilities must be supported in the first release: Secure Enclave, TPM-backed stores, HSM interfaces, OS key stores, or an abstraction over all available options?
17. Which keys must be strictly non-exportable versus exportable under policy?
18. What Kubernetes support is required in the first release: namespace-scoped application install only, cluster prerequisite audit, doctor workflows, or all three?
19. Which Kubernetes packaging and integration model should Calypso prefer: raw manifests, Helm, Kustomize, operator patterns, or a Calypso-defined abstraction?
20. Which native key-storage backends are mandatory in the first release on macOS and Linux?
21. What additional containerized service classes, if any, must be part of the default Calypso application model beyond static-files, API, and worker services?
22. What studio preview contract is required in the first release: full embedded browser preview, linked preview URL only, or both?
23. What branch-to-preview update mechanism should be primary in studio mode: GitHub event-driven, polling, or a hybrid model?

### Resolved decisions

1. **Concurrency model**: synchronous thread-per-concern with channels, no async runtime. See §14 Extensibility, Concurrency and runtime model.
