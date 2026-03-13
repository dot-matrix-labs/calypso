use std::path::Path;

use calypso_cli::app::{
    gate_status_label, missing_pull_request_evidence, missing_pull_request_ref,
    parse_pull_request_ref, render_feature_status, resolve_current_branch,
    resolve_current_pull_request_with_program, resolve_repo_root, run_command, run_doctor,
};
use calypso_cli::state::{
    FeatureState, Gate, GateGroup, GateStatus, PullRequestRef, WorkflowState,
};

fn feature_with_gate_statuses(statuses: &[GateStatus]) -> FeatureState {
    FeatureState {
        feature_id: "feature".to_string(),
        branch: "feature".to_string(),
        worktree_path: "/tmp/feature".to_string(),
        pull_request: PullRequestRef {
            number: 7,
            url: "https://github.com/dot-matrix-labs/calypso/pull/7".to_string(),
        },
        workflow_state: WorkflowState::Implementation,
        gate_groups: vec![GateGroup {
            id: "validation".to_string(),
            label: "Validation".to_string(),
            gates: statuses
                .iter()
                .enumerate()
                .map(|(index, status)| Gate {
                    id: format!("gate-{index}"),
                    label: format!("Gate {index}"),
                    task: format!("task-{index}"),
                    status: status.clone(),
                })
                .collect(),
        }],
        active_sessions: Vec::new(),
    }
}

fn make_temp_dir(name: &str) -> std::path::PathBuf {
    let unique = format!(
        "{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("system clock should be after epoch")
            .as_nanos()
    );
    let path = std::env::temp_dir().join(unique);
    std::fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn init_git_repo(branch: &str) -> std::path::PathBuf {
    let repo_root = make_temp_dir("calypso-cli-app-tests");
    std::process::Command::new("git")
        .args(["init", "-b", branch])
        .current_dir(&repo_root)
        .output()
        .expect("git init should run successfully");

    repo_root
}

#[test]
fn render_feature_status_reports_missing_pr_and_no_blocking_gates() {
    let feature = feature_with_gate_statuses(&[GateStatus::Passing, GateStatus::Passing]);
    let rendered = render_feature_status(Path::new("/tmp/feature"), "feature", None, &feature);

    assert!(rendered.contains("Pull request: missing"));
    assert!(rendered.contains("Blocking gates: none"));
}

#[test]
fn run_doctor_falls_back_to_current_directory_outside_git_repo() {
    let temp_dir = make_temp_dir("calypso-cli-run-doctor-no-git");

    let rendered = run_doctor(&temp_dir);

    assert!(rendered.contains("Doctor checks"));

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn resolve_repo_root_and_branch_report_git_context() {
    let repo_root = init_git_repo("feature/test-app-runtime");
    let nested_dir = repo_root.join("nested");
    std::fs::create_dir_all(&nested_dir).expect("nested dir should be created");
    let canonical_repo_root = std::fs::canonicalize(&repo_root).expect("repo root should resolve");

    assert_eq!(resolve_repo_root(&nested_dir), Some(canonical_repo_root));
    assert_eq!(
        resolve_current_branch(&repo_root),
        Some("feature/test-app-runtime".to_string())
    );

    std::fs::remove_dir_all(repo_root).expect("temp repo should be removed");
}

#[test]
fn gate_status_label_includes_manual_state() {
    assert_eq!(gate_status_label(&GateStatus::Manual), "manual");
}

#[test]
fn missing_pull_request_defaults_are_failing() {
    let pull_request = missing_pull_request_ref();
    assert_eq!(pull_request.number, 0);
    assert!(pull_request.url.is_empty());

    let evidence = missing_pull_request_evidence();
    assert_eq!(evidence.result_for("builtin.github.pr_exists"), Some(false));
    assert_eq!(evidence.result_for("builtin.github.pr_merged"), Some(false));
    assert_eq!(
        evidence.result_for("builtin.github.pr_checks_green"),
        Some(false)
    );
}

#[test]
fn run_command_returns_none_for_non_zero_exit() {
    assert_eq!(
        run_command(Path::new("."), "/bin/sh", &["-c", "exit 1"]),
        None
    );
}

#[test]
fn run_command_returns_none_when_process_cannot_spawn() {
    assert_eq!(
        run_command(Path::new("."), "/definitely/missing-binary", &[]),
        None
    );
}

#[test]
fn run_command_returns_trimmed_stdout_for_successful_process() {
    assert_eq!(
        run_command(Path::new("."), "/bin/sh", &["-c", "printf ' hello\\n'"]),
        Some("hello".to_string())
    );
}

#[test]
fn parse_pull_request_ref_rejects_invalid_json() {
    assert_eq!(parse_pull_request_ref("not-json"), None);
}

#[test]
fn parse_pull_request_ref_accepts_valid_json() {
    let pull_request = parse_pull_request_ref(
        r#"{"number":42,"url":"https://github.com/dot-matrix-labs/calypso/pull/42"}"#,
    )
    .expect("pull request should parse");

    assert_eq!(pull_request.number, 42);
    assert_eq!(
        pull_request.url,
        "https://github.com/dot-matrix-labs/calypso/pull/42"
    );
}

#[test]
fn resolve_current_pull_request_returns_none_when_gh_cannot_spawn() {
    assert_eq!(
        resolve_current_pull_request_with_program(Path::new("."), "/definitely/missing-binary"),
        None
    );
}

#[test]
fn resolve_current_pull_request_parses_successful_output() {
    let temp_dir = make_temp_dir("calypso-cli-resolve-pr");
    let gh_path = temp_dir.join("fake-gh.sh");
    std::fs::write(
        &gh_path,
        "#!/bin/sh\nprintf '{\"number\":7,\"url\":\"https://github.com/dot-matrix-labs/calypso/pull/7\"}'\n",
    )
    .expect("fake gh should be written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&gh_path)
            .expect("fake gh metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&gh_path, permissions).expect("fake gh should be executable");
    }

    let pull_request = resolve_current_pull_request_with_program(
        &temp_dir,
        gh_path.to_str().expect("path should be valid utf-8"),
    )
    .expect("pull request should resolve");

    assert_eq!(pull_request.number, 7);
    assert_eq!(
        pull_request.url,
        "https://github.com/dot-matrix-labs/calypso/pull/7"
    );

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
