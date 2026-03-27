use std::cell::RefCell;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use nightshift_core::feature_start::{
    FeatureStartEnvironment, FeatureStartError, FeatureStartRequest, HostFeatureStartEnvironment,
    derive_feature_branch_name, start_feature,
};
use nightshift_core::state::{PullRequestRef, RepositoryState};

static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);
static PATH_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

#[derive(Debug, Clone)]
struct FakeEnvironment {
    repo_root: PathBuf,
    current_branch: String,
    clean: bool,
    main_exists: bool,
    existing_branches: BTreeSet<String>,
    existing_paths: BTreeSet<PathBuf>,
    worktree_failure: Option<String>,
    push_failure: Option<String>,
    pr_failure: Option<String>,
    bootstrap_failure: Option<String>,
    actions: RefCell<Vec<String>>,
}

impl Default for FakeEnvironment {
    fn default() -> Self {
        Self {
            repo_root: PathBuf::from("/repo"),
            current_branch: "main".to_string(),
            clean: true,
            main_exists: true,
            existing_branches: BTreeSet::new(),
            existing_paths: BTreeSet::new(),
            worktree_failure: None,
            push_failure: None,
            pr_failure: None,
            bootstrap_failure: None,
            actions: RefCell::new(Vec::new()),
        }
    }
}

impl FeatureStartEnvironment for FakeEnvironment {
    fn resolve_repo_root(&self, _start_path: &Path) -> Result<PathBuf, FeatureStartError> {
        Ok(self.repo_root.clone())
    }

    fn current_branch(&self, _repo_root: &Path) -> Result<String, FeatureStartError> {
        Ok(self.current_branch.clone())
    }

    fn is_working_tree_clean(&self, _repo_root: &Path) -> Result<bool, FeatureStartError> {
        Ok(self.clean)
    }

    fn main_branch_exists(&self, _repo_root: &Path) -> Result<bool, FeatureStartError> {
        Ok(self.main_exists)
    }

    fn branch_exists(&self, _repo_root: &Path, branch: &str) -> Result<bool, FeatureStartError> {
        Ok(self.existing_branches.contains(branch))
    }

    fn path_exists(&self, path: &Path) -> bool {
        self.existing_paths.contains(path)
    }

    fn create_branch_from_main(
        &self,
        _repo_root: &Path,
        branch: &str,
    ) -> Result<(), FeatureStartError> {
        self.actions
            .borrow_mut()
            .push(format!("create-branch:{branch}"));
        Ok(())
    }

    fn create_worktree(
        &self,
        _repo_root: &Path,
        branch: &str,
        worktree_path: &Path,
    ) -> Result<(), FeatureStartError> {
        self.actions.borrow_mut().push(format!(
            "create-worktree:{branch}:{}",
            worktree_path.display()
        ));
        match &self.worktree_failure {
            Some(message) => Err(FeatureStartError::GitCommandFailed {
                action: "git worktree add".to_string(),
                details: message.clone(),
            }),
            None => Ok(()),
        }
    }

    fn push_branch(&self, worktree_path: &Path, branch: &str) -> Result<(), FeatureStartError> {
        self.actions
            .borrow_mut()
            .push(format!("push-branch:{branch}:{}", worktree_path.display()));
        match &self.push_failure {
            Some(message) => Err(FeatureStartError::GitCommandFailed {
                action: "git push".to_string(),
                details: message.clone(),
            }),
            None => Ok(()),
        }
    }

    fn create_draft_pull_request(
        &self,
        worktree_path: &Path,
        branch: &str,
        title: &str,
        _body: &str,
    ) -> Result<PullRequestRef, FeatureStartError> {
        self.actions.borrow_mut().push(format!(
            "create-pr:{branch}:{title}:{}",
            worktree_path.display()
        ));
        match &self.pr_failure {
            Some(message) => Err(FeatureStartError::GithubCommandFailed {
                action: "gh pr create".to_string(),
                details: message.clone(),
            }),
            None => Ok(PullRequestRef {
                number: 27,
                url: "https://github.com/dot-matrix-labs/calypso/pull/27".to_string(),
            }),
        }
    }

    fn bootstrap_state(
        &self,
        worktree_path: &Path,
        _pull_request: PullRequestRef,
    ) -> Result<PathBuf, FeatureStartError> {
        self.actions
            .borrow_mut()
            .push(format!("bootstrap-state:{}", worktree_path.display()));
        match &self.bootstrap_failure {
            Some(message) => Err(FeatureStartError::GitCommandFailed {
                action: "bootstrap".to_string(),
                details: message.clone(),
            }),
            None => Ok(worktree_path.join(".calypso/repository-state.json")),
        }
    }

    fn remove_worktree(
        &self,
        _repo_root: &Path,
        worktree_path: &Path,
    ) -> Result<(), FeatureStartError> {
        self.actions
            .borrow_mut()
            .push(format!("remove-worktree:{}", worktree_path.display()));
        Ok(())
    }

    fn remove_branch(&self, _repo_root: &Path, branch: &str) -> Result<(), FeatureStartError> {
        self.actions
            .borrow_mut()
            .push(format!("remove-branch:{branch}"));
        Ok(())
    }

    fn update_pull_request_body(
        &self,
        _worktree_path: &Path,
        pr_number: u64,
        _body: &str,
    ) -> Result<(), FeatureStartError> {
        self.actions
            .borrow_mut()
            .push(format!("update-pr-body:{pr_number}"));
        Ok(())
    }
}

fn sample_request() -> FeatureStartRequest {
    FeatureStartRequest {
        feature_id: "CLI Feature Start".to_string(),
        worktree_base: PathBuf::from("/worktrees"),
        title: None,
        body: None,
        allow_dirty: false,
        allow_non_main: false,
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

fn init_repo_with_remote() -> (PathBuf, PathBuf) {
    let bare_remote = unique_temp_dir("calypso-feature-start-remote.git");
    fs::create_dir_all(&bare_remote).expect("remote dir should exist");
    run_git(&bare_remote, &["init", "--bare"]);

    let repo_root = unique_temp_dir("calypso-feature-start-repo");
    fs::create_dir_all(&repo_root).expect("repo dir should exist");
    run_git(&repo_root, &["init", "--initial-branch", "main"]);
    run_git(&repo_root, &["config", "user.name", "Calypso Test"]);
    run_git(
        &repo_root,
        &["config", "user.email", "calypso-test@example.com"],
    );
    // Use a GitHub-style URL for the origin remote so resolve_owner_repo works.
    // We use the bare_remote as a separate "real" remote for push/fetch.
    run_git(
        &repo_root,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/dot-matrix-labs/calypso.git",
        ],
    );
    // Add the bare remote as "local" for actual push operations.
    run_git(
        &repo_root,
        &[
            "remote",
            "add",
            "local",
            bare_remote.to_string_lossy().as_ref(),
        ],
    );
    fs::write(repo_root.join("README.md"), "# feature start\n").expect("fixture file should write");
    run_git(&repo_root, &["add", "README.md"]);
    run_git(&repo_root, &["commit", "-m", "initial commit"]);
    run_git(&repo_root, &["push", "-u", "local", "main"]);

    (repo_root, bare_remote)
}

fn write_fake_script(prefix: &str, name: &str, contents: &str) -> PathBuf {
    let script_dir = unique_temp_dir(prefix);
    fs::create_dir_all(&script_dir).expect("script dir should exist");
    let script_path = script_dir.join(name);
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

fn with_fake_path_commands<T>(commands: &[(&str, &str, &str)], test_fn: impl FnOnce() -> T) -> T {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let mut script_dirs = Vec::new();
    for (prefix, name, script) in commands {
        script_dirs.push(write_fake_script(prefix, name, script));
    }

    let old_path = std::env::var_os("PATH");
    let mut new_path_parts = script_dirs.clone();
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

    for script_dir in script_dirs {
        fs::remove_dir_all(script_dir).expect("script dir should be removed");
    }
    result
}
#[test]
fn derive_feature_branch_name_slugifies_identifiers_deterministically() {
    assert_eq!(
        derive_feature_branch_name("CLI Feature Start").expect("branch should derive"),
        "feat/cli-feature-start"
    );
    assert_eq!(
        derive_feature_branch_name(" API_v2: Auth Refresh ").expect("branch should derive"),
        "feat/api-v2-auth-refresh"
    );
}

#[test]
fn start_feature_rejects_dirty_working_trees_before_mutation() {
    let environment = FakeEnvironment {
        clean: false,
        ..FakeEnvironment::default()
    };

    let error = start_feature(Path::new("."), &sample_request(), &environment)
        .expect_err("dirty tree should fail");

    assert!(matches!(error, FeatureStartError::DirtyWorkingTree));
    assert!(environment.actions.borrow().is_empty());
}

#[test]
fn start_feature_rejects_non_main_base_branch_by_default() {
    let environment = FakeEnvironment {
        current_branch: "feat/existing".to_string(),
        ..FakeEnvironment::default()
    };

    let error = start_feature(Path::new("."), &sample_request(), &environment)
        .expect_err("non-main base should fail");

    assert!(matches!(error, FeatureStartError::InvalidBaseBranch { .. }));
    assert!(environment.actions.borrow().is_empty());
}

#[test]
fn start_feature_rolls_back_branch_when_worktree_creation_fails() {
    let environment = FakeEnvironment {
        worktree_failure: Some("path exists".to_string()),
        ..FakeEnvironment::default()
    };

    let error = start_feature(Path::new("."), &sample_request(), &environment)
        .expect_err("worktree failure should bubble up");

    assert!(matches!(error, FeatureStartError::GitCommandFailed { .. }));
    assert_eq!(
        environment.actions.borrow().as_slice(),
        &[
            "create-branch:feat/cli-feature-start".to_string(),
            "create-worktree:feat/cli-feature-start:/worktrees/feat-cli-feature-start".to_string(),
            "remove-branch:feat/cli-feature-start".to_string(),
        ]
    );
}

#[test]
fn start_feature_reports_partial_failure_when_pull_request_creation_fails() {
    let environment = FakeEnvironment {
        pr_failure: Some("api down".to_string()),
        ..FakeEnvironment::default()
    };

    let error = start_feature(Path::new("."), &sample_request(), &environment)
        .expect_err("pr creation failure should report recovery");

    match error {
        FeatureStartError::PartialFailure { message, recovery } => {
            assert!(message.contains("api down"));
            assert_eq!(recovery.len(), 2);
        }
        other => panic!("expected partial failure, got {other:?}"),
    }

    assert_eq!(
        environment.actions.borrow().as_slice(),
        &[
            "create-branch:feat/cli-feature-start".to_string(),
            "create-worktree:feat/cli-feature-start:/worktrees/feat-cli-feature-start".to_string(),
            "push-branch:feat/cli-feature-start:/worktrees/feat-cli-feature-start".to_string(),
            "create-pr:feat/cli-feature-start:CLI Feature Start:/worktrees/feat-cli-feature-start"
                .to_string(),
            "remove-worktree:/worktrees/feat-cli-feature-start".to_string(),
            "remove-branch:feat/cli-feature-start".to_string(),
        ]
    );
}

#[test]
fn start_feature_returns_branch_worktree_pull_request_and_state_path_on_success() {
    let environment = FakeEnvironment::default();

    let result = start_feature(Path::new("."), &sample_request(), &environment)
        .expect("feature start should succeed");

    assert_eq!(result.branch, "feat/cli-feature-start");
    assert_eq!(
        result.worktree_path,
        PathBuf::from("/worktrees/feat-cli-feature-start")
    );
    assert_eq!(result.pull_request.number, 27);
    assert_eq!(
        result.state_path,
        PathBuf::from("/worktrees/feat-cli-feature-start/.calypso/repository-state.json")
    );
    assert_eq!(
        environment.actions.borrow().as_slice(),
        &[
            "create-branch:feat/cli-feature-start".to_string(),
            "create-worktree:feat/cli-feature-start:/worktrees/feat-cli-feature-start".to_string(),
            "push-branch:feat/cli-feature-start:/worktrees/feat-cli-feature-start".to_string(),
            "create-pr:feat/cli-feature-start:CLI Feature Start:/worktrees/feat-cli-feature-start"
                .to_string(),
            "bootstrap-state:/worktrees/feat-cli-feature-start".to_string(),
            "update-pr-body:27".to_string(),
        ]
    );
}

#[test]
fn start_feature_creates_real_git_worktree_and_seeded_state() {
    with_fake_path_commands(
        &[
            (
                "calypso-feature-start-gh",
                "gh",
                "#!/bin/sh\nif [ \"$1\" = \"api\" ] && echo \"$*\" | grep -q 'pulls'; then\n  printf '{\"number\":27,\"html_url\":\"https://github.com/dot-matrix-labs/calypso/pull/27\",\"url\":\"https://api.github.com/repos/dot-matrix-labs/calypso/pulls/27\"}'\n  exit 0\nfi\nprintf 'unexpected gh invocation: %s\\n' \"$*\" >&2\nexit 1\n",
            ),
            (
                "calypso-feature-start-git",
                "git",
                "#!/bin/sh\nif [ \"$1\" = \"push\" ]; then\n  exit 0\nfi\nexec /usr/bin/git \"$@\"\n",
            ),
        ],
        || {
            let (repo_root, bare_remote) = init_repo_with_remote();
            let worktree_base = unique_temp_dir("calypso-feature-start-worktrees");
            fs::create_dir_all(&worktree_base).expect("worktree base should exist");
            let request = FeatureStartRequest {
                feature_id: "CLI Feature Start".to_string(),
                worktree_base: worktree_base.clone(),
                title: None,
                body: None,
                allow_dirty: false,
                allow_non_main: false,
            };

            let result = start_feature(&repo_root, &request, &HostFeatureStartEnvironment)
                .expect("feature start should succeed in a real repo");

            assert_eq!(result.branch, "feat/cli-feature-start");
            assert!(result.worktree_path.exists());
            assert!(result.state_path.exists());

            let persisted = RepositoryState::load_from_path(&result.state_path)
                .expect("seeded repository state should load");
            assert_eq!(
                persisted.current_feature.branch,
                "feat/cli-feature-start".to_string()
            );
            assert_eq!(
                persisted.current_feature.worktree_path,
                result.worktree_path.display().to_string()
            );
            assert_eq!(persisted.current_feature.pull_request.number, 27);

            let worktree_git = git_cmd()
                .args(["branch", "--show-current"])
                .current_dir(&result.worktree_path)
                .output()
                .expect("git branch should run");
            assert!(worktree_git.status.success());
            assert_eq!(
                String::from_utf8(worktree_git.stdout)
                    .expect("branch stdout should be utf-8")
                    .trim(),
                "feat/cli-feature-start"
            );

            fs::remove_dir_all(repo_root).expect("repo root should be removed");
            fs::remove_dir_all(bare_remote).expect("bare remote should be removed");
            fs::remove_dir_all(worktree_base).expect("worktree base should be removed");
        },
    );
}

#[test]
fn start_feature_rejects_detached_head() {
    let environment = FakeEnvironment {
        current_branch: String::new(),
        ..FakeEnvironment::default()
    };

    let error = start_feature(Path::new("."), &sample_request(), &environment)
        .expect_err("detached HEAD should fail");

    assert!(matches!(error, FeatureStartError::DetachedHead));
    assert!(environment.actions.borrow().is_empty());
}

#[test]
fn start_feature_rejects_missing_main_branch() {
    let environment = FakeEnvironment {
        main_exists: false,
        ..FakeEnvironment::default()
    };

    let error = start_feature(Path::new("."), &sample_request(), &environment)
        .expect_err("missing main branch should fail");

    assert!(matches!(error, FeatureStartError::MissingMainBranch));
    assert!(environment.actions.borrow().is_empty());
}

#[test]
fn start_feature_rejects_existing_branch() {
    let mut existing_branches = BTreeSet::new();
    existing_branches.insert("feat/cli-feature-start".to_string());
    let environment = FakeEnvironment {
        existing_branches,
        ..FakeEnvironment::default()
    };

    let error = start_feature(Path::new("."), &sample_request(), &environment)
        .expect_err("existing branch should fail");

    assert!(matches!(error, FeatureStartError::BranchAlreadyExists(_)));
    assert!(environment.actions.borrow().is_empty());
}

#[test]
fn start_feature_rejects_existing_worktree_path() {
    let mut existing_paths = BTreeSet::new();
    existing_paths.insert(PathBuf::from("/worktrees/feat-cli-feature-start"));
    let environment = FakeEnvironment {
        existing_paths,
        ..FakeEnvironment::default()
    };

    let error = start_feature(Path::new("."), &sample_request(), &environment)
        .expect_err("existing worktree path should fail");

    assert!(matches!(error, FeatureStartError::WorktreePathExists(_)));
    assert!(environment.actions.borrow().is_empty());
}

#[test]
fn start_feature_rolls_back_when_push_fails() {
    let environment = FakeEnvironment {
        push_failure: Some("permission denied".to_string()),
        ..FakeEnvironment::default()
    };

    let error = start_feature(Path::new("."), &sample_request(), &environment)
        .expect_err("push failure should bubble up");

    assert!(matches!(error, FeatureStartError::GitCommandFailed { .. }));
    assert_eq!(
        environment.actions.borrow().as_slice(),
        &[
            "create-branch:feat/cli-feature-start".to_string(),
            "create-worktree:feat/cli-feature-start:/worktrees/feat-cli-feature-start".to_string(),
            "push-branch:feat/cli-feature-start:/worktrees/feat-cli-feature-start".to_string(),
            "remove-worktree:/worktrees/feat-cli-feature-start".to_string(),
            "remove-branch:feat/cli-feature-start".to_string(),
        ]
    );
}

#[test]
fn start_feature_reports_partial_failure_when_bootstrap_state_fails() {
    let environment = FakeEnvironment {
        bootstrap_failure: Some("runtime error".to_string()),
        ..FakeEnvironment::default()
    };

    let error = start_feature(Path::new("."), &sample_request(), &environment)
        .expect_err("bootstrap failure should report recovery");

    match error {
        FeatureStartError::PartialFailure { message, recovery } => {
            assert!(message.contains("runtime error"));
            assert_eq!(recovery.len(), 3);
            assert!(recovery[0].contains("pull/27"));
            assert!(recovery[1].contains("--delete"));
        }
        other => panic!("expected partial failure, got {other:?}"),
    }

    assert_eq!(
        environment.actions.borrow().as_slice(),
        &[
            "create-branch:feat/cli-feature-start".to_string(),
            "create-worktree:feat/cli-feature-start:/worktrees/feat-cli-feature-start".to_string(),
            "push-branch:feat/cli-feature-start:/worktrees/feat-cli-feature-start".to_string(),
            "create-pr:feat/cli-feature-start:CLI Feature Start:/worktrees/feat-cli-feature-start"
                .to_string(),
            "bootstrap-state:/worktrees/feat-cli-feature-start".to_string(),
            "remove-worktree:/worktrees/feat-cli-feature-start".to_string(),
            "remove-branch:feat/cli-feature-start".to_string(),
        ]
    );
}

#[test]
fn derive_feature_branch_name_rejects_all_non_alphanumeric_input() {
    let error = derive_feature_branch_name("---").expect_err("blank slug should fail");
    assert!(matches!(error, FeatureStartError::EmptyFeatureId));

    let error = derive_feature_branch_name("   ").expect_err("blank whitespace should fail");
    assert!(matches!(error, FeatureStartError::EmptyFeatureId));
}

#[test]
fn feature_start_error_display_covers_all_variants() {
    let io_error = std::io::Error::new(std::io::ErrorKind::NotFound, "not found");
    assert!(
        FeatureStartError::Io(io_error)
            .to_string()
            .contains("I/O error")
    );

    let runtime_error = nightshift_core::runtime::RuntimeError::MissingRepositoryName;
    assert!(
        FeatureStartError::Runtime(runtime_error)
            .to_string()
            .contains("runtime error")
    );

    assert_eq!(
        FeatureStartError::EmptyFeatureId.to_string(),
        "feature identifier must not be empty"
    );

    assert!(
        FeatureStartError::DetachedHead
            .to_string()
            .contains("detached HEAD")
    );

    assert!(
        FeatureStartError::DirtyWorkingTree
            .to_string()
            .contains("clean working tree")
    );

    assert!(
        FeatureStartError::MissingMainBranch
            .to_string()
            .contains("`main` branch")
    );

    assert!(
        FeatureStartError::InvalidBaseBranch {
            expected: "main".to_string(),
            actual: "feat/other".to_string(),
        }
        .to_string()
        .contains("feat/other")
    );

    assert!(
        FeatureStartError::BranchAlreadyExists("feat/foo".to_string())
            .to_string()
            .contains("feat/foo")
    );

    assert!(
        FeatureStartError::WorktreePathExists(PathBuf::from("/worktrees/foo"))
            .to_string()
            .contains("/worktrees/foo")
    );

    assert!(
        FeatureStartError::GitCommandFailed {
            action: "git branch".to_string(),
            details: "fatal: ref exists".to_string(),
        }
        .to_string()
        .contains("git branch")
    );

    assert!(
        FeatureStartError::GithubCommandFailed {
            action: "gh pr create".to_string(),
            details: "api down".to_string(),
        }
        .to_string()
        .contains("gh pr create")
    );

    let partial_no_recovery = FeatureStartError::PartialFailure {
        message: "something failed".to_string(),
        recovery: vec![],
    };
    assert!(partial_no_recovery.to_string().contains("something failed"));

    let partial_with_recovery = FeatureStartError::PartialFailure {
        message: "push failed".to_string(),
        recovery: vec!["delete the branch".to_string()],
    };
    let display = partial_with_recovery.to_string();
    assert!(display.contains("push failed"));
    assert!(display.contains("delete the branch"));
}

#[test]
fn host_environment_rolls_back_branch_and_worktree_when_gh_pr_create_fails() {
    with_fake_path_commands(
        &[
            (
                "calypso-feature-start-gh-fail",
                "gh",
                "#!/bin/sh\nprintf 'gh api unavailable\\n' >&2\nexit 1\n",
            ),
            (
                "calypso-feature-start-git-passthrough",
                "git",
                "#!/bin/sh\nif [ \"$1\" = \"push\" ]; then\n  exit 0\nfi\nexec /usr/bin/git \"$@\"\n",
            ),
        ],
        || {
            let (repo_root, bare_remote) = init_repo_with_remote();
            let worktree_base = unique_temp_dir("calypso-feature-start-worktrees");
            let request = FeatureStartRequest {
                feature_id: "GH Fail Test".to_string(),
                worktree_base: worktree_base.clone(),
                title: None,
                body: None,
                allow_dirty: false,
                allow_non_main: false,
            };

            let error = start_feature(&repo_root, &request, &HostFeatureStartEnvironment)
                .expect_err("gh failure should propagate as partial failure");

            assert!(matches!(error, FeatureStartError::PartialFailure { .. }));

            let worktree_path = worktree_base.join("feat-gh-fail-test");
            assert!(
                !worktree_path.exists(),
                "worktree should be removed on rollback"
            );

            let branch_check = git_cmd()
                .args([
                    "show-ref",
                    "--verify",
                    "--quiet",
                    "refs/heads/feat/gh-fail-test",
                ])
                .current_dir(&repo_root)
                .output()
                .expect("git show-ref should run");
            assert!(
                !branch_check.status.success(),
                "branch should be removed on rollback"
            );

            fs::remove_dir_all(repo_root).expect("repo root should be removed");
            fs::remove_dir_all(bare_remote).expect("bare remote should be removed");
            if worktree_base.exists() {
                fs::remove_dir_all(worktree_base).expect("worktree base should be removed");
            }
        },
    );
}

#[test]
fn run_feature_start_delegates_to_host_environment() {
    with_fake_path_commands(
        &[
            (
                "calypso-run-feature-start-gh",
                "gh",
                "#!/bin/sh\nif [ \"$1\" = \"api\" ] && echo \"$*\" | grep -q 'pulls'; then\n  printf '{\"number\":1,\"html_url\":\"https://github.com/dot-matrix-labs/calypso/pull/1\",\"url\":\"https://api.github.com/repos/dot-matrix-labs/calypso/pulls/1\"}'\n  exit 0\nfi\nprintf 'unexpected: %s\\n' \"$*\" >&2\nexit 1\n",
            ),
            (
                "calypso-run-feature-start-git",
                "git",
                "#!/bin/sh\nif [ \"$1\" = \"push\" ]; then\n  exit 0\nfi\nexec /usr/bin/git \"$@\"\n",
            ),
        ],
        || {
            let (repo_root, bare_remote) = init_repo_with_remote();
            let worktree_base = unique_temp_dir("calypso-run-feature-start-worktrees");
            fs::create_dir_all(&worktree_base).expect("worktree base should exist");
            let request = FeatureStartRequest {
                feature_id: "Run Feature Start".to_string(),
                worktree_base: worktree_base.clone(),
                title: None,
                body: None,
                allow_dirty: false,
                allow_non_main: false,
            };

            let result = nightshift_core::feature_start::run_feature_start(&repo_root, &request)
                .expect("run_feature_start should succeed");

            assert_eq!(result.branch, "feat/run-feature-start");
            assert!(result.worktree_path.exists());
            assert!(result.state_path.exists());

            fs::remove_dir_all(repo_root).expect("repo root should be removed");
            fs::remove_dir_all(bare_remote).expect("bare remote should be removed");
            fs::remove_dir_all(worktree_base).expect("worktree base should be removed");
        },
    );
}

#[test]
fn host_environment_creates_worktree_parent_directory_when_absent() {
    with_fake_path_commands(
        &[
            (
                "calypso-feature-start-mkdir-gh",
                "gh",
                "#!/bin/sh\nif [ \"$1\" = \"api\" ] && echo \"$*\" | grep -q 'pulls'; then\n  printf '{\"number\":5,\"html_url\":\"https://github.com/dot-matrix-labs/calypso/pull/5\",\"url\":\"https://api.github.com/repos/dot-matrix-labs/calypso/pulls/5\"}'\n  exit 0\nfi\nprintf 'unexpected: %s\\n' \"$*\" >&2\nexit 1\n",
            ),
            (
                "calypso-feature-start-mkdir-git",
                "git",
                "#!/bin/sh\nif [ \"$1\" = \"push\" ]; then\n  exit 0\nfi\nexec /usr/bin/git \"$@\"\n",
            ),
        ],
        || {
            let (repo_root, bare_remote) = init_repo_with_remote();
            let base = unique_temp_dir("calypso-feature-start-mkdir");
            let worktree_base = base.join("nested").join("path");
            let request = FeatureStartRequest {
                feature_id: "Mkdir Test".to_string(),
                worktree_base: worktree_base.clone(),
                title: None,
                body: None,
                allow_dirty: false,
                allow_non_main: false,
            };

            let result = start_feature(&repo_root, &request, &HostFeatureStartEnvironment)
                .expect("feature start should create missing parent directories");

            assert!(result.worktree_path.exists());

            fs::remove_dir_all(repo_root).expect("repo root should be removed");
            fs::remove_dir_all(bare_remote).expect("bare remote should be removed");
            fs::remove_dir_all(base).expect("base dir should be removed");
        },
    );
}
