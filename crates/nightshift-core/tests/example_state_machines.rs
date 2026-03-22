//! Integration tests that load and validate every example state-machine YAML
//! file shipped under `examples/state-machines/`.
//!
//! Each test confirms that `load_and_validate_file` accepts the file without
//! error.  The tests also assert structural invariants that reflect the
//! looping design of each example (e.g. that a `loop` state exists).

use nightshift_core::headless_sm::{HeadlessAction, load_and_validate_file};
use std::path::Path;

/// Resolve a path relative to the repository root.
///
/// `CARGO_MANIFEST_DIR` is the `crates/nightshift-core` directory; the repo
/// root is two levels up.
fn repo_root() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .expect("repo root must exist")
}

fn example_path(name: &str) -> std::path::PathBuf {
    repo_root().join("examples/state-machines").join(name)
}

// ── task-intake-loop ─────────────────────────────────────────────────────────

#[test]
fn task_intake_loop_loads_and_validates() {
    let path = example_path("task-intake-loop.yml");
    load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("task-intake-loop.yml failed to load: {e}"));
}

#[test]
fn task_intake_loop_initial_state_is_intake() {
    let path = example_path("task-intake-loop.yml");
    let sm = load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("task-intake-loop.yml failed to load: {e}"));
    assert_eq!(
        sm.initial_state, "intake",
        "initial_state should be 'intake'"
    );
}

#[test]
fn task_intake_loop_contains_a_loop_state() {
    let path = example_path("task-intake-loop.yml");
    let sm = load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("task-intake-loop.yml failed to load: {e}"));
    let has_loop = sm
        .states
        .values()
        .any(|s| matches!(s.action, HeadlessAction::Loop { .. }));
    assert!(
        has_loop,
        "task-intake-loop.yml must contain at least one loop state"
    );
}

#[test]
fn task_intake_loop_loop_targets_initial_state() {
    let path = example_path("task-intake-loop.yml");
    let sm = load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("task-intake-loop.yml failed to load: {e}"));
    let loops_to_initial = sm.states.values().any(|s| {
        if let HeadlessAction::Loop { target } = &s.action {
            target == &sm.initial_state
        } else {
            false
        }
    });
    assert!(
        loops_to_initial,
        "task-intake-loop.yml: a loop state must target the initial_state"
    );
}

// ── review-queue-loop ────────────────────────────────────────────────────────

#[test]
fn review_queue_loop_loads_and_validates() {
    let path = example_path("review-queue-loop.yml");
    load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("review-queue-loop.yml failed to load: {e}"));
}

#[test]
fn review_queue_loop_initial_state_is_dequeue() {
    let path = example_path("review-queue-loop.yml");
    let sm = load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("review-queue-loop.yml failed to load: {e}"));
    assert_eq!(
        sm.initial_state, "dequeue",
        "initial_state should be 'dequeue'"
    );
}

#[test]
fn review_queue_loop_contains_a_loop_state() {
    let path = example_path("review-queue-loop.yml");
    let sm = load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("review-queue-loop.yml failed to load: {e}"));
    let has_loop = sm
        .states
        .values()
        .any(|s| matches!(s.action, HeadlessAction::Loop { .. }));
    assert!(
        has_loop,
        "review-queue-loop.yml must contain at least one loop state"
    );
}

#[test]
fn review_queue_loop_loop_targets_initial_state() {
    let path = example_path("review-queue-loop.yml");
    let sm = load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("review-queue-loop.yml failed to load: {e}"));
    let loops_to_initial = sm.states.values().any(|s| {
        if let HeadlessAction::Loop { target } = &s.action {
            target == &sm.initial_state
        } else {
            false
        }
    });
    assert!(
        loops_to_initial,
        "review-queue-loop.yml: a loop state must target the initial_state"
    );
}

// ── implementation-loop ──────────────────────────────────────────────────────

#[test]
fn implementation_loop_loads_and_validates() {
    let path = example_path("implementation-loop.yml");
    load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("implementation-loop.yml failed to load: {e}"));
}

#[test]
fn implementation_loop_initial_state_is_inspect() {
    let path = example_path("implementation-loop.yml");
    let sm = load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("implementation-loop.yml failed to load: {e}"));
    assert_eq!(
        sm.initial_state, "inspect",
        "initial_state should be 'inspect'"
    );
}

#[test]
fn implementation_loop_contains_a_loop_state() {
    let path = example_path("implementation-loop.yml");
    let sm = load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("implementation-loop.yml failed to load: {e}"));
    let has_loop = sm
        .states
        .values()
        .any(|s| matches!(s.action, HeadlessAction::Loop { .. }));
    assert!(
        has_loop,
        "implementation-loop.yml must contain at least one loop state"
    );
}

#[test]
fn implementation_loop_loop_targets_initial_state() {
    let path = example_path("implementation-loop.yml");
    let sm = load_and_validate_file(&path)
        .unwrap_or_else(|e| panic!("implementation-loop.yml failed to load: {e}"));
    let loops_to_initial = sm.states.values().any(|s| {
        if let HeadlessAction::Loop { target } = &s.action {
            target == &sm.initial_state
        } else {
            false
        }
    });
    assert!(
        loops_to_initial,
        "implementation-loop.yml: a loop state must target the initial_state"
    );
}

// ── all examples are discoverable from the examples directory ─────────────

#[test]
fn examples_directory_contains_expected_files() {
    let dir = repo_root().join("examples/state-machines");
    let expected = [
        "task-intake-loop.yml",
        "review-queue-loop.yml",
        "implementation-loop.yml",
    ];
    for name in &expected {
        let path = dir.join(name);
        assert!(
            path.exists(),
            "expected example file not found: examples/state-machines/{name}"
        );
    }
}
