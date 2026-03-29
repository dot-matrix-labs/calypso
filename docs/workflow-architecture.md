# Workflow Architecture

## Status

This document records the intended architecture for Calypso workflow handling.
It is the target design for the ongoing refactor, not a description of the
current implementation.

## Core decision

Calypso has one canonical workflow format.

That format is:

- GitHub Actions YAML-shaped
- isomorphic across embedded and repository-local workflow files
- interchangeable regardless of source

The only meaningful difference between workflows is execution locality and
execution policy:

- some workflows or states are intended to run locally
- some workflows or states are intended to run on GitHub-hosted runners

Source is not syntax.

- an embedded workflow is not a different workflow type
- a repository-local workflow is not a different workflow type
- a "blueprint workflow" is not a separate schema from a GitHub Actions workflow

## Architectural goals

- one parser for all workflows
- one typed workflow model for all workflows
- one workflow catalog used by list, show, validate, select, audit, web, and execution
- one executor path for local and embedded workflows
- thin CLI crate with no workflow-specific business logic
- `nightshift-core` focused on repository/runtime orchestration rather than workflow syntax

## Crate layout

The target workspace layout is:

- `crates/calypso-workflows`
- `crates/calypso-workflow-exec`
- `crates/nightshift-core`
- `crates/calypso-web`
- `crates/calypso-cli`

The CLI crate should live under `crates/`, not under the repository root.

## Crate responsibilities

### `calypso-workflows`

This crate owns workflow definition handling.

Responsibilities:

- loading workflow files from different sources
- parsing GHA-shaped YAML into typed workflow documents
- normalizing parsed workflows
- semantic validation
- entry-point classification
- source precedence and effective-catalog construction
- consistency checks across workflow graphs

This crate should expose source-aware but syntax-neutral APIs such as:

- `WorkflowDocument`
- `WorkflowCatalog`
- `WorkflowSource`
- `WorkflowHandle`
- validation diagnostics

This crate must not assume that embedded workflows are special beyond being one
possible source.

### `calypso-workflow-exec`

This crate owns workflow execution.

Responsibilities:

- registry construction from parsed workflow documents
- interpreter state
- transition execution
- sub-workflow delegation
- scheduler support
- execution planning based on workflow metadata

This crate must execute the shared workflow model from `calypso-workflows`.

It must not:

- load embedded workflow files directly
- implement a separate parser
- distinguish local vs embedded workflows as different workflow types

Execution locality should be derived from explicit workflow or state metadata,
not from where a file was loaded from.

### `nightshift-core`

This crate owns repository and product orchestration.

Responsibilities:

- repository runtime state
- feature lifecycle state
- doctor
- init
- repository and PR orchestration
- provider supervision
- state-machine-driven product behaviors outside the generic workflow layer

`nightshift-core` may consume workflows and workflow execution, but it should
not own the workflow schema, parser, or embedded-only workflow registry.

### `calypso-web`

This crate should consume the shared workflow catalog and execution APIs.

It must not maintain its own workflow-loading logic or embedded/local fallback
rules.

### `calypso-cli`

This crate is a command surface only.

Responsibilities:

- argument parsing
- command dispatch
- output formatting
- exit code handling

It must not own:

- workflow discovery
- workflow parsing
- workflow validation logic
- workflow inventory precedence rules
- separate workflow execution loops

## Workflow model

All workflows should parse into one canonical model.

Required properties:

- GHA-shaped source YAML
- stable typed representation
- explicit trigger data
- explicit state or job graph
- explicit transitions
- explicit execution-locality metadata

The model should support these distinctions without changing schema:

- embedded vs repository-local source
- local execution vs GitHub execution
- entry-point workflow vs sub-workflow

## Workflow sources

The system should support multiple workflow sources through one catalog.

Expected sources:

- embedded Calypso workflows
- repository-local workflows
- future sources if needed

The catalog should resolve:

- effective workflow set
- source metadata
- precedence rules
- duplicate-name diagnostics
- lookup by effective name and source handle

## Repository-local workflow location

Repository-local workflows should live in an explicit directory such as:

- `.calypso/workflows/`

The system should not discover workflows by scanning every YAML file under
`.calypso/`.

Reasoning:

- `.calypso/` also contains non-workflow YAML files such as templates
- parse-failure filtering is not an acceptable discovery strategy
- explicit location reduces ambiguity and hidden coupling

Compatibility support for legacy locations may exist during migration, but the
target architecture is an explicit workflow directory.

## Execution model

There must be one execution path for workflows.

That means:

- local workflows and embedded workflows use the same executor
- execution behavior is determined by workflow metadata
- sub-workflow delegation works the same regardless of source
- scheduler entry-point discovery uses the same registry and catalog as the CLI

The codebase must not keep separate implementations for:

- local inline workflow execution
- embedded workflow execution

## Command parity requirement

The following surfaces must read from the same effective workflow catalog:

- `workflows list`
- `workflows show`
- `workflows validate`
- `--select-flow`
- web workflow views
- workflow audits
- schedulers and execution entry-point discovery

If these surfaces disagree, the architecture is violated.

## Naming guidance

The term "blueprint workflow" should be retired from core implementation APIs
for this layer.

Recommended naming uses generic terms such as:

- workflow
- workflow document
- workflow source
- workflow catalog
- workflow executor

Embedded Calypso-provided workflows may still be described as embedded Calypso
workflows, but not as a different syntax or workflow class.

## Migration plan

### Phase 1: workflow model and catalog

- create `crates/calypso-workflows`
- move parsing and validation there
- convert embedded-workflow access into a source provider
- add `WorkflowCatalog`

### Phase 2: consumer migration

- migrate CLI workflow list/show/validate/select to the catalog
- migrate web workflow views to the catalog
- migrate audit logic to the catalog

### Phase 3: executor extraction

- create `crates/calypso-workflow-exec`
- move interpreter and scheduler there
- make execution registry build from catalog data

### Phase 4: execution unification

- delete separate local inline workflow execution
- route all workflow execution through the shared executor

### Phase 5: crate-boundary cleanup

- move the CLI crate into `crates/calypso-cli`
- remove workflow logic from the CLI crate
- remove workflow parser and embedded-only registry ownership from `nightshift-core`

## Acceptance criteria

The target architecture is reached when all of the following are true:

- one parser exists for all workflow files
- one workflow model is used across the workspace
- one workflow catalog feeds all workflow-facing surfaces
- one executor handles both local and embedded workflows
- execution locality is metadata, not workflow type
- the CLI crate lives under `crates/`
- the CLI crate contains no standalone workflow parsing or execution logic

## Non-goals

This architecture does not imply:

- multiple workflow syntaxes
- separate local-workflow and GitHub-workflow schemas
- CLI-specific workflow behavior
- embedded workflows having privileged parsing or execution rules
