use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use calypso_cli::init::{InitEnvironment, InitError, InitRequest, init_repository};
use calypso_cli::state::RepositoryState;

// ── helpers ───────────────────────────────────────────────────────────────────

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_tmpdir(prefix: &str) -> PathBuf {
    let id = COUNTER.fetch_add(1, Ordering::SeqCst);
    let path = std::env::temp_dir().join(format!("calypso-init-test-{prefix}-{id}"));
    std::fs::create_dir_all(&path).expect("tmpdir creation");
    path
}

fn make_git_repo_with_github_remote(dir: &Path) {
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git command failed");
    };
    run(&["init"]);
    run(&["remote", "add", "origin", "https://github.com/org/repo.git"]);
}

fn make_git_repo_with_custom_remote(dir: &Path, remote_url: &str) {
    let run = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .expect("git command failed");
    };
    run(&["init"]);
    run(&["remote", "add", "origin", remote_url]);
}

// ── fake environment ──────────────────────────────────────────────────────────

#[derive(Debug, Default)]
struct FakeEnv {
    is_git: bool,
    remote_url: Option<String>,
    remote_error: Option<String>,
    default_branch: String,
    existing_paths: BTreeSet<PathBuf>,
    written_files: RefCell<BTreeMap<PathBuf, String>>,
    created_dirs: RefCell<Vec<PathBuf>>,
    executable_paths: RefCell<Vec<PathBuf>>,
    removed_dirs: RefCell<Vec<PathBuf>>,
    write_failure_path: Option<PathBuf>,
}

impl FakeEnv {
    fn with_github_remote(mut self) -> Self {
        self.is_git = true;
        self.remote_url = Some("https://github.com/org/repo.git".to_string());
        self.default_branch = "main".to_string();
        self
    }
}

impl InitEnvironment for FakeEnv {
    fn is_git_repo(&self, _path: &Path) -> Result<bool, InitError> {
        Ok(self.is_git)
    }

    fn remote_url(&self, _path: &Path) -> Result<String, InitError> {
        if let Some(ref err) = self.remote_error {
            return Err(InitError::GitCommandFailed {
                action: "git remote get-url origin".to_string(),
                details: err.clone(),
            });
        }
        Ok(self
            .remote_url
            .clone()
            .unwrap_or_else(|| "https://github.com/org/repo.git".to_string()))
    }

    fn default_branch(&self, _path: &Path) -> Result<String, InitError> {
        Ok(if self.default_branch.is_empty() {
            "main".to_string()
        } else {
            self.default_branch.clone()
        })
    }

    fn repo_name_from_url(&self, url: &str) -> Option<String> {
        // delegate to the same logic
        let trimmed = url.trim_end_matches(".git");
        trimmed.rsplit('/').next().map(|s| s.to_string())
    }

    fn path_exists(&self, path: &Path) -> bool {
        self.existing_paths.contains(path)
    }

    fn create_dir(&self, path: &Path) -> Result<(), InitError> {
        self.created_dirs.borrow_mut().push(path.to_path_buf());
        Ok(())
    }

    fn write_file(&self, path: &Path, contents: &str) -> Result<(), InitError> {
        if let Some(ref fail_path) = self.write_failure_path
            && path == fail_path.as_path()
        {
            return Err(InitError::GitCommandFailed {
                action: "write".to_string(),
                details: "simulated write failure".to_string(),
            });
        }
        self.written_files
            .borrow_mut()
            .insert(path.to_path_buf(), contents.to_string());
        Ok(())
    }

    fn set_executable(&self, path: &Path) -> Result<(), InitError> {
        self.executable_paths.borrow_mut().push(path.to_path_buf());
        Ok(())
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), InitError> {
        self.removed_dirs.borrow_mut().push(path.to_path_buf());
        Ok(())
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn init_creates_calypso_layout_and_state_file() {
    let env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/repo");
    let request = InitRequest {
        repo_path: repo_path.clone(),
        provider: None,
        allow_reinit: false,
    };

    let result = init_repository(&request, &env).expect("init should succeed");

    assert_eq!(result.calypso_dir, repo_path.join(".calypso"));
    assert_eq!(
        result.state_path,
        repo_path.join(".calypso/repository-state.json")
    );
    assert!(result.hooks_installed.contains(&"pre-push".to_string()));
    assert!(
        result
            .templates_written
            .contains(&"state-machine.yml".to_string())
    );
    assert!(result.templates_written.contains(&"agents.yml".to_string()));
    assert!(
        result
            .templates_written
            .contains(&"prompts.yml".to_string())
    );

    // state file was written
    let written = env.written_files.borrow();
    let state_contents = written
        .get(&repo_path.join(".calypso/repository-state.json"))
        .expect("state file should be written");
    let state: RepositoryState =
        serde_json::from_str(state_contents).expect("state should deserialize");
    assert_eq!(
        state.identity.github_remote_url,
        "https://github.com/org/repo.git"
    );
    assert_eq!(state.identity.default_branch, "main");
    assert_eq!(state.identity.name, "repo");
}

#[test]
fn init_on_non_git_directory_returns_error() {
    let env = FakeEnv {
        is_git: false,
        ..Default::default()
    };
    let request = InitRequest {
        repo_path: PathBuf::from("/not/a/repo"),
        provider: None,
        allow_reinit: false,
    };

    let err = init_repository(&request, &env).expect_err("should fail on non-git dir");
    assert!(
        matches!(err, InitError::NotAGitRepo { .. }),
        "expected NotAGitRepo, got: {err}"
    );
    assert!(err.to_string().contains("not a git repository"));
}

#[test]
fn init_on_non_github_remote_returns_error_with_remediation() {
    let env = FakeEnv {
        is_git: true,
        remote_url: Some("https://gitlab.com/org/repo.git".to_string()),
        default_branch: "main".to_string(),
        ..Default::default()
    };
    let request = InitRequest {
        repo_path: PathBuf::from("/fake/repo"),
        provider: None,
        allow_reinit: false,
    };

    let err = init_repository(&request, &env).expect_err("should fail for non-github remote");
    assert!(
        matches!(err, InitError::NotAGithubRemote { .. }),
        "expected NotAGithubRemote, got: {err}"
    );
    let msg = err.to_string();
    assert!(msg.contains("gitlab.com"));
    assert!(
        msg.contains("github.com"),
        "should mention github.com as remediation: {msg}"
    );
}

#[test]
fn reinit_without_flag_returns_error() {
    let mut env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/repo");
    env.existing_paths.insert(repo_path.join(".calypso"));

    let request = InitRequest {
        repo_path: repo_path.clone(),
        provider: None,
        allow_reinit: false,
    };

    let err = init_repository(&request, &env).expect_err("should fail on existing .calypso");
    assert!(
        matches!(err, InitError::AlreadyInitialized { .. }),
        "expected AlreadyInitialized: {err}"
    );
    assert!(err.to_string().contains("--allow-reinit"));
}

#[test]
fn reinit_with_flag_succeeds() {
    let mut env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/repo");
    env.existing_paths.insert(repo_path.join(".calypso"));

    let request = InitRequest {
        repo_path,
        provider: None,
        allow_reinit: true,
    };

    let result = init_repository(&request, &env).expect("reinit with flag should succeed");
    assert!(!result.state_path.to_string_lossy().is_empty());
}

#[test]
fn provider_flag_sets_provider_in_written_state() {
    let env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/repo");
    let request = InitRequest {
        repo_path: repo_path.clone(),
        provider: Some("claude".to_string()),
        allow_reinit: false,
    };

    init_repository(&request, &env).expect("init should succeed");

    let written = env.written_files.borrow();
    let state_contents = written
        .get(&repo_path.join(".calypso/repository-state.json"))
        .expect("state file should be written");
    let state: RepositoryState =
        serde_json::from_str(state_contents).expect("state should deserialize");
    assert!(state.providers.contains(&"claude".to_string()));
}

#[test]
fn pre_push_hook_file_is_created_and_marked_executable() {
    let env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/repo");
    let request = InitRequest {
        repo_path: repo_path.clone(),
        provider: None,
        allow_reinit: false,
    };

    init_repository(&request, &env).expect("init should succeed");

    let hook_path = repo_path.join(".git/hooks/pre-push");
    let written = env.written_files.borrow();
    assert!(
        written.contains_key(&hook_path),
        "pre-push hook file should be written"
    );
    let hook_contents = written.get(&hook_path).unwrap();
    assert!(hook_contents.contains("calypso-cli doctor"));

    let executables = env.executable_paths.borrow();
    assert!(
        executables.contains(&hook_path),
        "pre-push hook should be marked executable"
    );
}

#[test]
fn rollback_removes_calypso_dir_on_failure() {
    let repo_path = PathBuf::from("/fake/repo");
    // cause write_file to fail on the state file
    let env = FakeEnv {
        is_git: true,
        remote_url: Some("https://github.com/org/repo.git".to_string()),
        default_branch: "main".to_string(),
        write_failure_path: Some(repo_path.join(".calypso/repository-state.json")),
        ..Default::default()
    };

    let request = InitRequest {
        repo_path: repo_path.clone(),
        provider: None,
        allow_reinit: false,
    };

    let err = init_repository(&request, &env);
    assert!(err.is_err(), "should fail when state write fails");

    let removed = env.removed_dirs.borrow();
    assert!(
        removed.contains(&repo_path.join(".calypso")),
        "rollback should remove .calypso dir; removed: {removed:?}"
    );
}

// ── real filesystem integration tests ─────────────────────────────────────────

#[test]
fn real_init_creates_files_and_executable_hook() {
    let dir = unique_tmpdir("real-init");
    make_git_repo_with_github_remote(&dir);

    // Create a fake .git/hooks dir (git init does this, but let's be safe)
    // git init already creates it.

    use calypso_cli::init::{HostInitEnvironment, init_repository};
    let request = InitRequest {
        repo_path: dir.clone(),
        provider: Some("claude".to_string()),
        allow_reinit: false,
    };
    let result = init_repository(&request, &HostInitEnvironment).expect("real init should succeed");

    // .calypso/ exists
    assert!(dir.join(".calypso").is_dir());
    // state file exists and parses
    let state: RepositoryState = serde_json::from_str(
        &std::fs::read_to_string(&result.state_path).expect("state file should exist"),
    )
    .expect("state should parse");
    assert!(state.providers.contains(&"claude".to_string()));
    assert_eq!(
        state.identity.github_remote_url,
        "https://github.com/org/repo.git"
    );

    // templates exist
    assert!(dir.join(".calypso/state-machine.yml").exists());
    assert!(dir.join(".calypso/agents.yml").exists());
    assert!(dir.join(".calypso/prompts.yml").exists());

    // hook exists and is executable
    let hook_path = dir.join(".git/hooks/pre-push");
    assert!(hook_path.exists(), "pre-push hook should exist");
    let metadata = std::fs::metadata(&hook_path).expect("hook metadata");
    let mode = metadata.permissions().mode();
    assert!(mode & 0o111 != 0, "hook should be executable");

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn real_init_non_git_dir_returns_error() {
    let dir = unique_tmpdir("not-git");
    use calypso_cli::init::{HostInitEnvironment, init_repository};
    let request = InitRequest {
        repo_path: dir.clone(),
        provider: None,
        allow_reinit: false,
    };
    let err =
        init_repository(&request, &HostInitEnvironment).expect_err("should fail on non-git dir");
    assert!(matches!(err, InitError::NotAGitRepo { .. }));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn real_init_non_github_remote_returns_error() {
    let dir = unique_tmpdir("non-github");
    make_git_repo_with_custom_remote(&dir, "https://gitlab.com/org/repo.git");
    use calypso_cli::init::{HostInitEnvironment, init_repository};
    let request = InitRequest {
        repo_path: dir.clone(),
        provider: None,
        allow_reinit: false,
    };
    let err = init_repository(&request, &HostInitEnvironment)
        .expect_err("should fail for non-github remote");
    assert!(matches!(err, InitError::NotAGithubRemote { .. }));
    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn real_reinit_without_flag_fails_with_allow_reinit_succeeds() {
    let dir = unique_tmpdir("reinit");
    make_git_repo_with_github_remote(&dir);
    use calypso_cli::init::{HostInitEnvironment, init_repository};

    let request = InitRequest {
        repo_path: dir.clone(),
        provider: None,
        allow_reinit: false,
    };
    init_repository(&request, &HostInitEnvironment).expect("first init");

    // second init without flag should fail
    let err = init_repository(&request, &HostInitEnvironment)
        .expect_err("reinit without flag should fail");
    assert!(matches!(err, InitError::AlreadyInitialized { .. }));

    // with flag succeeds
    let request2 = InitRequest {
        repo_path: dir.clone(),
        provider: None,
        allow_reinit: true,
    };
    init_repository(&request2, &HostInitEnvironment).expect("reinit with flag should succeed");

    std::fs::remove_dir_all(&dir).ok();
}
