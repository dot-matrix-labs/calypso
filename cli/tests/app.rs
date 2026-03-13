use std::path::Path;
use std::sync::{LazyLock, Mutex};

use calypso_cli::app::{
    CommandOutput, gate_status_label, missing_pull_request_evidence, missing_pull_request_ref,
    parse_pull_request_ref, render_feature_status, resolve_current_branch,
    resolve_current_pull_request_with_program, resolve_repo_root, run_command, run_doctor,
    run_status,
};

static PATH_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
use calypso_cli::state::{
    EvidenceStatus, FeatureState, Gate, GateGroup, GateStatus, GithubMergeability,
    GithubPullRequestSnapshot, GithubReviewStatus, PullRequestRef, WorkflowState,
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
        github_snapshot: None,
        github_error: None,
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
    // Use /var/tmp (persistent, non-tmpfs on Linux) instead of /tmp (tmpfs) to
    // avoid ETXTBSY when executing scripts written to the same tmpfs that the
    // instrumented test binary itself runs from under cargo-llvm-cov.
    let base = if std::path::Path::new("/var/tmp").exists() {
        std::path::PathBuf::from("/var/tmp")
    } else {
        std::env::temp_dir()
    };
    let path = base.join(unique);
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
    assert!(rendered.contains("PR checklist"));
    assert!(rendered.contains("- [x] Gate 0"));
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
    assert_eq!(
        evidence.result_for("builtin.github.pr_ready_for_review"),
        None
    );
    assert_eq!(evidence.result_for("builtin.github.pr_checks_green"), None);
}

#[test]
fn render_feature_status_includes_normalized_github_snapshot() {
    let mut feature = feature_with_gate_statuses(&[GateStatus::Passing]);
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: false,
        review_status: GithubReviewStatus::Approved,
        checks: EvidenceStatus::Passing,
        mergeability: GithubMergeability::Mergeable,
    });

    let rendered = render_feature_status(
        Path::new("/tmp/feature"),
        "feature",
        Some(&feature.pull_request),
        &feature,
    );

    assert!(rendered.contains("GitHub"));
    assert!(rendered.contains("- PR state: ready-for-review"));
    assert!(rendered.contains("- Review: approved"));
    assert!(rendered.contains("- Checks: passing"));
    assert!(rendered.contains("- Mergeability: mergeable"));
}

#[test]
fn run_command_returns_none_for_non_zero_exit() {
    assert!(matches!(
        run_command(Path::new("."), "/bin/sh", &["-c", "echo boom >&2; exit 1"]),
        Ok(CommandOutput::Failure(error)) if error == "boom"
    ));
}

#[test]
fn run_command_returns_none_when_process_cannot_spawn() {
    assert!(
        run_command(Path::new("."), "/definitely/missing-binary", &[])
            .expect_err("missing binary should return an error")
            .contains("failed to spawn")
    );
}

#[test]
fn run_command_returns_trimmed_stdout_for_successful_process() {
    assert_eq!(
        run_command(Path::new("."), "/bin/sh", &["-c", "printf ' hello\\n'"]),
        Ok(CommandOutput::Success("hello".to_string()))
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
    assert!(
        resolve_current_pull_request_with_program(Path::new("."), "/definitely/missing-binary")
            .expect_err("missing gh should return an error")
            .contains("failed to spawn")
    );
}

#[test]
fn resolve_current_pull_request_parses_successful_output() {
    let temp_dir = make_temp_dir("calypso-cli-resolve-pr");
    let gh_path = temp_dir.join("fake-gh.sh");
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&gh_path).expect("fake gh should be created");
        f.write_all(b"#!/bin/sh\nprintf '{\"number\":7,\"url\":\"https://github.com/dot-matrix-labs/calypso/pull/7\"}'\n")
            .expect("fake gh should be written");
        f.sync_all().expect("fake gh should be synced");
    }
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
    .expect("pull request lookup should succeed")
    .expect("pull request should resolve");

    assert_eq!(pull_request.number, 7);
    assert_eq!(
        pull_request.url,
        "https://github.com/dot-matrix-labs/calypso/pull/7"
    );

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn render_feature_status_includes_github_error_when_snapshot_is_unavailable() {
    let mut feature = feature_with_gate_statuses(&[GateStatus::Failing]);
    feature.github_error = Some("Run `gh auth login`.".to_string());

    let rendered = render_feature_status(
        Path::new("/tmp/feature"),
        "feature",
        Some(&feature.pull_request),
        &feature,
    );

    assert!(rendered.contains("GitHub"));
    assert!(rendered.contains("- Error: Run `gh auth login`."));
}

#[test]
fn render_feature_status_labels_all_github_review_and_mergeability_variants() {
    use calypso_cli::state::EvidenceStatus;

    // ReviewRequired → "review-required"
    let mut feature = feature_with_gate_statuses(&[GateStatus::Passing]);
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: true,
        review_status: GithubReviewStatus::ReviewRequired,
        checks: EvidenceStatus::Failing,
        mergeability: GithubMergeability::Conflicting,
    });
    let rendered = render_feature_status(
        Path::new("/tmp/feature"),
        "feature",
        Some(&feature.pull_request),
        &feature,
    );
    assert!(rendered.contains("- PR state: draft"));
    assert!(rendered.contains("- Review: review-required"));
    assert!(rendered.contains("- Checks: failing"));
    assert!(rendered.contains("- Mergeability: conflicting"));

    // ChangesRequested + Blocked + Pending checks
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: false,
        review_status: GithubReviewStatus::ChangesRequested,
        checks: EvidenceStatus::Pending,
        mergeability: GithubMergeability::Blocked,
    });
    let rendered = render_feature_status(
        Path::new("/tmp/feature"),
        "feature",
        Some(&feature.pull_request),
        &feature,
    );
    assert!(rendered.contains("- Review: changes-requested"));
    assert!(rendered.contains("- Checks: pending"));
    assert!(rendered.contains("- Mergeability: blocked"));

    // Manual checks + Unknown mergeability
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: false,
        review_status: GithubReviewStatus::Approved,
        checks: EvidenceStatus::Manual,
        mergeability: GithubMergeability::Unknown,
    });
    let rendered = render_feature_status(
        Path::new("/tmp/feature"),
        "feature",
        Some(&feature.pull_request),
        &feature,
    );
    assert!(rendered.contains("- Checks: manual"));
    assert!(rendered.contains("- Mergeability: unknown"));
}

#[test]
fn resolve_repo_root_returns_none_outside_a_git_repo() {
    let temp_dir = make_temp_dir("calypso-cli-no-git-root");

    assert_eq!(resolve_repo_root(&temp_dir), None);

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn resolve_current_branch_returns_none_for_non_git_directory() {
    // Running git branch --show-current outside a git repo exits non-zero.
    let temp_dir = make_temp_dir("calypso-cli-no-git-branch");

    assert_eq!(resolve_current_branch(&temp_dir), None);

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn resolve_current_pull_request_returns_error_for_unrecognised_gh_failure() {
    let temp_dir = make_temp_dir("calypso-cli-pr-error");
    let gh_path = temp_dir.join("fake-gh.sh");
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&gh_path).expect("fake gh should be created");
        f.write_all(b"#!/bin/sh\necho 'fatal: repository not found' >&2\nexit 1\n")
            .expect("fake gh should be written");
        f.sync_all().expect("fake gh should be synced");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&gh_path)
            .expect("fake gh metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&gh_path, permissions).expect("fake gh should be executable");
    }

    let error = resolve_current_pull_request_with_program(
        &temp_dir,
        gh_path.to_str().expect("path should be valid utf-8"),
    )
    .expect_err("unrecognised gh failure should return an error");

    assert!(error.contains("repository not found"));

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn run_command_uses_status_message_when_stderr_is_empty() {
    // Exit non-zero with no stderr — the error should mention the exit status.
    let result = run_command(
        Path::new("."),
        "/bin/sh",
        &["-c", "exit 2"],
    );

    assert!(matches!(result, Ok(CommandOutput::Failure(ref msg)) if msg.contains("exit")));
}

#[test]
fn run_status_surfaces_gh_error_in_output_when_pr_lookup_fails() {
    let _guard = PATH_LOCK.lock().expect("path lock should be available");

    let repo_root = init_git_repo("feat/run-status-gh-error");
    // Make an initial commit so the repo is valid
    std::fs::write(repo_root.join("README"), "init").expect("readme should write");
    std::process::Command::new("git")
        .args(["add", "."])
        .current_dir(&repo_root)
        .output()
        .expect("git add should run");
    std::process::Command::new("git")
        .args([
            "-c", "user.email=test@test.com",
            "-c", "user.name=Test",
            "commit", "-m", "init",
        ])
        .current_dir(&repo_root)
        .output()
        .expect("git commit should run");

    let fake_gh_dir = make_temp_dir("calypso-cli-run-status-gh-err");
    let gh_path = fake_gh_dir.join("gh");
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&gh_path).expect("fake gh should be created");
        f.write_all(b"#!/bin/sh\necho 'fatal: repo not found' >&2\nexit 1\n")
            .expect("fake gh should be written");
        f.sync_all().expect("fake gh should be synced");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&gh_path)
            .expect("fake gh metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&gh_path, permissions).expect("fake gh should be executable");
    }

    let original_path = std::env::var_os("PATH").unwrap_or_default();
    let mut search_path = std::ffi::OsString::new();
    search_path.push(&fake_gh_dir);
    search_path.push(std::ffi::OsStr::new(":"));
    search_path.push(&original_path);

    unsafe {
        std::env::set_var("PATH", &search_path);
    }

    let output = run_status(&repo_root).expect("run_status should succeed even with gh error");

    unsafe {
        std::env::set_var("PATH", original_path);
    }

    assert!(output.contains("Feature status"));
    assert!(output.contains("Error:") || output.contains("GitHub"));

    std::fs::remove_dir_all(repo_root).expect("repo should be removed");
    std::fs::remove_dir_all(fake_gh_dir).expect("fake gh dir should be removed");
}

#[test]
fn resolve_current_pull_request_returns_error_when_gh_succeeds_with_malformed_json() {
    let temp_dir = make_temp_dir("calypso-cli-pr-malformed");
    let gh_path = temp_dir.join("fake-gh.sh");
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&gh_path).expect("fake gh should be created");
        f.write_all(b"#!/bin/sh\nprintf 'not valid json'\n")
            .expect("fake gh should be written");
        f.sync_all().expect("fake gh should be synced");
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut permissions = std::fs::metadata(&gh_path)
            .expect("fake gh metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&gh_path, permissions).expect("fake gh should be executable");
    }

    let error = resolve_current_pull_request_with_program(
        &temp_dir,
        gh_path.to_str().expect("path should be valid utf-8"),
    )
    .expect_err("malformed JSON from gh should return an error");

    assert!(error.contains("malformed pull request JSON"));

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
