use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::runtime::{
    GhCliPullRequestResolver, PullRequestResolver, RuntimeError,
    discover_current_repository_context, discover_repository_context,
    load_or_initialize_current_runtime, load_or_initialize_runtime,
};
use calypso_cli::state::{
    GateInitializationError, PullRequestRef, RepositoryState, StateError, WorkflowState,
};
use calypso_cli::template::TemplateError;

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);
static PATH_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

struct FakePullRequestResolver {
    pull_request: PullRequestRef,
}

impl PullRequestResolver for FakePullRequestResolver {
    fn resolve_for_branch(
        &self,
        _repo_root: &Path,
        _branch: &str,
    ) -> Result<PullRequestRef, RuntimeError> {
        Ok(self.pull_request.clone())
    }
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!("{prefix}-{timestamp}-{counter}"))
}

fn path_mutex() -> &'static Mutex<()> {
    PATH_MUTEX.get_or_init(|| Mutex::new(()))
}

fn git_cmd() -> Command {
    let mut cmd = Command::new("git");
    cmd.env_remove("GIT_DIR").env_remove("GIT_WORK_TREE");
    cmd
}

fn run_git(repo_root: &Path, args: &[&str]) {
    let output = git_cmd()
        .args(args)
        .current_dir(repo_root)
        .output()
        .expect("git command should run");

    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_repo(branch: &str) -> PathBuf {
    let repo_root = unique_temp_dir("calypso-runtime-test");
    fs::create_dir_all(&repo_root).expect("temp repo root should be created");

    run_git(&repo_root, &["init", "--initial-branch", branch]);
    run_git(&repo_root, &["config", "user.name", "Calypso Test"]);
    run_git(
        &repo_root,
        &["config", "user.email", "calypso-test@example.com"],
    );

    fs::write(repo_root.join("README.md"), "# temp repo\n").expect("fixture file should write");
    run_git(&repo_root, &["add", "README.md"]);
    run_git(&repo_root, &["commit", "-m", "initial commit"]);

    repo_root
}

fn make_detached_head_repo() -> PathBuf {
    let repo_root = init_repo("feat/runtime-context");
    run_git(&repo_root, &["checkout", "--detach"]);
    repo_root
}

fn write_fake_gh_script(contents: &str) -> PathBuf {
    let script_dir = unique_temp_dir("calypso-gh-script");
    fs::create_dir_all(&script_dir).expect("script dir should be created");
    let script_path = script_dir.join("gh");
    fs::write(&script_path, contents).expect("script should be written");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        let mut permissions = fs::metadata(&script_path)
            .expect("script metadata should exist")
            .permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&script_path, permissions).expect("script should be executable");
    }

    script_dir
}

fn with_fake_gh<T>(script: &str, test_fn: impl FnOnce() -> T) -> T {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");
    let script_dir = write_fake_gh_script(script);
    let old_path = std::env::var_os("PATH");
    let mut new_path_parts = vec![script_dir.clone()];
    if let Some(existing_path) = old_path.as_ref() {
        new_path_parts.extend(std::env::split_paths(existing_path));
    }
    let new_path = std::env::join_paths(new_path_parts).expect("PATH should join");

    unsafe {
        std::env::set_var("PATH", &new_path);
    }

    let result = test_fn();

    match old_path {
        Some(path) => unsafe {
            std::env::set_var("PATH", path);
        },
        None => unsafe {
            std::env::remove_var("PATH");
        },
    }

    fs::remove_dir_all(script_dir).expect("script dir should be removed");
    result
}

fn sample_pull_request() -> PullRequestRef {
    PullRequestRef {
        number: 19,
        url: "https://github.com/org/repo/pull/19".to_string(),
    }
}

#[test]
fn gh_cli_pull_request_resolver_reads_pull_request_from_gh_cli_output() {
    // The resolver now calls `gh api repos/{o}/{r}/pulls?head=...` which returns a JSON array.
    // It also needs `git remote get-url origin` to resolve owner/repo, so we need a real git repo.
    let repo_root = init_repo("feat/runtime-context");
    run_git(
        &repo_root,
        &["remote", "add", "origin", "https://github.com/org/repo.git"],
    );

    with_fake_gh(
        "#!/bin/sh\nprintf '[{\"number\":19,\"html_url\":\"https://github.com/org/repo/pull/19\",\"url\":\"https://api.github.com/repos/org/repo/pulls/19\"}]'\n",
        || {
            let resolver = GhCliPullRequestResolver;
            let pull_request = resolver
                .resolve_for_branch(&repo_root, "feat/runtime-context")
                .expect("resolver should parse fake gh output");

            assert_eq!(pull_request, sample_pull_request());
        },
    );

    fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
}

#[test]
fn gh_cli_pull_request_resolver_reports_command_failures() {
    let repo_root = init_repo("feat/runtime-context");
    run_git(
        &repo_root,
        &["remote", "add", "origin", "https://github.com/org/repo.git"],
    );

    with_fake_gh("#!/bin/sh\necho 'boom' >&2\nexit 1\n", || {
        let resolver = GhCliPullRequestResolver;
        let error = resolver
            .resolve_for_branch(&repo_root, "feat/runtime-context")
            .expect_err("resolver should report gh failures");

        assert!(matches!(error, RuntimeError::CommandFailed { .. }));
        assert!(error.to_string().contains("boom"));
    });

    fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
}

#[test]
fn gh_cli_pull_request_resolver_reports_missing_pull_requests() {
    let repo_root = init_repo("feat/runtime-context");
    run_git(
        &repo_root,
        &["remote", "add", "origin", "https://github.com/org/repo.git"],
    );

    with_fake_gh("#!/bin/sh\nprintf '[]'\n", || {
        let resolver = GhCliPullRequestResolver;
        let error = resolver
            .resolve_for_branch(&repo_root, "feat/runtime-context")
            .expect_err("resolver should fail when no PR is returned");

        assert!(matches!(error, RuntimeError::PullRequestNotFound(_)));
        assert!(error.to_string().contains("feat/runtime-context"));
    });

    fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
}

#[test]
fn repository_context_discovers_git_root_branch_and_feature_binding() {
    let repo_root = init_repo("feat/runtime-context");
    let nested_dir = repo_root.join("src");
    fs::create_dir_all(&nested_dir).expect("nested dir should be created");
    let canonical_repo_root = repo_root
        .canonicalize()
        .expect("temp repo root should canonicalize");

    let context = discover_repository_context(
        &nested_dir,
        &FakePullRequestResolver {
            pull_request: sample_pull_request(),
        },
    )
    .expect("repository context should resolve");

    assert_eq!(context.repo_root, canonical_repo_root);
    assert_eq!(context.branch, "feat/runtime-context");
    assert_eq!(context.feature.feature_id, "feat/runtime-context");
    assert_eq!(context.feature.branch, "feat/runtime-context");
    assert_eq!(
        context.feature.worktree_path,
        context.repo_root.display().to_string()
    );
    assert_eq!(context.feature.pull_request, sample_pull_request());

    fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
}

#[test]
fn discover_current_repository_context_uses_gh_cli_resolver() {
    with_fake_gh(
        "#!/bin/sh\nprintf '[{\"number\":19,\"html_url\":\"https://github.com/org/repo/pull/19\",\"url\":\"https://api.github.com/repos/org/repo/pulls/19\"}]'\n",
        || {
            let repo_root = init_repo("feat/runtime-context");
            run_git(
                &repo_root,
                &["remote", "add", "origin", "https://github.com/org/repo.git"],
            );
            let context = discover_current_repository_context(&repo_root)
                .expect("current repository context should resolve");

            assert_eq!(context.feature.pull_request, sample_pull_request());

            fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
        },
    );
}

#[test]
fn discover_repository_context_rejects_detached_head_checkouts() {
    let repo_root = make_detached_head_repo();
    let error = discover_repository_context(
        &repo_root,
        &FakePullRequestResolver {
            pull_request: sample_pull_request(),
        },
    )
    .expect_err("detached HEAD should be rejected");

    assert!(matches!(error, RuntimeError::DetachedHead));

    fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
}

#[test]
fn discover_repository_context_reports_git_failures_for_non_repositories() {
    let directory = unique_temp_dir("calypso-non-repo");
    fs::create_dir_all(&directory).expect("directory should be created");

    let error = discover_repository_context(
        &directory,
        &FakePullRequestResolver {
            pull_request: sample_pull_request(),
        },
    )
    .expect_err("non-repository discovery should fail");

    assert!(matches!(error, RuntimeError::CommandFailed { .. }));
    assert!(error.to_string().contains("git command failed"));

    fs::remove_dir_all(directory).expect("temp dir should be removed");
}

#[test]
fn load_or_initialize_runtime_creates_and_persists_repository_state() {
    let repo_root = init_repo("feat/runtime-context");

    let runtime = load_or_initialize_runtime(
        &repo_root,
        &FakePullRequestResolver {
            pull_request: sample_pull_request(),
        },
    )
    .expect("runtime should load");

    assert_eq!(runtime.state.version, 1);
    assert_eq!(
        runtime.state.current_feature.feature_id,
        "feat/runtime-context"
    );
    assert_eq!(
        runtime.state.current_feature.pull_request,
        sample_pull_request()
    );
    assert!(runtime.state_path.exists());

    let persisted =
        RepositoryState::load_from_path(&runtime.state_path).expect("persisted state should load");
    assert_eq!(persisted, runtime.state);

    fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
}

#[test]
fn load_or_initialize_runtime_resumes_existing_repository_state() {
    let repo_root = init_repo("feat/runtime-context");
    let resolver = FakePullRequestResolver {
        pull_request: sample_pull_request(),
    };

    let mut runtime =
        load_or_initialize_runtime(&repo_root, &resolver).expect("runtime should initialize");
    runtime.state.current_feature.workflow_state = WorkflowState::ReleaseReady;
    runtime.save().expect("runtime state should save");

    let resumed = load_or_initialize_runtime(&repo_root, &resolver).expect("runtime should resume");

    assert_eq!(
        resumed.state.current_feature.workflow_state,
        WorkflowState::ReleaseReady
    );
    assert_eq!(resumed.state_path, runtime.state_path);

    fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
}

#[test]
fn load_or_initialize_current_runtime_uses_gh_cli_resolver() {
    with_fake_gh(
        "#!/bin/sh\nprintf '[{\"number\":19,\"html_url\":\"https://github.com/org/repo/pull/19\",\"url\":\"https://api.github.com/repos/org/repo/pulls/19\"}]'\n",
        || {
            let repo_root = init_repo("feat/runtime-context");
            run_git(
                &repo_root,
                &["remote", "add", "origin", "https://github.com/org/repo.git"],
            );
            let runtime = load_or_initialize_current_runtime(&repo_root)
                .expect("current runtime should load through gh");

            assert_eq!(
                runtime.state.current_feature.pull_request,
                sample_pull_request()
            );

            fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
        },
    );
}

#[test]
fn load_or_initialize_runtime_rejects_state_branch_mismatches() {
    let repo_root = init_repo("feat/runtime-context");
    let resolver = FakePullRequestResolver {
        pull_request: sample_pull_request(),
    };

    let runtime =
        load_or_initialize_runtime(&repo_root, &resolver).expect("runtime should initialize");
    let mut state =
        RepositoryState::load_from_path(&runtime.state_path).expect("state should load from disk");
    state.current_feature.branch = "feat/different-branch".to_string();
    state
        .save_to_path(&runtime.state_path)
        .expect("mismatched state should save");

    let error = load_or_initialize_runtime(&repo_root, &resolver)
        .expect_err("branch mismatch should be rejected");

    assert!(matches!(error, RuntimeError::StateBranchMismatch { .. }));
    assert!(error.to_string().contains("feat/different-branch"));

    fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
}

#[test]
fn runtime_save_handles_parentless_paths() {
    let repo_root = init_repo("feat/runtime-context");
    let resolver = FakePullRequestResolver {
        pull_request: sample_pull_request(),
    };

    let mut runtime =
        load_or_initialize_runtime(&repo_root, &resolver).expect("runtime should initialize");
    runtime.state_path = PathBuf::new();

    let error = runtime
        .save()
        .expect_err("saving to an empty path should report a state error");

    assert!(matches!(error, RuntimeError::State(_)));

    fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
}

#[test]
fn runtime_error_display_covers_all_variants() {
    let io_error = RuntimeError::Io(std::io::Error::other("io boom"));
    assert!(io_error.to_string().contains("runtime I/O error"));

    let json_error = RuntimeError::Json(
        serde_json::from_str::<serde_json::Value>("{ nope")
            .expect_err("json parse should fail for fixture"),
    );
    assert!(json_error.to_string().contains("runtime JSON error"));

    let template_error =
        RuntimeError::Template(TemplateError::Validation("bad template".to_string()));
    assert!(
        template_error
            .to_string()
            .contains("runtime template error: template validation error: bad template")
    );

    let state_error = RuntimeError::State(StateError::Io(std::io::Error::other("state boom")));
    assert!(
        state_error
            .to_string()
            .contains("runtime state error: state I/O error")
    );

    let gate_error = RuntimeError::GateInitialization(
        GateInitializationError::UnknownWorkflowState("made-up".to_string()),
    );
    assert!(
        gate_error
            .to_string()
            .contains("runtime gate initialization error")
    );

    let command_error = RuntimeError::CommandFailed {
        program: "gh".to_string(),
        details: "boom".to_string(),
    };
    assert_eq!(command_error.to_string(), "gh command failed: boom");

    let missing_pr = RuntimeError::PullRequestNotFound("feat/runtime-context".to_string());
    assert!(
        missing_pr
            .to_string()
            .contains("no pull request found for branch 'feat/runtime-context'")
    );

    let missing_repo_name = RuntimeError::MissingRepositoryName;
    assert!(
        missing_repo_name
            .to_string()
            .contains("could not derive a repository identifier")
    );

    let detached_head = RuntimeError::DetachedHead;
    assert!(
        detached_head
            .to_string()
            .contains("current git checkout is detached")
    );

    let branch_mismatch = RuntimeError::StateBranchMismatch {
        expected: "feat/runtime-context".to_string(),
        actual: "feat/different-branch".to_string(),
    };
    assert!(
        branch_mismatch
            .to_string()
            .contains("repository state belongs to branch 'feat/different-branch'")
    );
}
