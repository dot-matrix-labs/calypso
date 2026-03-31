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
fn running_without_arguments_exits_cleanly() {
    // With no .calypso/state.json in the test working directory, the binary
    // attempts to launch the doctor TUI. In a non-terminal environment the TUI
    // setup fails gracefully and the process exits 0.
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .output()
        .expect("failed to run calypso-cli");

    assert!(output.status.success());
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
fn help_documents_step_as_debug_tooling() {
    let output = calypso()
        .arg("--help")
        .output()
        .expect("run calypso --help");
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("utf-8");
    assert!(
        stdout.contains("Debug:"),
        "help should have a Debug section"
    );
    assert!(
        stdout.contains("--step") && stdout.contains("debug tooling"),
        "step should be documented as debug tooling"
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

// ── Default headless path uses daemon mode ──────────────────────────────────

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
fn step_mode_still_available_as_debug_tooling() {
    // Verify --step does not crash in a non-terminal environment
    // (it exits cleanly because there is no state file).
    let output = calypso()
        .arg("--step")
        .output()
        .expect("run calypso --step");

    assert!(output.status.success());
}
