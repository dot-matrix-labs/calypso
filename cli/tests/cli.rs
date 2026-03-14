use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::state::{
    AgentSession, AgentSessionStatus, FeatureState, FeatureType, Gate, GateGroup, GateStatus,
    PullRequestRef, RepositoryIdentity, RepositoryState, SchedulingMeta, SessionOutput,
    SessionOutputStream, WorkflowState,
};

fn temp_state_path() -> std::path::PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("calypso-cli-status-{unique}.json"))
}

fn sample_state() -> RepositoryState {
    RepositoryState {
        version: 1,
        schema_version: 1,
        repo_id: "acme-api".to_string(),
        identity: RepositoryIdentity::default(),
        providers: Vec::new(),
        github_auth_ref: None,
        secure_key_refs: Vec::new(),
        active_features: Vec::new(),
        known_worktrees: Vec::new(),
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
    assert!(stdout.contains("Version: "));
    assert!(stdout.contains("Git hash: "));
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("feature-start <feature-id> --worktree-base <path>"));
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
    assert!(stdout.contains("feature-binding-resolved"));
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
    assert!(stdout.contains("Calypso Operator Surface"));
    assert!(stdout.contains("Feature: feat-tui-surface"));
    assert!(stdout.contains("engineer (session_01) [waiting-for-human]"));
    assert!(stdout.contains("Waiting on operator guidance"));

    std::fs::remove_file(path).expect("temp state file should be removed");
}

#[cfg(coverage)]
#[test]
fn interactive_status_command_persists_state_file_updates() {
    let path = temp_state_path();
    sample_state()
        .save_to_path(&path)
        .expect("fixture state should save");

    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .args(["status", "--state"])
        .arg(&path)
        .output()
        .expect("failed to run interactive calypso-cli status");

    assert!(output.status.success());

    let restored = RepositoryState::load_from_path(&path).expect("state should reload");
    assert_eq!(
        restored.current_feature.active_sessions[0].pending_follow_ups,
        vec!["a".to_string()]
    );

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
    assert!(stderr.contains("status error: not inside a git repository"));

    std::fs::remove_dir_all(path).expect("temp dir should be removed");
}
