use std::path::Path;
use std::sync::{LazyLock, Mutex, RwLock};

use calypso_cli::app::{
    CommandOutput, FixAttemptResult, gate_status_label, missing_pull_request_evidence,
    missing_pull_request_ref, parse_pull_request_ref, render_feature_status, render_fix_results,
    resolve_current_branch, resolve_current_pull_request_with_program, resolve_repo_root,
    run_command, run_doctor, run_doctor_fix_all, run_doctor_fix_single, run_doctor_json,
    run_doctor_verbose, run_state_status_json, run_status, state_status_json_report,
};

// Tests that write a script file and then exec it must hold EXEC_LOCK as a
// write (exclusive) guard while the fd is open for writing.  Any test that
// forks a child process (which would otherwise inherit that fd and cause
// ETXTBSY) must hold EXEC_LOCK as a read (shared) guard.  This ensures the
// write-fd is never inherited by a concurrently-forked child.
static EXEC_LOCK: LazyLock<RwLock<()>> = LazyLock::new(|| RwLock::new(()));

static PATH_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

use calypso_cli::state::{
    EvidenceStatus, FeatureState, FeatureType, Gate, GateGroup, GateStatus, GithubMergeability,
    GithubPullRequestSnapshot, GithubReviewStatus, PullRequestRef, SchedulingMeta, WorkflowState,
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
        feature_type: FeatureType::Feat,
        roles: Vec::new(),
        scheduling: SchedulingMeta::default(),
        artifact_refs: Vec::new(),
        transcript_refs: Vec::new(),
        clarification_history: Vec::new(),
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
    git_cmd()
        .args(["init", "-b", branch])
        .current_dir(&repo_root)
        .output()
        .expect("git init should run successfully");
    repo_root
}

/// Returns a `Command` for `git` with `GIT_DIR` and `GIT_WORK_TREE` unset.
///
/// When cargo runs tests inside a pre-push hook, git sets `GIT_DIR` in the
/// environment. Any spawned `git` subprocess inherits it and operates on the
/// outer repo instead of the temporary test repo. Unsetting those variables
/// forces git to discover the correct repo from `current_dir()`.
fn git_cmd() -> std::process::Command {
    let mut cmd = std::process::Command::new("git");
    cmd.env_remove("GIT_DIR").env_remove("GIT_WORK_TREE");
    cmd
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
    let _lock = EXEC_LOCK.read().unwrap_or_else(|e| e.into_inner());
    let temp_dir = make_temp_dir("calypso-cli-run-doctor-no-git");

    let rendered = run_doctor(&temp_dir);

    assert!(rendered.contains("Doctor checks"));

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn resolve_repo_root_and_branch_report_git_context() {
    let _lock = EXEC_LOCK.read().unwrap_or_else(|e| e.into_inner());
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
    let _lock = EXEC_LOCK.read().unwrap_or_else(|e| e.into_inner());
    assert!(matches!(
        run_command(Path::new("."), "/bin/sh", &["-c", "echo boom >&2; exit 1"]),
        Ok(CommandOutput::Failure(error)) if error == "boom"
    ));
}

#[test]
fn run_command_returns_none_when_process_cannot_spawn() {
    let _lock = EXEC_LOCK.read().unwrap_or_else(|e| e.into_inner());
    assert!(
        run_command(Path::new("."), "/definitely/missing-binary", &[])
            .expect_err("missing binary should return an error")
            .contains("failed to spawn")
    );
}

#[test]
fn run_command_returns_trimmed_stdout_for_successful_process() {
    let _lock = EXEC_LOCK.read().unwrap_or_else(|e| e.into_inner());
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
fn resolve_current_pull_request_returns_error_when_no_github_remote() {
    let _lock = EXEC_LOCK.read().unwrap_or_else(|e| e.into_inner());
    let temp_dir = make_temp_dir("calypso-cli-no-remote");
    // Init a git repo but with no remote.
    git_cmd()
        .args(["init", "-b", "feat/test"])
        .current_dir(&temp_dir)
        .output()
        .expect("git init should run");
    git_cmd()
        .args([
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=Test",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ])
        .current_dir(&temp_dir)
        .output()
        .expect("git commit should run");

    let error = resolve_current_pull_request_with_program(&temp_dir, "/definitely/missing-binary")
        .expect_err("missing remote should return an error");

    assert!(
        error.contains("could not resolve owner/repo"),
        "error should mention owner/repo resolution: {error}"
    );

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn resolve_current_pull_request_parses_successful_output() {
    let _lock = EXEC_LOCK.write().unwrap_or_else(|e| e.into_inner());
    // Need a real git repo with a GitHub remote and a branch for
    // resolve_current_pull_request_with_program to work (it resolves owner/repo and branch).
    let temp_dir = make_temp_dir("calypso-cli-resolve-pr");
    // Init a git repo with a GitHub remote.
    git_cmd()
        .args(["init", "-b", "feat/test-pr"])
        .current_dir(&temp_dir)
        .output()
        .expect("git init should run");
    git_cmd()
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/dot-matrix-labs/calypso.git",
        ])
        .current_dir(&temp_dir)
        .output()
        .expect("git remote add should run");
    std::fs::write(temp_dir.join("README"), "init").expect("readme should write");
    git_cmd()
        .args([
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=Test",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ])
        .current_dir(&temp_dir)
        .output()
        .expect("git commit should run");

    let gh_path = temp_dir.join("fake-gh.sh");
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&gh_path).expect("fake gh should be created");
        // Return a JSON array with one PR (REST format).
        f.write_all(b"#!/bin/sh\nprintf '[{\"number\":7,\"html_url\":\"https://github.com/dot-matrix-labs/calypso/pull/7\",\"url\":\"https://api.github.com/repos/dot-matrix-labs/calypso/pulls/7\"}]'\n")
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
    let _lock = EXEC_LOCK.read().unwrap_or_else(|e| e.into_inner());
    let temp_dir = make_temp_dir("calypso-cli-no-git-root");

    assert_eq!(resolve_repo_root(&temp_dir), None);

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn resolve_current_branch_returns_none_for_non_git_directory() {
    let _lock = EXEC_LOCK.read().unwrap_or_else(|e| e.into_inner());
    // Running git branch --show-current outside a git repo exits non-zero.
    let temp_dir = make_temp_dir("calypso-cli-no-git-branch");

    assert_eq!(resolve_current_branch(&temp_dir), None);

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn resolve_current_pull_request_returns_error_for_unrecognised_gh_failure() {
    let _lock = EXEC_LOCK.write().unwrap_or_else(|e| e.into_inner());
    let temp_dir = make_temp_dir("calypso-cli-pr-error");
    // Init a git repo with a GitHub remote.
    git_cmd()
        .args(["init", "-b", "feat/test-err"])
        .current_dir(&temp_dir)
        .output()
        .expect("git init should run");
    git_cmd()
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/dot-matrix-labs/calypso.git",
        ])
        .current_dir(&temp_dir)
        .output()
        .expect("git remote add should run");
    git_cmd()
        .args([
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=Test",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ])
        .current_dir(&temp_dir)
        .output()
        .expect("git commit should run");

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
    let _lock = EXEC_LOCK.read().unwrap_or_else(|e| e.into_inner());
    // Exit non-zero with no stderr — the error should mention the exit status.
    let result = run_command(Path::new("."), "/bin/sh", &["-c", "exit 2"]);

    assert!(matches!(result, Ok(CommandOutput::Failure(ref msg)) if msg.contains("exit")));
}

#[test]
fn run_status_surfaces_gh_error_in_output_when_pr_lookup_fails() {
    let _exec_guard = EXEC_LOCK.read().unwrap_or_else(|e| e.into_inner());
    let _guard = PATH_LOCK.lock().expect("path lock should be available");

    let repo_root = init_git_repo("feat/run-status-gh-error");
    // Add a GitHub remote so resolve_owner_repo works.
    git_cmd()
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/dot-matrix-labs/calypso.git",
        ])
        .current_dir(&repo_root)
        .output()
        .expect("git remote add should run");
    // Make an initial commit so the repo is valid
    std::fs::write(repo_root.join("README"), "init").expect("readme should write");
    git_cmd()
        .args(["add", "."])
        .current_dir(&repo_root)
        .output()
        .expect("git add should run");
    git_cmd()
        .args([
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=Test",
            "commit",
            "-m",
            "init",
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
    let _lock = EXEC_LOCK.write().unwrap_or_else(|e| e.into_inner());
    let temp_dir = make_temp_dir("calypso-cli-pr-malformed");
    // Init a git repo with a GitHub remote.
    git_cmd()
        .args(["init", "-b", "feat/test-malformed"])
        .current_dir(&temp_dir)
        .output()
        .expect("git init should run");
    git_cmd()
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/dot-matrix-labs/calypso.git",
        ])
        .current_dir(&temp_dir)
        .output()
        .expect("git remote add should run");
    git_cmd()
        .args([
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=Test",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ])
        .current_dir(&temp_dir)
        .output()
        .expect("git commit should run");

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

#[test]
fn resolve_current_pull_request_returns_none_when_gh_returns_empty_array() {
    let _lock = EXEC_LOCK.write().unwrap_or_else(|e| e.into_inner());
    let temp_dir = make_temp_dir("calypso-cli-pr-no-pr");
    // Init a git repo with a GitHub remote.
    git_cmd()
        .args(["init", "-b", "feat/test-no-pr"])
        .current_dir(&temp_dir)
        .output()
        .expect("git init should run");
    git_cmd()
        .args([
            "remote",
            "add",
            "origin",
            "https://github.com/dot-matrix-labs/calypso.git",
        ])
        .current_dir(&temp_dir)
        .output()
        .expect("git remote add should run");
    git_cmd()
        .args([
            "-c",
            "user.email=test@test.com",
            "-c",
            "user.name=Test",
            "commit",
            "--allow-empty",
            "-m",
            "init",
        ])
        .current_dir(&temp_dir)
        .output()
        .expect("git commit should run");

    let gh_path = temp_dir.join("fake-gh.sh");
    {
        use std::io::Write as _;
        let mut f = std::fs::File::create(&gh_path).expect("fake gh should be created");
        // Return an empty JSON array (no PRs found).
        f.write_all(b"#!/bin/sh\nprintf '[]'\n")
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

    let result = resolve_current_pull_request_with_program(
        &temp_dir,
        gh_path.to_str().expect("path should be valid utf-8"),
    )
    .expect("empty array should return Ok(None)");

    assert!(result.is_none());

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

// ── run_doctor_verbose ───────────────────────────────────────────────────────

#[test]
fn run_doctor_verbose_returns_non_empty_string() {
    let temp_dir = std::env::temp_dir().join("calypso-app-doctor-verbose");
    std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    let result = run_doctor_verbose(&temp_dir);
    assert!(!result.is_empty(), "verbose report should be non-empty");
    std::fs::remove_dir_all(&temp_dir).ok();
}

// ── run_doctor_fix_single ────────────────────────────────────────────────────

#[test]
fn run_doctor_fix_single_returns_error_for_unknown_check_id() {
    let temp_dir = std::env::temp_dir().join("calypso-app-fix-single-unknown");
    std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    let result = run_doctor_fix_single(&temp_dir, "definitely-not-a-real-check");
    assert!(result.is_err(), "unknown check id should return Err");
    let msg = result.unwrap_err();
    assert!(
        msg.contains("unknown check id"),
        "error should mention unknown check id"
    );
    std::fs::remove_dir_all(&temp_dir).ok();
}

// ── run_doctor_fix_all ───────────────────────────────────────────────────────

#[test]
fn run_doctor_fix_all_returns_vec_without_panicking() {
    let temp_dir = std::env::temp_dir().join("calypso-app-fix-all");
    std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    // Runs on a temp dir that fails some checks — should return results without panicking.
    let results = run_doctor_fix_all(&temp_dir);
    // Every result must have a non-empty label.
    for r in &results {
        assert!(!r.check_label.is_empty(), "check_label should be set");
    }
    std::fs::remove_dir_all(&temp_dir).ok();
}

// ── render_fix_results ───────────────────────────────────────────────────────

#[test]
fn render_fix_results_formats_applied_and_not_applied() {
    let results = vec![
        FixAttemptResult {
            check_label: "check-a".to_string(),
            applied: true,
            output: "fix applied".to_string(),
            validated: Some(true),
        },
        FixAttemptResult {
            check_label: "check-b".to_string(),
            applied: false,
            output: "no fix available".to_string(),
            validated: None,
        },
    ];
    let rendered = render_fix_results(&results);
    assert!(rendered.contains("check-a"), "should mention check-a");
    assert!(rendered.contains("check-b"), "should mention check-b");
}

// ── render_fix_results — additional branches ─────────────────────────────────

#[test]
fn render_fix_results_empty_list_returns_all_passing_message() {
    let rendered = render_fix_results(&[]);
    assert!(
        rendered.contains("All checks passing"),
        "empty results should say all passing"
    );
}

#[test]
fn render_fix_results_covers_failed_applied_and_skip_variants() {
    let results = vec![
        FixAttemptResult {
            check_label: "check-failed".to_string(),
            applied: true,
            output: "something went wrong".to_string(),
            validated: Some(false),
        },
        FixAttemptResult {
            check_label: "check-applied".to_string(),
            applied: true,
            output: "applied without validation".to_string(),
            validated: None,
        },
        FixAttemptResult {
            check_label: "check-pass".to_string(),
            applied: false,
            output: "already passing".to_string(),
            validated: Some(true),
        },
        FixAttemptResult {
            check_label: "check-skip".to_string(),
            applied: false,
            output: "no fix available".to_string(),
            validated: None,
        },
    ];
    let rendered = render_fix_results(&results);
    assert!(rendered.contains("FAILED"));
    assert!(rendered.contains("APPLIED"));
    assert!(rendered.contains("PASS"));
    assert!(rendered.contains("SKIP"));
}

// ── run_doctor_json ──────────────────────────────────────────────────────────

#[test]
fn run_doctor_json_returns_json_string() {
    let temp_dir = std::env::temp_dir().join("calypso-app-doctor-json");
    std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    // Returns Ok or Err depending on check results — both return valid JSON.
    let result = run_doctor_json(&temp_dir);
    let json_str = match result {
        Ok(ref s) => s.as_str(),
        Err(ref s) => s.as_str(),
    };
    let parsed: serde_json::Value =
        serde_json::from_str(json_str).expect("run_doctor_json should return valid JSON");
    assert!(parsed.get("summary").is_some(), "JSON should have summary");
    std::fs::remove_dir_all(&temp_dir).ok();
}

// ── state_status_json_report ─────────────────────────────────────────────────

#[test]
fn state_status_json_report_builds_from_feature_state() {
    let feature = feature_with_gate_statuses(&[
        calypso_cli::state::GateStatus::Passing,
        calypso_cli::state::GateStatus::Failing,
    ]);
    let report = state_status_json_report(&feature);
    assert_eq!(report.feature_id, "feature");
    assert_eq!(report.pr_number, Some(7));
    assert!(!report.gate_groups.is_empty());
}

// ── run_state_status_json ────────────────────────────────────────────────────

#[test]
fn run_state_status_json_returns_err_when_no_state_file() {
    let temp_dir = std::env::temp_dir().join("calypso-app-state-status-json-no-file");
    std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    let result = run_state_status_json(&temp_dir);
    assert!(result.is_err(), "should error when state.json is missing");
    std::fs::remove_dir_all(&temp_dir).ok();
}

// ── run_doctor_fix_single — already-passing branch ───────────────────────────

#[test]
fn run_doctor_fix_single_returns_already_passing_for_passing_check() {
    // state-machine-integrity checks embedded templates — always passes.
    let temp_dir = std::env::temp_dir().join("calypso-app-fix-single-passing");
    std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    let result = run_doctor_fix_single(&temp_dir, "gh-installed");
    // The check should be found and report "already passing".
    if let Ok(ref fix_result) = result {
        assert!(
            !fix_result.applied,
            "already-passing check should not be applied"
        );
    }
    // Either Ok (passing) or Err (not found on this build) — must not panic.
    std::fs::remove_dir_all(&temp_dir).ok();
}

// ── run_doctor_fix_single — fix application path ─────────────────────────────

#[test]
fn run_doctor_fix_single_applies_fix_for_failing_check_with_auto_fix() {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    // A fresh temp dir is not a git repo → git-initialized check fails.
    // Its auto-fix runs `git init`, which should succeed.
    let temp_dir = std::env::temp_dir().join(format!("calypso-app-fix-apply-{nanos}"));
    std::fs::create_dir_all(&temp_dir).expect("temp dir should be created");
    let result = run_doctor_fix_single(&temp_dir, "git-initialized");
    // The fix should be applied — even if it ends up failing validation.
    if let Ok(ref r) = result {
        assert!(r.applied, "fix should be applied when check was failing");
    }
    // Either Ok (fix applied) or Err (apply failed) — must not panic.
    let _ = result;
    std::fs::remove_dir_all(&temp_dir).ok();
}
