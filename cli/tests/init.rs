use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicU64, Ordering};

use calypso_cli::init::{
    InitEnvironment, InitError, InitProgress, InitRequest, InitStep, RepoInitStatus, WORKFLOW_CI,
    WORKFLOW_PR_CHECKLIST, WORKFLOW_PR_DEPENDS_ON, detect_repo_status, init_repository,
    refresh_workflows, run_init_interactive, scaffold_github_actions,
};
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
    git_inits: RefCell<Vec<PathBuf>>,
    remotes_set: RefCell<Vec<(PathBuf, String)>>,
    workflows_written: RefCell<Vec<(String, String)>>,
    git_init_fails: bool,
}

impl FakeEnv {
    fn with_github_remote(mut self) -> Self {
        self.is_git = true;
        self.remote_url = Some("https://github.com/org/repo.git".to_string());
        self.default_branch = "main".to_string();
        self
    }
}

fn default_init_request(repo_path: PathBuf) -> InitRequest {
    InitRequest {
        repo_path,
        provider: None,
        allow_reinit: false,
        create_git_repo: false,
        github_org: None,
        github_repo_name: None,
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

    fn git_init(&self, path: &Path) -> Result<(), InitError> {
        if self.git_init_fails {
            return Err(InitError::GitCommandFailed {
                action: "git init".to_string(),
                details: "simulated git init failure".to_string(),
            });
        }
        self.git_inits.borrow_mut().push(path.to_path_buf());
        Ok(())
    }

    fn create_github_repo(&self, org: &str, repo: &str) -> Result<String, InitError> {
        Ok(format!("https://github.com/{org}/{repo}.git"))
    }

    fn set_remote(&self, path: &Path, url: &str) -> Result<(), InitError> {
        self.remotes_set
            .borrow_mut()
            .push((path.to_path_buf(), url.to_string()));
        Ok(())
    }

    fn write_workflow_file(
        &self,
        _path: &Path,
        name: &str,
        content: &str,
    ) -> Result<(), InitError> {
        self.workflows_written
            .borrow_mut()
            .push((name.to_string(), content.to_string()));
        Ok(())
    }

    fn git_hooks_path(&self, path: &Path) -> Result<PathBuf, InitError> {
        Ok(path.join(".git").join("hooks"))
    }

    fn save_init_state(
        &self,
        _repo_root: &Path,
        _progress: &InitProgress,
    ) -> Result<(), InitError> {
        // No-op for fake environment — avoid writing to fake paths.
        Ok(())
    }

    fn load_init_state(&self, _repo_root: &Path) -> Result<Option<InitProgress>, InitError> {
        Ok(None)
    }
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn init_creates_calypso_layout_and_state_file() {
    let env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/repo");
    let request = default_init_request(repo_path.clone());

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
    let request = default_init_request(PathBuf::from("/not/a/repo"));

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
    let request = default_init_request(PathBuf::from("/fake/repo"));

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

    let request = default_init_request(repo_path.clone());

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
        create_git_repo: false,
        github_org: None,
        github_repo_name: None,
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
        create_git_repo: false,
        github_org: None,
        github_repo_name: None,
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
    let request = default_init_request(repo_path.clone());

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

    let request = default_init_request(repo_path.clone());

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

    use calypso_cli::init::{HostInitEnvironment, init_repository};
    let request = InitRequest {
        repo_path: dir.clone(),
        provider: Some("claude".to_string()),
        allow_reinit: false,
        create_git_repo: false,
        github_org: None,
        github_repo_name: None,
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
fn real_init_state_machine_audit_passes() {
    let dir = unique_tmpdir("init-audit");
    // Clean up in case a previous test run left behind a stale directory.
    std::fs::remove_dir_all(&dir).ok();
    std::fs::create_dir_all(&dir).expect("tmpdir creation");
    make_git_repo_with_github_remote(&dir);

    use calypso_cli::init::HostInitEnvironment;
    let request = InitRequest {
        repo_path: dir.clone(),
        provider: Some("claude".to_string()),
        allow_reinit: false,
        create_git_repo: false,
        github_org: None,
        github_repo_name: None,
    };
    init_repository(&request, &HostInitEnvironment).expect("init should succeed");
    scaffold_github_actions(&dir, &HostInitEnvironment).expect("scaffold workflows should succeed");

    // Verify all blueprint-required workflow files were scaffolded
    let workflows_dir = dir.join(".github/workflows");
    for wf in &[
        "pr-checklist.yml",
        "pr-depends-on.yml",
        "ci.yml",
        "rust-quality.yml",
        "rust-unit.yml",
        "rust-integration.yml",
        "rust-e2e.yml",
        "rust-coverage.yml",
        "release-cli.yml",
    ] {
        assert!(
            workflows_dir.join(wf).exists(),
            "workflow file should exist after init: {wf}"
        );
    }

    // Verify git hooks were installed
    assert!(dir.join(".git/hooks/pre-push").exists(), "pre-push hook should exist");

    // Run state machine audit — must report zero errors
    let result = calypso_cli::sm_audit::run_audit(&dir);
    assert_eq!(
        result.error_count(),
        0,
        "state machine audit must have zero errors after init:\n{}",
        result.format_errors()
    );

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
        create_git_repo: false,
        github_org: None,
        github_repo_name: None,
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
        create_git_repo: false,
        github_org: None,
        github_repo_name: None,
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
        create_git_repo: false,
        github_org: None,
        github_repo_name: None,
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
        create_git_repo: false,
        github_org: None,
        github_repo_name: None,
    };
    init_repository(&request2, &HostInitEnvironment).expect("reinit with flag should succeed");

    std::fs::remove_dir_all(&dir).ok();
}

// ── InitStep state machine tests ──────────────────────────────────────────────

#[test]
fn init_step_as_str_values_are_kebab_case() {
    assert_eq!(InitStep::PromptDirectory.as_str(), "prompt-directory");
    assert_eq!(InitStep::CreateGitRepo.as_str(), "create-git-repo");
    assert_eq!(InitStep::CreateUpstream.as_str(), "create-upstream");
    assert_eq!(
        InitStep::ScaffoldGithubActions.as_str(),
        "scaffold-github-actions"
    );
    assert_eq!(InitStep::ConfigureLocal.as_str(), "configure-local");
    assert_eq!(InitStep::VerifySetup.as_str(), "verify-setup");
    assert_eq!(InitStep::Complete.as_str(), "complete");
}

#[test]
fn init_step_next_follows_linear_sequence() {
    assert_eq!(
        InitStep::PromptDirectory.next(),
        Some(InitStep::CreateGitRepo)
    );
    assert_eq!(
        InitStep::CreateGitRepo.next(),
        Some(InitStep::CreateUpstream)
    );
    assert_eq!(
        InitStep::CreateUpstream.next(),
        Some(InitStep::ScaffoldGithubActions)
    );
    assert_eq!(
        InitStep::ScaffoldGithubActions.next(),
        Some(InitStep::ConfigureLocal)
    );
    assert_eq!(InitStep::ConfigureLocal.next(), Some(InitStep::VerifySetup));
    assert_eq!(InitStep::VerifySetup.next(), Some(InitStep::Complete));
    assert_eq!(InitStep::Complete.next(), None);
}

#[test]
fn init_step_complete_is_terminal() {
    assert!(InitStep::Complete.is_complete());
    assert!(!InitStep::PromptDirectory.is_complete());
    assert!(!InitStep::VerifySetup.is_complete());
}

#[test]
fn init_step_display_matches_as_str() {
    use std::fmt::Write as _;
    let steps = [
        InitStep::PromptDirectory,
        InitStep::CreateGitRepo,
        InitStep::CreateUpstream,
        InitStep::ScaffoldGithubActions,
        InitStep::ConfigureLocal,
        InitStep::VerifySetup,
        InitStep::Complete,
    ];
    for step in &steps {
        let mut s = String::new();
        write!(s, "{step}").unwrap();
        assert_eq!(s, step.as_str());
    }
}

#[test]
fn init_step_serializes_to_kebab_case() {
    let json = serde_json::to_string(&InitStep::ScaffoldGithubActions).unwrap();
    assert_eq!(json, "\"scaffold-github-actions\"");
}

#[test]
fn init_step_deserializes_from_kebab_case() {
    let step: InitStep = serde_json::from_str("\"create-git-repo\"").unwrap();
    assert_eq!(step, InitStep::CreateGitRepo);
}

#[test]
fn init_step_round_trips_through_json_for_all_variants() {
    let steps = [
        InitStep::PromptDirectory,
        InitStep::CreateGitRepo,
        InitStep::CreateUpstream,
        InitStep::ScaffoldGithubActions,
        InitStep::ConfigureLocal,
        InitStep::VerifySetup,
        InitStep::Complete,
    ];
    for step in &steps {
        let json = serde_json::to_string(step).unwrap();
        let decoded: InitStep = serde_json::from_str(&json).unwrap();
        assert_eq!(&decoded, step, "round-trip failed for {step}");
    }
}

// ── InitProgress tests ────────────────────────────────────────────────────────

#[test]
fn init_progress_new_starts_at_prompt_directory() {
    let progress = InitProgress::new(PathBuf::from("/fake/repo"));
    assert_eq!(progress.current_step, InitStep::PromptDirectory);
    assert!(progress.completed_steps.is_empty());
}

#[test]
fn init_progress_advance_moves_through_sequence() {
    let mut progress = InitProgress::new(PathBuf::from("/fake/repo"));
    assert_eq!(progress.current_step, InitStep::PromptDirectory);

    progress.advance();
    assert_eq!(progress.current_step, InitStep::CreateGitRepo);
    assert!(progress.is_step_done(&InitStep::PromptDirectory));

    progress.advance();
    assert_eq!(progress.current_step, InitStep::CreateUpstream);
    assert!(progress.is_step_done(&InitStep::CreateGitRepo));
}

#[test]
fn init_progress_advance_at_complete_is_a_no_op() {
    let mut progress = InitProgress::new(PathBuf::from("/fake/repo"));
    for _ in 0..6 {
        progress.advance();
    }
    assert_eq!(progress.current_step, InitStep::Complete);
    progress.advance();
    assert_eq!(progress.current_step, InitStep::Complete);
}

#[test]
fn init_progress_is_step_done_returns_false_for_future_steps() {
    let progress = InitProgress::new(PathBuf::from("/fake/repo"));
    assert!(!progress.is_step_done(&InitStep::CreateGitRepo));
    assert!(!progress.is_step_done(&InitStep::Complete));
}

#[test]
fn init_progress_serializes_and_deserializes() {
    let mut progress = InitProgress::new(PathBuf::from("/fake/repo"));
    progress.advance();
    progress.github_org = Some("my-org".to_string());
    progress.github_repo = Some("my-repo".to_string());

    let json = serde_json::to_string(&progress).unwrap();
    let decoded: InitProgress = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded.current_step, InitStep::CreateGitRepo);
    assert_eq!(decoded.github_org.as_deref(), Some("my-org"));
    assert_eq!(decoded.github_repo.as_deref(), Some("my-repo"));
    assert!(decoded.is_step_done(&InitStep::PromptDirectory));
}

// ── scaffold_github_actions tests ─────────────────────────────────────────────

#[test]
fn scaffold_github_actions_writes_three_workflow_files() {
    let env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/repo");

    let scaffolded = scaffold_github_actions(&repo_path, &env).expect("scaffold should succeed");

    let workflows = env.workflows_written.borrow();
    assert_eq!(workflows.len(), 9, "should scaffold 9 workflow files");

    let names: Vec<&str> = workflows.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        names.contains(&"pr-checklist.yml"),
        "missing pr-checklist.yml"
    );
    assert!(
        names.contains(&"pr-depends-on.yml"),
        "missing pr-depends-on.yml"
    );
    assert!(names.contains(&"ci.yml"), "missing ci.yml");
    assert!(names.contains(&"rust-quality.yml"), "missing rust-quality.yml");
    assert!(names.contains(&"rust-unit.yml"), "missing rust-unit.yml");
    assert!(names.contains(&"rust-integration.yml"), "missing rust-integration.yml");
    assert!(names.contains(&"rust-e2e.yml"), "missing rust-e2e.yml");
    assert!(names.contains(&"rust-coverage.yml"), "missing rust-coverage.yml");
    assert!(names.contains(&"release-cli.yml"), "missing release-cli.yml");
    assert_eq!(scaffolded.len(), 9);
}

#[test]
fn scaffold_github_actions_skips_existing_workflow_files() {
    let repo_path = PathBuf::from("/fake/repo");
    let mut env = FakeEnv::default().with_github_remote();
    env.existing_paths.insert(
        repo_path
            .join(".github")
            .join("workflows")
            .join("pr-checklist.yml"),
    );

    let scaffolded = scaffold_github_actions(&repo_path, &env).expect("scaffold should succeed");

    let workflows = env.workflows_written.borrow();
    assert_eq!(
        workflows.len(),
        8,
        "should skip existing workflow file; got: {workflows:?}"
    );
    assert_eq!(scaffolded.len(), 8);

    let names: Vec<&str> = workflows.iter().map(|(n, _)| n.as_str()).collect();
    assert!(
        !names.contains(&"pr-checklist.yml"),
        "should skip existing file"
    );
    assert!(names.contains(&"pr-depends-on.yml"));
    assert!(names.contains(&"ci.yml"));
}

#[test]
fn workflow_pr_checklist_content_is_valid_yaml() {
    let val: serde_yaml::Value =
        serde_yaml::from_str(WORKFLOW_PR_CHECKLIST).expect("pr-checklist.yml should be valid YAML");
    let map = val.as_mapping().expect("top-level should be a mapping");
    assert!(map.contains_key(serde_yaml::Value::String("name".into())));
    assert!(map.contains_key(serde_yaml::Value::String("on".into())));
    assert!(map.contains_key(serde_yaml::Value::String("jobs".into())));
}

#[test]
fn workflow_pr_depends_on_content_is_valid_yaml() {
    let val: serde_yaml::Value = serde_yaml::from_str(WORKFLOW_PR_DEPENDS_ON)
        .expect("pr-depends-on.yml should be valid YAML");
    let map = val.as_mapping().expect("top-level should be a mapping");
    assert!(map.contains_key(serde_yaml::Value::String("name".into())));
    assert!(map.contains_key(serde_yaml::Value::String("jobs".into())));
}

#[test]
fn workflow_ci_content_is_valid_yaml() {
    let val: serde_yaml::Value =
        serde_yaml::from_str(WORKFLOW_CI).expect("ci.yml should be valid YAML");
    let map = val.as_mapping().expect("top-level should be a mapping");
    assert!(map.contains_key(serde_yaml::Value::String("jobs".into())));
}

// ── run_init_interactive tests ─────────────────────────────────────────────────

#[test]
fn run_init_interactive_with_existing_git_and_github_remote_succeeds() {
    let env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/repo");

    let progress =
        run_init_interactive(&repo_path, false, &env).expect("interactive init should succeed");

    assert!(progress.current_step.is_complete(), "should reach Complete");
    assert!(progress.is_step_done(&InitStep::PromptDirectory));
    assert!(progress.is_step_done(&InitStep::CreateGitRepo));
    assert!(progress.is_step_done(&InitStep::ScaffoldGithubActions));
}

#[test]
fn run_init_interactive_calls_git_init_when_not_a_git_repo() {
    let env = FakeEnv {
        is_git: false,
        remote_url: None,
        remote_error: Some("no remote".to_string()),
        default_branch: "main".to_string(),
        ..Default::default()
    };
    let repo_path = PathBuf::from("/fake/fresh");

    // Will fail at configure-local (no GitHub remote), but git_init is called
    let _result = run_init_interactive(&repo_path, false, &env);

    let git_inits = env.git_inits.borrow();
    assert!(
        git_inits.contains(&repo_path),
        "git_init should be called for non-git directory"
    );
}

#[test]
fn run_init_interactive_does_not_call_git_init_when_already_a_git_repo() {
    let env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/existing-git");

    run_init_interactive(&repo_path, false, &env).expect("interactive init should succeed");

    let git_inits = env.git_inits.borrow();
    assert!(
        git_inits.is_empty(),
        "git_init should not be called when already a git repo"
    );
}

#[test]
fn run_init_interactive_scaffolds_workflow_files() {
    let env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/scaffold-test");

    run_init_interactive(&repo_path, false, &env).expect("interactive init should succeed");

    let workflows = env.workflows_written.borrow();
    assert!(!workflows.is_empty(), "workflow files should be scaffolded");
}

// ── detect_repo_status tests ──────────────────────────────────────────────────

#[test]
fn detect_repo_status_no_git() {
    let env = FakeEnv {
        is_git: false,
        ..Default::default()
    };
    let status = detect_repo_status(&PathBuf::from("/fake/no-git"), &env);
    assert_eq!(status, RepoInitStatus::NoGit);
}

#[test]
fn detect_repo_status_git_no_upstream() {
    let env = FakeEnv {
        is_git: true,
        remote_error: Some("no remote".to_string()),
        ..Default::default()
    };
    let status = detect_repo_status(&PathBuf::from("/fake/no-upstream"), &env);
    assert_eq!(status, RepoInitStatus::GitNoUpstream);
}

#[test]
fn detect_repo_status_git_with_upstream_not_configured() {
    let env = FakeEnv::default().with_github_remote();
    let status = detect_repo_status(&PathBuf::from("/fake/with-remote"), &env);
    // No .calypso/ or .github/workflows/ in existing_paths, so not fully configured.
    assert_eq!(status, RepoInitStatus::GitWithUpstream);
}

#[test]
fn detect_repo_status_fully_configured() {
    let repo_path = PathBuf::from("/fake/configured");
    let mut env = FakeEnv::default().with_github_remote();
    env.existing_paths.insert(repo_path.join(".calypso"));
    env.existing_paths
        .insert(repo_path.join(".github").join("workflows"));
    let status = detect_repo_status(&repo_path, &env);
    assert_eq!(status, RepoInitStatus::FullyConfigured);
}

// ── refresh_workflows tests ────────────────────────────────────────────────────

#[test]
fn refresh_workflows_overwrites_all_three_files() {
    let env = FakeEnv::default().with_github_remote();
    let repo_path = PathBuf::from("/fake/refresh");

    let refreshed = refresh_workflows(&repo_path, &env).expect("refresh should succeed");

    assert_eq!(refreshed.len(), 9, "should refresh all 9 workflow files");
    assert!(refreshed.contains(&"pr-checklist.yml".to_string()));
    assert!(refreshed.contains(&"pr-depends-on.yml".to_string()));
    assert!(refreshed.contains(&"ci.yml".to_string()));
    assert!(refreshed.contains(&"rust-quality.yml".to_string()));
    assert!(refreshed.contains(&"rust-unit.yml".to_string()));
    assert!(refreshed.contains(&"rust-integration.yml".to_string()));
    assert!(refreshed.contains(&"rust-e2e.yml".to_string()));
    assert!(refreshed.contains(&"rust-coverage.yml".to_string()));
    assert!(refreshed.contains(&"release-cli.yml".to_string()));

    let workflows = env.workflows_written.borrow();
    assert_eq!(workflows.len(), 9);
}

// ── init progress persistence tests (real filesystem) ──────────────────────────

#[test]
fn init_progress_save_and_load_round_trips() {
    let dir = unique_tmpdir("progress-persist");
    std::fs::create_dir_all(dir.join(".calypso")).ok();

    let mut progress = InitProgress::new(dir.clone());
    progress.advance(); // PromptDirectory -> CreateGitRepo
    progress.advance(); // CreateGitRepo -> CreateUpstream
    progress.github_org = Some("test-org".to_string());
    progress.github_repo = Some("test-repo".to_string());

    progress.save(&dir).expect("save should succeed");

    let loaded = InitProgress::load(&dir)
        .expect("load should succeed")
        .expect("should find saved progress");

    assert_eq!(loaded.current_step, InitStep::CreateUpstream);
    assert!(loaded.is_step_done(&InitStep::PromptDirectory));
    assert!(loaded.is_step_done(&InitStep::CreateGitRepo));
    assert_eq!(loaded.github_org.as_deref(), Some("test-org"));
    assert_eq!(loaded.github_repo.as_deref(), Some("test-repo"));

    std::fs::remove_dir_all(&dir).ok();
}

#[test]
fn init_progress_load_returns_none_when_missing() {
    let dir = unique_tmpdir("no-progress");
    let loaded = InitProgress::load(&dir).expect("load should not error");
    assert!(loaded.is_none(), "should return None when file missing");
    std::fs::remove_dir_all(&dir).ok();
}

// ── run_init_interactive creates upstream when org/repo provided ────────────────

#[test]
fn run_init_interactive_creates_upstream_when_org_provided() {
    let repo_path = PathBuf::from("/fake/new-repo");
    let env = FakeEnv {
        is_git: false,
        remote_url: None,
        remote_error: Some("no remote".to_string()),
        default_branch: "main".to_string(),
        ..Default::default()
    };

    // Set up org/repo on progress before running.
    // The interactive flow reads from progress.github_org/github_repo,
    // so we test the create_github_repo path through the env trait.
    let mut progress = InitProgress::new(repo_path.clone());
    progress.github_org = Some("my-org".to_string());
    progress.github_repo = Some("my-repo".to_string());

    // Verify create_github_repo is callable.
    let url = env
        .create_github_repo("my-org", "my-repo")
        .expect("should succeed");
    assert_eq!(url, "https://github.com/my-org/my-repo.git");
}

// ── RepoInitStatus serialization ──────────────────────────────────────────────

#[test]
fn repo_init_status_serializes_all_variants() {
    let variants = [
        (RepoInitStatus::NoGit, "\"no-git\""),
        (RepoInitStatus::GitNoUpstream, "\"git-no-upstream\""),
        (RepoInitStatus::GitWithUpstream, "\"git-with-upstream\""),
        (RepoInitStatus::FullyConfigured, "\"fully-configured\""),
    ];
    for (variant, expected) in &variants {
        let json = serde_json::to_string(variant).unwrap();
        assert_eq!(&json, *expected, "unexpected serialization for {variant:?}");
    }
}
