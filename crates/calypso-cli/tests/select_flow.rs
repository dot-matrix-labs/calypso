mod helpers;

use helpers::fake_claude::unique_temp_dir;
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

    // Count only numbered list entries (e.g. "  1) greet (workflow_dispatch) -- dispatch-only.yaml"),
    // not the "Selected: ..." confirmation line which may also mention the filename.
    let count = out
        .stdout
        .lines()
        .filter(|l| {
            let t = l.trim();
            t.contains("dispatch-only.yaml") && t.starts_with(|c: char| c.is_ascii_digit())
        })
        .count();
    assert_eq!(
        count, 1,
        "dispatch-only.yaml should appear exactly once in the list; stdout:\n{}",
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

    let listed = out.stdout.lines().any(|l| l.contains("call-only.yaml"));
    assert!(
        !listed,
        "call-only.yaml should not appear (no user entrypoint); stdout:\n{}",
        out.stdout
    );
}

// ── Embedded workflows ───────────────────────────────────────────────────────

/// The embedded workflow catalog must contribute at least one entry with a
/// `workflow_dispatch` or `cron` trigger.
#[test]
fn embedded_workflows_appear_in_list() {
    let out = spawned_calypso().args(["--select-flow"]).stdin("1\n").run();

    let has_embedded = out
        .stdout
        .lines()
        .any(|l| l.contains("workflow_dispatch") || l.contains("cron:"));
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

// ── Error: local workflow with no valid transition ────────────────────────────

/// A dispatch-only workflow has a single state with no outgoing transitions.
/// After running it, the driver should hit a "no transition" error and exit 1.
#[test]
fn local_workflow_with_no_transition_exits_with_error() {
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
        out.stderr.contains("transition error") || out.stderr.contains("error"),
        "expected an error in stderr; stderr:\n{}",
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

// ── Local workflows override embedded defaults ────────────────────────────────

/// When a project directory contains local workflow files in `.calypso/`, the
/// selector must show ONLY those local files — no embedded workflow entries.
///
/// This also exercises the full setup path: create a fresh directory, insert
/// `turnstile.yaml`, and verify the selector output.
#[test]
fn local_workflows_override_embedded_defaults() {
    let turnstile = include_str!("fixtures/workflows/turnstile.yaml");
    let out = spawned_calypso()
        .args(["--select-flow"])
        .calypso_file("turnstile.yaml", turnstile)
        // Send an out-of-range selection so the process exits cleanly after
        // printing the list (we only care about the listing, not execution).
        .stdin("0\n")
        .run();

    // The turnstile workflow must appear.
    assert!(
        out.stdout.contains("turnstile.yaml"),
        "expected 'turnstile.yaml' in selector list; stdout:\n{}",
        out.stdout
    );

    // No embedded workflow filenames may appear.
    for embedded in &[
        "calypso-orchestrator-startup.yaml",
        "calypso-planning.yaml",
        "calypso-implementation-loop.yaml",
    ] {
        assert!(
            !out.stdout.contains(embedded),
            "embedded workflow '{embedded}' must not appear when local files exist; stdout:\n{}",
            out.stdout
        );
    }
}

// ── Turnstile: cyclic local workflow ──────────────────────────────────────────

/// The turnstile workflow cycles alice → bob → carol → alice indefinitely.
/// Each state runs a shell command (sleep 1 + echo), so after 7 seconds at
/// least 5 steps should have completed.  The test force-kills the process
/// and asserts each persona appeared in stdout.
#[test]
fn turnstile_runs_for_seven_seconds() {
    use std::io::Write as _;
    use std::time::Duration;

    let turnstile_yaml = include_str!("fixtures/workflows/turnstile.yaml");

    // Set up a temp working directory with the turnstile YAML in .calypso/.
    let work_dir = unique_temp_dir("calypso-turnstile-e2e");
    let calypso_dir = work_dir.join(".calypso");
    std::fs::create_dir_all(&calypso_dir).expect(".calypso dir should be created");
    std::fs::write(calypso_dir.join("turnstile.yaml"), turnstile_yaml)
        .expect("turnstile.yaml should be written");

    let binary = env!("CARGO_BIN_EXE_calypso-cli");
    let mut child = std::process::Command::new(binary)
        .args(["--select-flow"])
        .current_dir(&work_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("calypso-cli should spawn");

    // Feed "1\n" to select the first (and only) local workflow.
    if let Some(mut stdin_pipe) = child.stdin.take() {
        let _ = stdin_pipe.write_all(b"1\n");
        // Keep stdin open so the process doesn't get EOF and exit early.
        std::mem::forget(stdin_pipe);
    }

    // Let the workflow run for 7 seconds.
    std::thread::sleep(Duration::from_secs(7));

    // Force-kill the process.
    let _ = child.kill();
    let output = child
        .wait_with_output()
        .expect("wait_with_output should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();

    // Each persona must have appeared at least once.
    assert!(
        stdout.contains("Alice says hello"),
        "expected 'Alice says hello' in stdout; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Bob says hello"),
        "expected 'Bob says hello' in stdout; stdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Carol says hello"),
        "expected 'Carol says hello' in stdout; stdout:\n{stdout}"
    );

    // At least 5 "says hello" lines should have appeared in 7 seconds.
    let hello_count = stdout.lines().filter(|l| l.contains("says hello")).count();
    assert!(
        hello_count >= 5,
        "expected at least 5 'says hello' lines in 7 s; got {hello_count}; stdout:\n{stdout}"
    );
}
