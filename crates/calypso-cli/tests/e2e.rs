use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_id() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time")
        .as_nanos()
}

fn calypso() -> Command {
    Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
}

/// Create a temp directory with a `.calypso/` dir and a workflow-run.json.
fn temp_project_with_run(state: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("calypso-e2e-run-{}", unique_id()));
    let calypso_dir = dir.join(".calypso");
    std::fs::create_dir_all(&calypso_dir).expect("create .calypso dir");

    let run_json = format!(
        r#"{{
  "run_id": "test-wf-1",
  "workflow_id": "test-wf",
  "current_state": "{state}",
  "locality": "local",
  "transition_history": [],
  "pending_checks": [],
  "steering": [],
  "agent_runs": [],
  "iteration": 0,
  "created_at": "2026-03-31T00:00:00Z",
  "updated_at": "2026-03-31T00:00:00Z"
}}"#
    );
    std::fs::write(calypso_dir.join("workflow-run.json"), run_json)
        .expect("write workflow-run.json");
    dir
}

#[test]
fn running_without_state_file_reaches_daemon_path() {
    // Before fix #294, invoking `calypso --path <non-git-dir>` would print
    // doctor output to stdout and exit 0.  After the fix, the daemon always
    // starts — it exits non-zero (can't resolve git repo) but stdout is clean.
    let dir = std::env::temp_dir().join(format!("calypso-e2e-noargs-{}", unique_id()));
    std::fs::create_dir_all(&dir).expect("create temp dir");

    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .args(["--path"])
        .arg(&dir)
        .output()
        .expect("failed to run calypso-cli");

    let _ = std::fs::remove_dir_all(&dir);

    // Daemon mode exits non-zero when repo root cannot be resolved.
    // stdout must be empty (daemon mode writes structured logs to stderr only).
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.is_empty(),
        "daemon path must not write to stdout: {stdout}"
    );
}

#[test]
fn version_flag_prints_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("--version")
        .output()
        .expect("failed to run calypso-cli");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("0.1.0"));
}

#[test]
fn help_flag_prints_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("help")
        .output()
        .expect("failed to run calypso-cli");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("Daemon commands:"));
}

// ── Help text documents daemon and run commands ─────────────────────────────

#[test]
fn help_documents_daemon_start_command() {
    let output = calypso()
        .arg("--help")
        .output()
        .expect("run calypso --help");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(
        stdout.contains("daemon start"),
        "help should document daemon start"
    );
    assert!(
        stdout.contains("run inspect"),
        "help should document run inspect"
    );
    assert!(
        stdout.contains("run retry"),
        "help should document run retry"
    );
    assert!(
        stdout.contains("run abort"),
        "help should document run abort"
    );
    assert!(
        stdout.contains("run clarify"),
        "help should document run clarify"
    );
    assert!(
        stdout.contains("run force-transition"),
        "help should document run force-transition"
    );
}

#[test]
fn help_does_not_document_removed_step_flag() {
    // --step was a legacy debug tool and has been removed.
    let output = calypso()
        .arg("--help")
        .output()
        .expect("run calypso --help");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(
        !stdout.contains("--step"),
        "--step must not appear in help output after removal"
    );
    assert!(
        !stdout.contains("Debug:"),
        "Debug section must not appear in help output after --step removal"
    );
}

// ── Run list / inspect / control CLI tests ──────────────────────────────────

#[test]
fn run_list_shows_active_run() {
    let dir = temp_project_with_run("implement");
    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .args(["run", "list"])
        .output()
        .expect("run calypso run list");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(
        stdout.contains("test-wf-1"),
        "run list should show the run ID"
    );
    assert!(
        stdout.contains("implement"),
        "run list should show the current state"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_list_handles_no_run() {
    let dir = std::env::temp_dir().join(format!("calypso-e2e-norun-{}", unique_id()));
    let calypso_dir = dir.join(".calypso");
    std::fs::create_dir_all(&calypso_dir).expect("create dir");

    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .args(["run", "list"])
        .output()
        .expect("run calypso run list");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(stdout.contains("No workflow runs"), "should report no runs");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_inspect_shows_run_details() {
    let dir = temp_project_with_run("scan");
    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .args(["run", "inspect"])
        .output()
        .expect("run calypso run inspect");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(stdout.contains("Run:"), "should show run header");
    assert!(stdout.contains("test-wf-1"), "should show run ID");
    assert!(stdout.contains("scan"), "should show current state");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_retry_records_steering_in_run_state() {
    let dir = temp_project_with_run("implement");
    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .args(["run", "retry"])
        .output()
        .expect("run calypso run retry");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(stdout.contains("Retry requested"), "should confirm retry");

    // Verify the steering was persisted.
    let run_json =
        std::fs::read_to_string(dir.join(".calypso/workflow-run.json")).expect("read run state");
    assert!(
        run_json.contains("retry"),
        "run state should contain retry steering"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_abort_terminates_run() {
    let dir = temp_project_with_run("implement");
    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .args(["run", "abort"])
        .output()
        .expect("run calypso run abort");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(stdout.contains("aborted"), "should confirm abort");

    let run_json =
        std::fs::read_to_string(dir.join(".calypso/workflow-run.json")).expect("read run state");
    assert!(
        run_json.contains("\"aborted\""),
        "run state should contain terminal reason"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_clarify_records_message() {
    let dir = temp_project_with_run("stuck");
    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .args(["run", "clarify", "Use production DB"])
        .output()
        .expect("run calypso run clarify");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(
        stdout.contains("Clarification recorded"),
        "should confirm clarification"
    );

    let run_json =
        std::fs::read_to_string(dir.join(".calypso/workflow-run.json")).expect("read run state");
    assert!(
        run_json.contains("Use production DB"),
        "run state should contain the clarification message"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_force_transition_updates_state() {
    let dir = temp_project_with_run("implement");
    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .args([
            "run",
            "force-transition",
            "review",
            "--reason",
            "CI is green",
        ])
        .output()
        .expect("run calypso run force-transition");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(stdout.contains("review"), "should confirm new state");

    let run_json =
        std::fs::read_to_string(dir.join(".calypso/workflow-run.json")).expect("read run state");
    assert!(
        run_json.contains("\"review\""),
        "run state should reflect new current_state"
    );
    assert!(
        run_json.contains("CI is green"),
        "run state should record the reason"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ── Default path uses daemon mode (orchestrator) ────────────────────────────

#[test]
fn daemon_start_single_pass_exits_cleanly_without_state() {
    // In a non-git temp directory, daemon start --single-pass should fail
    // gracefully with a non-zero exit code (doctor checks fail).
    let dir = std::env::temp_dir().join(format!("calypso-e2e-daemon-{}", unique_id()));
    std::fs::create_dir_all(&dir).expect("create dir");

    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .args(["daemon", "start", "--single-pass"])
        .output()
        .expect("run calypso daemon start --single-pass");

    // Non-zero exit is expected when not in a git repo.
    assert!(
        !output.status.success(),
        "daemon start --single-pass should fail outside a git repo"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn step_flag_removed_falls_through_to_help() {
    // --step was removed; unknown flags fall through to the help catch-all.
    let output = calypso()
        .arg("--step")
        .output()
        .expect("run calypso --step");

    // Falls through to help output — exits 0.
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(
        stdout.contains("Usage:"),
        "expected help output for removed --step flag"
    );
}

// ── --path without repository-state.json reaches the scheduler ───────────────

#[test]
fn path_flag_without_state_file_does_not_print_doctor_output() {
    // Regression test for fix #294: before the fix, `calypso --path <non-git-dir>`
    // would print doctor output to stdout and exit 0.  After the fix, the daemon
    // starts unconditionally — it exits non-zero when repo root cannot be
    // resolved, but stdout must be empty (daemon logs go to stderr only).
    let dir = std::env::temp_dir().join(format!("calypso-e2e-nostate-{}", unique_id()));
    std::fs::create_dir_all(&dir).expect("create dir");

    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .output()
        .expect("run calypso --path <non-git-dir>");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // The old doctor-fallback path printed a table to stdout.
    // After the fix, stdout must be empty.
    assert!(
        stdout.is_empty(),
        "--path without state file must not print doctor output to stdout: {stdout}"
    );

    // Non-zero exit: non-git dir → repo-root resolution fails → exit 1.
    assert!(
        !output.status.success(),
        "--path on a non-git dir must exit non-zero"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Positional path argument routes to daemon (issue #298) ───────────────────

/// A bare positional path argument must reach `daemon::run_daemon_default`, not
/// the legacy `run_project_dir` path.  The observable contract is identical to
/// the `--path` flag: daemon exits non-zero on a non-git dir and writes nothing
/// to stdout (daemon logs go to stderr only).
#[test]
fn positional_path_arg_routes_to_daemon_not_legacy_path() {
    let dir = std::env::temp_dir().join(format!("calypso-e2e-pos-path-{}", unique_id()));
    std::fs::create_dir_all(&dir).expect("create dir");

    let output = calypso()
        .arg(&dir)
        .output()
        .expect("run calypso <positional-path>");

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Daemon path: stdout must be empty (daemon logs go to stderr only).
    // Before fix #298 this would have printed doctor output via run_project_dir.
    assert!(
        stdout.is_empty(),
        "positional path must not print doctor output to stdout; got: {stdout}"
    );

    // Non-zero exit: non-git dir → repo-root resolution fails → exit 1.
    assert!(
        !output.status.success(),
        "positional path on a non-git dir must exit non-zero"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

/// Passing a positional path and `--path` pointing to the same non-git directory
/// must behave identically — both reach the daemon and emit no stdout.
#[test]
fn positional_path_arg_and_path_flag_behave_identically() {
    let dir = std::env::temp_dir().join(format!("calypso-e2e-equiv-{}", unique_id()));
    std::fs::create_dir_all(&dir).expect("create dir");

    let positional_output = calypso()
        .arg(&dir)
        .output()
        .expect("run calypso <positional-path>");

    let flag_output = calypso()
        .args(["--path"])
        .arg(&dir)
        .output()
        .expect("run calypso --path <dir>");

    let _ = std::fs::remove_dir_all(&dir);

    let pos_stdout = String::from_utf8_lossy(&positional_output.stdout);
    let flag_stdout = String::from_utf8_lossy(&flag_output.stdout);

    assert!(
        pos_stdout.is_empty(),
        "positional path stdout must be empty: {pos_stdout}"
    );
    assert!(
        flag_stdout.is_empty(),
        "--path flag stdout must be empty: {flag_stdout}"
    );

    assert_eq!(
        positional_output.status.code(),
        flag_output.status.code(),
        "positional path and --path flag must produce the same exit code"
    );
}
