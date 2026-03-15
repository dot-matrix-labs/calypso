# Scrub Agent Prompt — Phase 1 Code Scrub

## Context

You are working on the `calypso` Rust CLI codebase at `/home/lucas/calypso/cli/`.
Phase 1 is complete. Your job is a full scrub: remove dead code, comment out future-feature stubs (do NOT delete them), tighten allocations, and confirm the build and tests remain green.

Tracking issue: **#72**

---

## Rules

1. **Never delete future stubs** — comment them out with `// FUTURE: #<issue>` on the line above.
2. **Never add `#[allow(...)]` suppressions** without a justification comment.
3. **cargo clippy -- -D warnings must pass** after your changes.
4. **cargo test must pass** after your changes. Run it before opening a PR.
5. Only touch code in `cli/src/` and `cli/tests/`. Do not touch `cli/Cargo.toml` unless removing a dependency confirmed unused by `cargo check` + clippy.
6. Make atomic, reviewable commits — one commit per logical change (dead code removal, stub commenting, perf, deps).

---

## Task list

### 1. Comment out `codex.rs` (future Codex provider — #48)

- In `cli/src/lib.rs`, comment out the `pub mod codex;` declaration and annotate:
  ```rust
  // FUTURE: #48 — Codex provider; re-enable when multi-vendor registry is implemented
  // pub mod codex;
  ```
- In `cli/tests/codex.rs`, wrap the entire test file in `// FUTURE: #48` block comments, or add `#![cfg(future_codex)]` at the top so it compiles out cleanly.
- Verify `cargo check` is clean after this change.

### 2. Comment out dormant `RepositoryState` fields

In `cli/src/state.rs`, find these fields on `RepositoryState` and comment them out:

```rust
// FUTURE: #48 — multi-vendor provider registry
// pub providers: Vec<String>,

// FUTURE: #42 — secure key management
// pub github_auth_ref: Option<String>,
// pub secure_key_refs: Vec<SecureKeyRef>,
```

Also find and comment out the `SecureKeyRef` struct itself if it is only referenced by those fields.

Check whether `active_features`, `known_worktrees`, `releases`, `deployments` on `RepositoryState` are actively populated anywhere in `src/` (not just declared). If they are only declared and never written to, comment them out with appropriate `// FUTURE:` markers pointing to the relevant Phase 3 issue:
- `active_features` / `known_worktrees` → `// FUTURE: #40`
- `releases` / `deployments` → `// FUTURE: #34` (already merged — if these are now populated, leave them)

### 3. Comment out unenforced `GateTemplate` optional fields

In `cli/src/template.rs`, find the fields on `GateTemplate` (or wherever the template types live) that are parsed from YAML but never read during gate evaluation:

- `recheck_trigger`
- `blocking_scope`
- `applies_to`
- `status_source`

Comment each out with:
```rust
// FUTURE: #72 — parsed but not yet enforced by the workflow engine
// pub recheck_trigger: Option<String>,
```

Verify YAML deserialization still works for existing test fixtures after commenting (use `#[serde(skip)]` or just remove the field — if the YAML template files include these keys, they'll cause a deserialization error if serde is set to deny unknown fields).

### 4. Performance: `render_gates_section` capacity hint

In `cli/src/pr_checklist.rs`, find `render_gates_section()`. Change the initial `String::new()` or `String::from(...)` allocation to use `String::with_capacity(512)` (or a reasonable estimate based on the function's typical output size).

### 5. Dead import and unused variable sweep

Run:
```
cargo clippy --manifest-path cli/Cargo.toml -- -D warnings 2>&1
```

Fix all warnings. Common patterns:
- `unused import` — remove the import
- `unused variable` — prefix with `_` or remove
- `dead_code` — remove, or comment out with `// FUTURE:` if it is a planned feature

### 6. Confirm dependency hygiene

In `cli/Cargo.toml`:
- Confirm `crossterm` is still needed (used in `tui.rs`; check cfg gates)
- Confirm `regex-lite` is still needed (used in `error.rs` for redaction)
- If `codex.rs` is commented out, check if any dep was only pulled in by that module

---

## Verification checklist (run before committing)

```bash
cd /home/lucas/calypso/cli
cargo check 2>&1
cargo clippy -- -D warnings 2>&1
cargo test 2>&1
```

All three must be clean. If any test fails, investigate and fix before committing.

---

## PR instructions

- Branch: `chore/phase1-scrub`
- Title: `chore: Phase 1 code scrub — dead code, stubs, deps, compile time`
- Closes: #72
- PR body must follow the project format: `## Summary`, `## Key decisions`, `## Test plan`
- Do NOT include "Generated with Claude Code" or co-author lines
