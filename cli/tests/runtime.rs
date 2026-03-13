use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::runtime::{
    PullRequestResolver, discover_repository_context, load_or_initialize_runtime,
};
use calypso_cli::state::{PullRequestRef, RepositoryState, WorkflowState};

struct FakePullRequestResolver {
    pull_request: PullRequestRef,
}

impl PullRequestResolver for FakePullRequestResolver {
    fn resolve_for_branch(
        &self,
        _repo_root: &Path,
        _branch: &str,
    ) -> Result<PullRequestRef, calypso_cli::runtime::RuntimeError> {
        Ok(self.pull_request.clone())
    }
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("{prefix}-{unique}"))
}

fn run_git(repo_root: &Path, args: &[&str]) {
    let output = Command::new("git")
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

fn sample_pull_request() -> PullRequestRef {
    PullRequestRef {
        number: 19,
        url: "https://github.com/org/repo/pull/19".to_string(),
    }
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
    runtime.state.current_feature.workflow_state = WorkflowState::ReadyForReview;
    runtime.save().expect("runtime state should save");

    let resumed = load_or_initialize_runtime(&repo_root, &resolver).expect("runtime should resume");

    assert_eq!(
        resumed.state.current_feature.workflow_state,
        WorkflowState::ReadyForReview
    );
    assert_eq!(resumed.state_path, runtime.state_path);

    fs::remove_dir_all(repo_root).expect("temp repo root should be removed");
}
