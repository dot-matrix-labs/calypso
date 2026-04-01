use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use nightshift_core::state::{
    AgentSession, AgentSessionStatus, FeatureState, FeatureType, Gate, GateGroup, GateStatus,
    PullRequestRef, RepositoryIdentity, RepositoryState, SchedulingMeta, SessionOutput,
    SessionOutputStream, WorkflowState,
};

fn unique_id() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos()
}

fn temp_state_path() -> std::path::PathBuf {
    std::env::temp_dir().join(format!("calypso-cli-status-{}.json", unique_id()))
}

/// Create an isolated temp directory that is NOT a git repository.
fn temp_non_git_dir() -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("calypso-cli-test-{}", unique_id()));
    std::fs::create_dir_all(&dir).expect("temp dir should be created");
    dir
}

/// Create a temp project directory that has a `.calypso/repository-state.json`.
fn temp_project_dir_with_state(state: &RepositoryState) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("calypso-cli-project-{}", unique_id()));
    let calypso_dir = dir.join(".calypso");
    std::fs::create_dir_all(&calypso_dir).expect("project dir should be created");
    let state_path = calypso_dir.join("repository-state.json");
    state.save_to_path(&state_path).expect("state should save");
    dir
}

fn calypso() -> Command {
    Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
}

fn sample_state() -> RepositoryState {
    RepositoryState {
        version: 1,
        schema_version: 1,
        repo_id: "acme-api".to_string(),
        identity: RepositoryIdentity::default(),
        providers: Vec::new(),
        releases: Vec::new(),
        deployments: Vec::new(),
        current_feature: FeatureState {
            feature_id: "feat-tui-surface".to_string(),
            branch: "feat/cli-tui-operator-surface".to_string(),
            worktree_path: "/worktrees/feat-cli-tui-operator-surface".to_string(),
            pull_request: PullRequestRef {
                number: 22,
                url: "https://github.com/org/repo/pull/22".to_string(),
            },
            github_snapshot: None,
            github_error: None,
            workflow_state: WorkflowState::Implementation,
            gate_groups: vec![GateGroup {
                id: "validation".to_string(),
                label: "Validation".to_string(),
                gates: vec![Gate {
                    id: "rust-quality-green".to_string(),
                    label: "Rust quality green".to_string(),
                    task: "rust-quality".to_string(),
                    status: GateStatus::Passing,
                }],
            }],
            active_sessions: vec![AgentSession {
                role: "engineer".to_string(),
                session_id: "session_01".to_string(),
                provider_session_id: Some("codex_01".to_string()),
                status: AgentSessionStatus::WaitingForHuman,
                output: vec![SessionOutput {
                    stream: SessionOutputStream::Stdout,
                    text: "Waiting on operator guidance".to_string(),
                }],
                pending_follow_ups: Vec::new(),
                terminal_outcome: None,
            }],
            feature_type: FeatureType::Feat,
            roles: Vec::new(),
            scheduling: SchedulingMeta::default(),
            artifact_refs: Vec::new(),
            transcript_refs: Vec::new(),
            clarification_history: Vec::new(),
        },
        github_auth_ref: None,
        secure_key_refs: Vec::new(),
    }
}

#[test]
fn version_flag_prints_required_build_metadata() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("--version")
        .output()
        .expect("failed to run calypso-cli --version");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("calypso-cli "));
    assert!(stdout.contains("git:"));
    assert!(stdout.contains("built:"));
    assert!(stdout.contains("tags:"));
    // version output must be a single line
    assert_eq!(
        stdout.trim().lines().count(),
        1,
        "version output must be one line"
    );
}

#[test]
fn help_flag_exposes_version_information() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("--help")
        .output()
        .expect("failed to run calypso-cli --help");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("calypso-cli"));
    assert!(stdout.contains("Git hash: "));
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("--path"));
    assert!(stdout.contains("Daemon commands:"));
    assert!(stdout.contains("run list"));
}

#[test]
fn doctor_command_prints_local_prerequisite_checks() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("doctor")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run calypso-cli doctor");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Doctor checks"));
    assert!(stdout.contains("gh-installed"));
    assert!(stdout.contains("codex-installed"));
    assert!(stdout.contains("github-remote-configured"));
    assert!(stdout.contains("required-workflows-present"));
}

#[test]
fn status_command_prints_feature_gate_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("status")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run calypso-cli status");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Feature status"));
    assert!(stdout.contains("Validation"));
    assert!(stdout.contains("Blocking gates"));
}

#[test]
fn status_command_renders_operator_surface_from_state_file() {
    let path = temp_state_path();
    sample_state()
        .save_to_path(&path)
        .expect("fixture state should save");

    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .args(["status", "--state"])
        .arg(&path)
        .arg("--headless")
        .output()
        .expect("failed to run calypso-cli status");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Calypso"));
    assert!(stdout.contains("Feature: feat-tui-surface"));
    assert!(stdout.contains("engineer (session_01) [waiting-for-human]"));
    assert!(stdout.contains("Waiting on operator guidance"));

    std::fs::remove_file(path).expect("temp state file should be removed");
}

#[cfg(coverage)]
#[test]
fn status_command_renders_text_from_state_file() {
    let path = temp_state_path();
    sample_state()
        .save_to_path(&path)
        .expect("fixture state should save");

    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .args(["status", "--state"])
        .arg(&path)
        .output()
        .expect("failed to run calypso-cli status");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(!stdout.is_empty(), "status should produce output");

    std::fs::remove_file(path).expect("temp state file should be removed");
}

#[test]
fn status_command_reports_errors_outside_git_repository() {
    let path = std::env::temp_dir().join(format!(
        "calypso-cli-status-no-git-{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos()
    ));
    std::fs::create_dir_all(&path).expect("temp dir should be created");

    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("status")
        .current_dir(&path)
        .output()
        .expect("failed to run calypso-cli status");

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid utf-8");
    assert!(
        stderr.contains("status error:") && stderr.contains("not inside a git repository"),
        "stderr should contain status error with repo root message: {stderr}"
    );

    std::fs::remove_dir_all(path).expect("temp dir should be removed");
}

// ── --path / -p flag routing ──────────────────────────────────────────────────

#[test]
fn path_flag_long_routes_doctor_to_specified_directory() {
    // A non-git dir will make github-remote-configured fail.
    // Crucially it must NOT make doctor itself fail to run (exit 0).
    let dir = temp_non_git_dir();

    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .arg("doctor")
        .output()
        .expect("failed to run calypso-cli --path <dir> doctor");

    std::fs::remove_dir_all(&dir).ok();

    assert!(
        output.status.success(),
        "doctor should exit 0 even with failing checks"
    );
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(
        stdout.contains("gh-installed"),
        "doctor output should list checks"
    );
    assert!(
        stdout.contains("github-remote-configured"),
        "routing used the supplied dir"
    );
}

#[test]
fn path_flag_short_routes_doctor_to_specified_directory() {
    let dir = temp_non_git_dir();

    let output = calypso()
        .args(["-p"])
        .arg(&dir)
        .arg("doctor")
        .output()
        .expect("failed to run calypso-cli -p <dir> doctor");

    std::fs::remove_dir_all(&dir).ok();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("gh-installed"));
}

#[test]
fn path_flag_placed_after_subcommand_is_also_accepted() {
    // extract_path_flag strips -p wherever it appears in the arg list.
    let dir = temp_non_git_dir();

    let output = calypso()
        .arg("doctor")
        .args(["-p"])
        .arg(&dir)
        .output()
        .expect("failed to run calypso-cli doctor -p <dir>");

    std::fs::remove_dir_all(&dir).ok();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("gh-installed"));
}

#[test]
fn path_flag_routes_status_to_specified_directory() {
    let dir = temp_non_git_dir();

    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .arg("status")
        .output()
        .expect("failed to run calypso-cli --path <dir> status");

    std::fs::remove_dir_all(&dir).ok();

    // Non-git dir → status exits 1 with a routing-confirming error
    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid utf-8");
    assert!(
        stderr.contains("status error"),
        "routing reached the status command handler"
    );
}

#[test]
fn state_show_falls_through_to_help_when_removed() {
    // state show was removed; it should fall through to the help catch-all.
    let output = calypso()
        .args(["state", "show"])
        .output()
        .expect("failed to run calypso-cli state show");

    // Falls through to help output — exits 0.
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Usage:"), "expected help output for removed state show command");
}

#[test]
fn template_validate_falls_through_to_help_when_removed() {
    // template validate was removed; it should fall through to the help catch-all.
    let output = calypso()
        .args(["template", "validate"])
        .output()
        .expect("failed to run calypso-cli template validate");

    // Falls through to help output — exits 0.
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Usage:"), "expected help output for removed template validate command");
}

#[test]
fn doctor_fix_unknown_id_exits_nonzero_with_message() {
    let output = calypso()
        .args(["doctor", "--fix", "nonexistent-check-id"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run calypso-cli doctor --fix");

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid utf-8");
    assert!(stderr.contains("nonexistent-check-id"));
}

#[test]
fn unknown_command_prints_help_and_exits_zero() {
    let output = calypso()
        .arg("--this-flag-does-not-exist")
        .output()
        .expect("failed to run calypso-cli with unknown flag");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("Daemon commands:"));
}

// ── Deprecated command surface removal ───────────────────────────────────────

/// `calypso init` is no longer a first-class product command; the dispatch arm
/// was removed. The CLI should respond with the help output (falls through to the
/// unknown-command catch-all).
#[test]
fn init_command_is_no_longer_dispatched_as_a_product_command() {
    let output = calypso()
        .arg("init")
        .output()
        .expect("failed to run calypso-cli init");

    // Falls through to the catch-all help output — exits 0.
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(
        stdout.contains("Usage:"),
        "expected help output for removed init command, got: {stdout}"
    );
}

/// `calypso dev-status` is no longer a first-class product command.
#[test]
fn dev_status_command_is_no_longer_dispatched_as_a_product_command() {
    let output = calypso()
        .arg("dev-status")
        .output()
        .expect("failed to run calypso-cli dev-status");

    // Falls through to the catch-all help output — exits 0.
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(
        stdout.contains("Usage:"),
        "expected help output for removed dev-status command, got: {stdout}"
    );
}

/// `calypso feature-start` is no longer a first-class product command.
#[test]
fn feature_start_command_is_no_longer_dispatched_as_a_product_command() {
    let output = calypso()
        .args(["feature-start", "123", "--worktree-base", "/tmp"])
        .output()
        .expect("failed to run calypso-cli feature-start");

    // Falls through to the catch-all help output — exits 0.
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(
        stdout.contains("Usage:"),
        "expected help output for removed feature-start command, got: {stdout}"
    );
}

/// `calypso keys` is no longer a first-class product command.
#[test]
fn keys_command_is_no_longer_dispatched_as_a_product_command() {
    let output = calypso()
        .args(["keys", "list"])
        .output()
        .expect("failed to run calypso-cli keys list");

    // Falls through to the catch-all help output — exits 0.
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(
        stdout.contains("Usage:"),
        "expected help output for removed keys command, got: {stdout}"
    );
}

/// The primary help output must not advertise deprecated commands.
#[test]
fn help_output_does_not_expose_deprecated_commands() {
    let output = calypso()
        .arg("--help")
        .output()
        .expect("failed to run calypso-cli --help");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(
        !stdout.contains("feature-start"),
        "help must not expose deprecated feature-start command"
    );
    assert!(
        !stdout.contains("dev-status"),
        "help must not expose deprecated dev-status command"
    );
    assert!(
        !stdout.contains("keys list"),
        "help must not expose deprecated keys command"
    );
    assert!(
        !stdout.contains("keys rotate"),
        "help must not expose deprecated keys command"
    );
    assert!(
        !stdout.contains("template validate"),
        "help must not expose removed template validate command"
    );
    assert!(
        !stdout.contains("  state "),
        "help must not expose removed state subcommand"
    );
    assert!(
        !stdout.contains("  agents"),
        "help must not expose removed agents subcommand"
    );
    assert!(
        !stdout.contains("--step"),
        "help must not expose removed --step flag"
    );
}

// ── Logger integration tests ─────────────────────────────────────────────────

/// The dispatch log event is emitted for each CLI invocation. This test verifies
/// that the logger emits a startup entry to stderr for a basic doctor invocation.
#[test]
fn calypso_log_info_emits_dispatch_event_to_stderr() {
    let output = calypso()
        .args(["doctor"])
        .env("CALYPSO_LOG", "info")
        .output()
        .expect("failed to run calypso-cli doctor with CALYPSO_LOG=info");

    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid utf-8");
    assert!(
        stderr.contains("dispatching calypso-cli doctor"),
        "expected dispatch log on stderr; got: {stderr}"
    );
}
