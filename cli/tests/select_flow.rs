mod helpers;

use helpers::spawned_calypso::spawned_calypso;

// ── Listing format ────────────────────────────────────────────────────────────

/// A local `hello-world.yaml` with both cron and dispatch triggers must appear
/// as two entries — one for each trigger type.
#[test]
fn local_workflow_with_cron_and_dispatch_appears_twice() {
    let hello_world = include_str!("fixtures/workflows/hello-world.yaml");
    let out = spawned_calypso()
        .args(["--select-flow"])
        .calypso_file("hello-world.yaml", hello_world)
        .stdin("1\n")
        .run();

    // Both entries for hello-world.yaml must be listed.
    let cron_line = out
        .stdout
        .lines()
        .any(|l| l.contains("cron:") && l.contains("hello-world.yaml"));
    let dispatch_line = out
        .stdout
        .lines()
        .any(|l| l.contains("workflow_dispatch") && l.contains("hello-world.yaml"));
    assert!(
        cron_line,
        "expected a cron entry for hello-world.yaml; stdout:\n{}",
        out.stdout
    );
    assert!(
        dispatch_line,
        "expected a workflow_dispatch entry for hello-world.yaml; stdout:\n{}",
        out.stdout
    );
}

/// The menu entry format must be `N) <name> (<trigger>) -- <filename>`.
#[test]
fn listing_format_matches_expected_pattern() {
    let hello_world = include_str!("fixtures/workflows/hello-world.yaml");
    let out = spawned_calypso()
        .args(["--select-flow"])
        .calypso_file("hello-world.yaml", hello_world)
        .stdin("1\n")
        .run();

    // At least one line must match the `N) … (cron: …) -- hello-world.yaml` pattern.
    let has_formatted_entry = out.stdout.lines().any(|l| {
        let trimmed = l.trim();
        // e.g. "1) say-hello (cron: */5 * * * *) -- hello-world.yaml"
        trimmed.contains(") ")
            && trimmed.contains("(")
            && trimmed.contains(")")
            && trimmed.contains(" -- hello-world.yaml")
    });
    assert!(
        has_formatted_entry,
        "expected at least one formatted entry for hello-world.yaml; stdout:\n{}",
        out.stdout
    );
}

// ── Dispatch-only workflow ────────────────────────────────────────────────────

/// A workflow with only `workflow_dispatch` must appear exactly once.
#[test]
fn dispatch_only_workflow_appears_exactly_once() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let out = spawned_calypso()
        .args(["--select-flow"])
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .stdin("1\n")
        .run();

    let count = out
        .stdout
        .lines()
        .filter(|l| l.contains("dispatch-only.yaml"))
        .count();
    assert_eq!(
        count, 1,
        "dispatch-only.yaml should appear exactly once; stdout:\n{}",
        out.stdout
    );
}

// ── Call-only workflow (no entrypoint) ────────────────────────────────────────

/// A workflow with only `workflow_call` (no user-facing entry point) must NOT
/// appear in the list at all.
#[test]
fn call_only_workflow_does_not_appear() {
    let call_only = include_str!("fixtures/workflows/call-only.yaml");
    // Also add a dispatch-only so the list is non-empty (otherwise the binary
    // exits before printing the list).
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let out = spawned_calypso()
        .args(["--select-flow"])
        .calypso_file("call-only.yaml", call_only)
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .stdin("1\n")
        .run();

    let listed = out
        .stdout
        .lines()
        .any(|l| l.contains("call-only.yaml"));
    assert!(
        !listed,
        "call-only.yaml should not appear (no user entrypoint); stdout:\n{}",
        out.stdout
    );
}

// ── Embedded blueprint workflows ──────────────────────────────────────────────

/// The embedded blueprint library must contribute at least one entry with a
/// `workflow_dispatch` or `cron` trigger.
#[test]
fn embedded_workflows_appear_in_list() {
    let out = spawned_calypso()
        .args(["--select-flow"])
        .stdin("1\n")
        .run();

    let has_embedded = out.stdout.lines().any(|l| {
        l.contains("workflow_dispatch") || l.contains("cron:")
    });
    assert!(
        has_embedded,
        "expected at least one embedded workflow entry; stdout:\n{}",
        out.stdout
    );
}

// ── "Selected:" confirmation ──────────────────────────────────────────────────

/// After the user picks a valid number, stdout must contain `"Selected:"`.
#[test]
fn valid_selection_prints_selected_line() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let out = spawned_calypso()
        .args(["--select-flow"])
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .stdin("1\n")
        .run();

    // The binary shows the selector but there is no state file, so it exits 1.
    // The "Selected:" confirmation is still printed to stdout before the error.
    assert!(
        out.stdout.contains("Selected:"),
        "expected 'Selected:' in stdout; stdout:\n{}\nstderr:\n{}",
        out.stdout,
        out.stderr
    );
}

// ── Error: no state file, never initialized ───────────────────────────────────

/// When the project directory has no state file and no `init-state.json`, the
/// error message must mention `init` but NOT `--reinit`.
#[test]
fn no_state_file_no_init_shows_init_error() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let out = spawned_calypso()
        .args(["--select-flow"])
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .stdin("1\n")
        .run();

    assert_eq!(
        out.exit_code, 1,
        "expected exit code 1; stderr:\n{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("init"),
        "expected stderr to mention 'init'; stderr:\n{}",
        out.stderr
    );
    assert!(
        !out.stderr.contains("--reinit"),
        "expected stderr NOT to mention '--reinit' for uninitialised project; stderr:\n{}",
        out.stderr
    );
}

// ── Error: initialized project but missing state file ────────────────────────

/// When the project has a completed `init-state.json` but no `repository-state.json`,
/// the error message must mention `--reinit`.
#[test]
fn initialized_project_missing_state_file_shows_reinit_error() {
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let out = spawned_calypso()
        .args(["--select-flow"])
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .with_completed_init()
        .stdin("1\n")
        .run();

    assert_eq!(
        out.exit_code, 1,
        "expected exit code 1; stderr:\n{}",
        out.stderr
    );
    assert!(
        out.stderr.contains("--reinit"),
        "expected stderr to mention '--reinit' for initialised-but-broken project; stderr:\n{}",
        out.stderr
    );
}

// ── GHA workflow selected → user-friendly note ────────────────────────────────

/// Selecting a plain GitHub Actions workflow (not a calypso state-machine) must
/// print a human-readable note (not an internal YAML error) to stderr.
/// This test requires a valid `repository-state.json` so the driver actually runs.
#[test]
fn selecting_gha_workflow_prints_friendly_note_not_internal_error() {
    // Minimal state JSON that just has a `current_step` field present but points
    // to a terminal state so the driver exits quickly without calling Claude.
    let state_json = r#"{
  "current_step": "done",
  "repo_path": "/tmp",
  "completed_steps": []
}"#;
    let dispatch_only = include_str!("fixtures/workflows/dispatch-only.yaml");
    let out = spawned_calypso()
        .args(["--select-flow"])
        .state_file_json(state_json)
        .calypso_file("dispatch-only.yaml", dispatch_only)
        .stdin("1\n")
        .run();

    // Must NOT contain the raw internal YAML error.
    assert!(
        !out.stderr.contains("missing field `initial_state`"),
        "expected no internal YAML error in stderr; stderr:\n{}",
        out.stderr
    );
    // Must NOT contain "template YAML error".
    assert!(
        !out.stderr.contains("template YAML error"),
        "expected no raw template YAML error in stderr; stderr:\n{}",
        out.stderr
    );
}

// ── Cron entry format ─────────────────────────────────────────────────────────

/// The cron pattern must be included in the label, e.g.
/// `say-hello (cron: */5 * * * *) -- hello-world.yaml`.
#[test]
fn cron_entry_includes_cron_pattern() {
    let hello_world = include_str!("fixtures/workflows/hello-world.yaml");
    let out = spawned_calypso()
        .args(["--select-flow"])
        .calypso_file("hello-world.yaml", hello_world)
        .stdin("1\n")
        .run();

    let has_cron_pattern = out
        .stdout
        .lines()
        .any(|l| l.contains("cron: */5 * * * *") && l.contains("hello-world.yaml"));
    assert!(
        has_cron_pattern,
        "expected cron pattern '*/5 * * * *' in list entry; stdout:\n{}",
        out.stdout
    );
}
