# Worktree Implementation Plan

## PR: `feat: enforce CLI hook policy gates`

- [x] Inspect the branch-local PRD, current checklist source, and implementation-plan requirements for the hook policy enforcement slice.
- [x] Extend the embedded methodology template schema to model first-class policy gates, including hook/workflow metadata and tag-push exemption for hook rules.
- [x] Add deterministic policy evaluators for implementation-plan presence, implementation-plan freshness, next-prompt presence, required workflow files, and main-compatibility evidence.
- [x] Surface policy results in the default grouped gates and expose a PR-checklist mapping from evaluated gate state.
- [x] Add or update tests covering embedded policy registration, malformed policy-gate validation, policy evidence evaluation, and checklist rendering.
- [x] Re-run `cargo test -p calypso-cli` and any other required validation once the Rust dependency set is available in this environment.

## Remaining Work

- [x] Unblock Cargo dependency resolution for `serde_json`'s locked `zmij` dependency in this sandbox, then run the full CLI test suite.
- [x] If validation is green, update the live PR body/checklist to match the implemented policy-gate slice and create the next small commit.
