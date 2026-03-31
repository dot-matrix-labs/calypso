mod helpers;

use helpers::spawned_calypso::spawned_calypso;

// ── Single eligible local workflow ────────────────────────────────────────────

/// With daemon-first design, a single eligible local workflow is no longer
/// auto-started when calypso is invoked with no arguments. The daemon-default
/// path checks for a state file instead. Use `--select-flow` for the legacy
/// interactive workflow selection behavior.
#[test]
fn single_local_workflow_not_auto_started_without_select_flow() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let out = spawned_calypso()
        // No --select-flow flag: daemon-first, not auto-detect.
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .run();

    // Daemon-first: no auto-start of local workflows without --select-flow.
    assert!(
        !out.stdout.contains("Starting workflow:"),
        "expected no auto-start without --select-flow in daemon-first mode; \
         stdout:\n{}\nstderr:\n{}",
        out.stdout,
        out.stderr
    );
}

/// With a single eligible local workflow present, no interactive selection
/// prompt should appear (i.e., "Available workflows:" must not be printed).
#[test]
fn single_local_workflow_does_not_show_selection_menu() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let out = spawned_calypso()
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .run();

    assert!(
        !out.stdout.contains("Available workflows:"),
        "expected no selection menu for a single local workflow; stdout:\n{}",
        out.stdout
    );
}

// ── Multiple eligible local workflows ────────────────────────────────────────

/// With daemon-first design, multiple eligible local workflows no longer
/// trigger interactive selection when calypso is invoked with no arguments.
/// The daemon-default path checks for a state file instead. Use `--select-flow`
/// for the legacy interactive workflow selection behavior.
#[test]
fn multiple_local_workflows_no_menu_without_select_flow() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let hello_world = include_str!("fixtures/workflows/hello-world.yaml");
    let out = spawned_calypso()
        // No --select-flow flag: daemon-first, not auto-detect.
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .calypso_file("hello-world.yaml", hello_world)
        .run();

    // Daemon-first: no interactive selection without --select-flow.
    assert!(
        !out.stdout.contains("Available workflows:"),
        "expected no selection menu in daemon-first mode; stdout:\n{}",
        out.stdout
    );
}

/// With multiple local workflows, both workflow names must appear in the menu.
#[test]
fn multiple_local_workflows_menu_lists_both_files() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let hello_world = include_str!("fixtures/workflows/hello-world.yaml");
    let out = spawned_calypso()
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .calypso_file("hello-world.yaml", hello_world)
        .stdin("0\n")
        .run();

    assert!(
        out.stdout.contains("dispatch-only.yaml"),
        "expected 'dispatch-only.yaml' in menu; stdout:\n{}",
        out.stdout
    );
    assert!(
        out.stdout.contains("hello-world.yaml"),
        "expected 'hello-world.yaml' in menu; stdout:\n{}",
        out.stdout
    );
}

// ── No local workflows ────────────────────────────────────────────────────────

/// When no local `.yaml` workflows exist in `.calypso/`, calypso (no args)
/// must fall back to the existing embedded catalog behaviour.  In a bare temp
/// directory that has no `repository-state.json` the embedded fallback is
/// the doctor output.
#[test]
fn no_local_workflows_falls_back_to_embedded_behavior() {
    // No calypso_file() calls — bare temp directory, no `.calypso/` yaml files.
    let out = spawned_calypso().run();

    // The doctor output (embedded fallback) should appear.  It always contains
    // a summary line or at minimum exits zero.  We just assert that the
    // binary did NOT auto-start a workflow (no "Starting workflow:" line).
    assert!(
        !out.stdout.contains("Starting workflow:"),
        "expected no auto-started workflow when no local files exist; stdout:\n{}",
        out.stdout
    );
}

/// A call-only workflow (`workflow_call` trigger only) must NOT be
/// auto-selected even when it is the only file in `.calypso/`, because it has
/// no user-facing entry point.
#[test]
fn call_only_workflow_is_not_auto_selected() {
    let call_only = include_str!("fixtures/workflows/call-only.yaml");
    let out = spawned_calypso()
        .calypso_file("call-only.yaml", call_only)
        .run();

    // Should not auto-start (no trigger/dispatch entry point).
    assert!(
        !out.stdout.contains("Starting workflow:"),
        "call-only workflow must not be auto-selected; stdout:\n{}",
        out.stdout
    );
}

// ── RepositoryState file present ─────────────────────────────────────────────

/// When a `repository-state.json` exists, calypso (no args) must route to
/// the state machine path, regardless of any local workflow files.
#[test]
fn state_file_present_skips_local_workflow_auto_detect() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    // A minimal state file that resolves to a terminal state so the driver
    // exits quickly without needing Claude.
    let state_json = r#"{
  "current_step": "done",
  "repo_path": "/tmp",
  "completed_steps": []
}"#;
    let out = spawned_calypso()
        .state_file_json(state_json)
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .run();

    // State machine path is taken: no selection menu, no "Starting workflow:"
    // from the WorkflowInterpreter path.
    assert!(
        !out.stdout.contains("Available workflows:"),
        "state-machine path must not show a selection menu; stdout:\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("Starting workflow:"),
        "state-machine path must not auto-start a local workflow; stdout:\n{}",
        out.stdout
    );
}

// ── --path dispatch branch ──────────────────────────────────────────────────

/// With daemon-first design, `calypso --path <dir>` with no subcommand also
/// uses the daemon-default path and does not auto-start local workflows.
#[test]
fn path_flag_single_local_workflow_not_auto_started_without_select_flow() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let out = spawned_calypso()
        .args(["--path", "{WORK_DIR}"])
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .run();

    // Daemon-first: no auto-start without --select-flow.
    assert!(
        !out.stdout.contains("Starting workflow:"),
        "expected no auto-start in daemon-first mode for --path; stdout:\n{}\nstderr:\n{}",
        out.stdout,
        out.stderr
    );
}

/// With daemon-first design, `calypso --path <dir>` with multiple local
/// workflows also does not show interactive selection.
#[test]
fn path_flag_multiple_local_workflows_no_menu_without_select_flow() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let hello_world = include_str!("fixtures/workflows/hello-world.yaml");
    let out = spawned_calypso()
        .args(["--path", "{WORK_DIR}"])
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .calypso_file("hello-world.yaml", hello_world)
        .run();

    // Daemon-first: no interactive selection without --select-flow.
    assert!(
        !out.stdout.contains("Available workflows:"),
        "expected no selection menu in daemon-first mode for --path; stdout:\n{}",
        out.stdout
    );
}

/// `calypso --path <dir>` must fall back to doctor output when no eligible
/// local workflows are present.
#[test]
fn path_flag_no_local_workflows_falls_back_to_embedded_behavior() {
    let out = spawned_calypso().args(["--path", "{WORK_DIR}"]).run();

    assert!(
        !out.stdout.contains("Starting workflow:"),
        "expected no auto-started workflow for --path with no local files; stdout:\n{}",
        out.stdout
    );
}

/// `calypso --path <dir>` must still drive the state machine when a repository
/// state file is present.
#[test]
fn path_flag_state_file_present_skips_local_workflow_auto_detect() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let state_json = r#"{
  "current_step": "done",
  "repo_path": "/tmp",
  "completed_steps": []
}"#;
    let out = spawned_calypso()
        .args(["--path", "{WORK_DIR}"])
        .state_file_json(state_json)
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .run();

    assert!(
        !out.stdout.contains("Available workflows:"),
        "state-machine path must not show a selection menu for --path; stdout:\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("Starting workflow:"),
        "state-machine path must not auto-start a local workflow for --path; stdout:\n{}",
        out.stdout
    );
}

/// `calypso --path <dir> --select-flow` must continue to show the selector
/// instead of auto-starting local workflows.
#[test]
fn path_flag_with_select_flow_keeps_selector() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let out = spawned_calypso()
        .args(["--path", "{WORK_DIR}", "--select-flow"])
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .stdin("0\n")
        .run();

    assert!(
        out.stdout.contains("Available workflows:"),
        "expected selector for --path with --select-flow; stdout:\n{}",
        out.stdout
    );
    assert!(
        !out.stdout.contains("Starting workflow:"),
        "expected no auto-start for --path with --select-flow; stdout:\n{}",
        out.stdout
    );
}
