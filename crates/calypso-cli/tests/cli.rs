use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::state::{
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
    assert!(stdout.contains("feature-start <id> --worktree-base <path>"));
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
    assert!(stderr.contains("status error: not inside a git repository"));

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
fn path_flag_routes_state_show_to_specified_directory() {
    let state = sample_state();
    let dir = temp_project_dir_with_state(&state);

    let output = calypso()
        .args(["--path"])
        .arg(&dir)
        .arg("state")
        .arg("show")
        .output()
        .expect("failed to run calypso-cli --path <dir> state show");

    std::fs::remove_dir_all(&dir).ok();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    // Output is the JSON state file — must contain the feature id we seeded
    assert!(
        stdout.contains("feat-tui-surface"),
        "state show used the supplied directory"
    );
}

// ── Routing: subcommands not yet covered ─────────────────────────────────────

#[test]
fn state_show_prints_json_for_current_directory() {
    // Seed a state file under a temp dir and run state show from there.
    let state = sample_state();
    let dir = temp_project_dir_with_state(&state);

    let output = calypso()
        .args(["state", "show"])
        .current_dir(&dir)
        .output()
        .expect("failed to run calypso-cli state show");

    std::fs::remove_dir_all(&dir).ok();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("feat-tui-surface"));
    // Must be valid JSON
    serde_json::from_str::<serde_json::Value>(&stdout)
        .expect("state show output should be valid JSON");
}

#[test]
fn state_show_fails_gracefully_when_no_state_file_exists() {
    let dir = temp_non_git_dir();

    let output = calypso()
        .args(["state", "show"])
        .current_dir(&dir)
        .output()
        .expect("failed to run calypso-cli state show");

    std::fs::remove_dir_all(&dir).ok();

    assert!(!output.status.success());
    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid utf-8");
    assert!(stderr.contains("state show error"));
}

#[test]
fn init_state_subcommand_exits_cleanly_when_no_init_state_exists() {
    let dir = temp_non_git_dir();

    let output = calypso()
        .args(["init", "--state"])
        .current_dir(&dir)
        .output()
        .expect("failed to run calypso-cli init --state");

    std::fs::remove_dir_all(&dir).ok();

    // No init state yet — should print a message and exit 0 (informational, not an error)
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("No init state found") || stdout.contains("init"));
}

#[test]
fn template_validate_succeeds_for_bundled_templates() {
    // Run from the cli crate root where the embedded templates live
    let output = calypso()
        .args(["template", "validate"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run calypso-cli template validate");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert_eq!(stdout.trim(), "OK");
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
    assert!(stdout.contains("Commands:"));
}

// ── calypso keys subcommands ──────────────────────────────────────────────

#[test]
fn keys_list_on_empty_store_reports_no_keys() {
    let dir = temp_non_git_dir();
    // Ensure .calypso/ exists but has no keys.json
    std::fs::create_dir_all(dir.join(".calypso")).expect("create .calypso dir");

    let output = calypso()
        .args(["keys", "list"])
        .current_dir(&dir)
        .output()
        .expect("failed to run calypso-cli keys list");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(
        output.status.success(),
        "keys list should exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        stdout.contains("No managed keys"),
        "expected 'No managed keys' in output: {stdout}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn help_output_documents_keys_commands() {
    let output = calypso()
        .arg("--help")
        .output()
        .expect("failed to run calypso-cli --help");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(
        stdout.contains("keys list"),
        "expected 'keys list' in help output"
    );
    assert!(
        stdout.contains("keys rotate"),
        "expected 'keys rotate' in help output"
    );
    assert!(
        stdout.contains("keys revoke"),
        "expected 'keys revoke' in help output"
    );
}

#[test]
fn doctor_output_includes_key_store_health_check() {
    let output = calypso()
        .arg("doctor")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run calypso-cli doctor");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(
        stdout.contains("key-store-health"),
        "expected key-store-health check in doctor output: {stdout}"
    );
}

#[test]
fn keys_rotate_non_existent_key_exits_nonzero() {
    let dir = temp_non_git_dir();
    std::fs::create_dir_all(dir.join(".calypso")).expect("create .calypso dir");

    let output = calypso()
        .args(["keys", "rotate", "no-such-key"])
        .current_dir(&dir)
        .output()
        .expect("failed to run calypso-cli keys rotate");

    assert!(
        !output.status.success(),
        "rotate of non-existent key should exit nonzero"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keys_revoke_non_existent_key_exits_nonzero() {
    let dir = temp_non_git_dir();
    std::fs::create_dir_all(dir.join(".calypso")).expect("create .calypso dir");

    let output = calypso()
        .args(["keys", "revoke", "no-such-key"])
        .current_dir(&dir)
        .output()
        .expect("failed to run calypso-cli keys revoke");

    assert!(
        !output.status.success(),
        "revoke of non-existent key should exit nonzero"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn keys_list_json_returns_valid_json_array() {
    let dir = temp_non_git_dir();
    std::fs::create_dir_all(dir.join(".calypso")).expect("create .calypso dir");

    let output = calypso()
        .args(["keys", "list", "--json"])
        .current_dir(&dir)
        .output()
        .expect("failed to run calypso-cli keys list --json");

    assert!(
        output.status.success(),
        "keys list --json should exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    // Should parse as a JSON array.
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(stdout.trim());
    assert!(
        parsed.is_ok(),
        "keys list --json output should be valid JSON: {stdout}"
    );
    let val = parsed.unwrap();
    assert!(
        val.is_array(),
        "keys list --json should return a JSON array"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
