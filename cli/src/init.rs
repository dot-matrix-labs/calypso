use std::fmt;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::state::{RepositoryIdentity, RepositoryState};
use crate::template::{DEFAULT_AGENTS_YAML, DEFAULT_PROMPTS_YAML, DEFAULT_STATE_MACHINE_YAML};

pub struct InitRequest {
    pub repo_path: PathBuf,
    pub provider: Option<String>,
    pub allow_reinit: bool,
}

#[derive(Debug)]
pub struct InitResult {
    pub calypso_dir: PathBuf,
    pub state_path: PathBuf,
    pub hooks_installed: Vec<String>,
    pub templates_written: Vec<String>,
}

#[derive(Debug)]
pub enum InitError {
    Io(std::io::Error),
    NotAGitRepo { path: PathBuf },
    NotAGithubRemote { url: String },
    GitCommandFailed { action: String, details: String },
    AlreadyInitialized { calypso_dir: PathBuf },
    StateSerialize(serde_json::Error),
}

impl InitError {
    fn git(action: &str, details: &str) -> Self {
        Self::GitCommandFailed {
            action: action.to_string(),
            details: details.to_string(),
        }
    }
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "init I/O error: {e}"),
            Self::NotAGitRepo { path } => write!(
                f,
                "path '{}' is not a git repository; run `git init` first",
                path.display()
            ),
            Self::NotAGithubRemote { url } => write!(
                f,
                "remote URL '{url}' is not a GitHub URL; calypso requires a GitHub remote — \
                 update the origin remote to a github.com URL and retry"
            ),
            Self::GitCommandFailed { action, details } => {
                write!(f, "{action} failed: {details}")
            }
            Self::AlreadyInitialized { calypso_dir } => write!(
                f,
                "'.calypso/' already exists at '{}'; pass --allow-reinit to re-initialise",
                calypso_dir.display()
            ),
            Self::StateSerialize(e) => write!(f, "failed to serialise repository state: {e}"),
        }
    }
}

impl std::error::Error for InitError {}

pub trait InitEnvironment {
    fn is_git_repo(&self, path: &Path) -> Result<bool, InitError>;
    fn remote_url(&self, path: &Path) -> Result<String, InitError>;
    fn default_branch(&self, path: &Path) -> Result<String, InitError>;
    fn repo_name_from_url(&self, url: &str) -> Option<String>;
    fn path_exists(&self, path: &Path) -> bool;
    fn create_dir(&self, path: &Path) -> Result<(), InitError>;
    fn write_file(&self, path: &Path, contents: &str) -> Result<(), InitError>;
    fn set_executable(&self, path: &Path) -> Result<(), InitError>;
    fn remove_dir_all(&self, path: &Path) -> Result<(), InitError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HostInitEnvironment;

impl InitEnvironment for HostInitEnvironment {
    fn is_git_repo(&self, path: &Path) -> Result<bool, InitError> {
        let output = Command::new("git")
            .args(["-C", &path.to_string_lossy(), "rev-parse", "--git-dir"])
            .output()
            .map_err(InitError::Io)?;
        Ok(output.status.success())
    }

    fn remote_url(&self, path: &Path) -> Result<String, InitError> {
        let output = Command::new("git")
            .args(["-C", &path.to_string_lossy(), "remote", "get-url", "origin"])
            .output()
            .map_err(InitError::Io)?;
        if !output.status.success() {
            return Err(InitError::git(
                "git remote get-url origin",
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn default_branch(&self, path: &Path) -> Result<String, InitError> {
        let output = Command::new("git")
            .args([
                "-C",
                &path.to_string_lossy(),
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
            ])
            .output()
            .map_err(InitError::Io)?;
        if !output.status.success() {
            // fall back to "main" if the symbolic ref is not set
            return Ok("main".to_string());
        }
        let full = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // full looks like "refs/remotes/origin/main" — extract last segment
        Ok(full.split('/').next_back().unwrap_or("main").to_string())
    }

    fn repo_name_from_url(&self, url: &str) -> Option<String> {
        extract_repo_name(url)
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_dir(&self, path: &Path) -> Result<(), InitError> {
        fs::create_dir_all(path).map_err(InitError::Io)
    }

    fn write_file(&self, path: &Path, contents: &str) -> Result<(), InitError> {
        fs::write(path, contents).map_err(InitError::Io)
    }

    fn set_executable(&self, path: &Path) -> Result<(), InitError> {
        let mut perms = fs::metadata(path).map_err(InitError::Io)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).map_err(InitError::Io)
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), InitError> {
        fs::remove_dir_all(path).map_err(InitError::Io)
    }
}

fn extract_repo_name(url: &str) -> Option<String> {
    // Handles:
    //   https://github.com/org/repo.git
    //   git@github.com:org/repo.git
    //   https://github.com/org/repo
    let trimmed = url.trim_end_matches(".git");
    let after_slash = trimmed.rsplit('/').next()?;
    let after_colon = trimmed.rsplit(':').next()?;
    // Take the shorter non-empty segment that looks like a repo name (no dots)
    // Actually just take the last path component after the last slash or colon.
    // The last '/' already gives us the repo name.
    if !after_slash.is_empty() && !after_slash.contains('.') {
        Some(after_slash.to_string())
    } else if !after_colon.is_empty() {
        // git@github.com:org/repo — last component after last '/'
        let last = after_colon.split('/').next_back()?;
        Some(last.trim_end_matches(".git").to_string())
    } else {
        None
    }
}

fn is_github_url(url: &str) -> bool {
    url.contains("github.com")
}

const PRE_PUSH_HOOK: &str = "\
#!/bin/sh
# Calypso pre-push hook — run doctor non-blocking (warn but do not fail)
if command -v calypso-cli > /dev/null 2>&1; then
    calypso-cli doctor || true
fi
";

pub fn run_init(request: &InitRequest) -> Result<InitResult, InitError> {
    init_repository(request, &HostInitEnvironment)
}

pub fn init_repository(
    request: &InitRequest,
    env: &impl InitEnvironment,
) -> Result<InitResult, InitError> {
    // Step 1: validate git repo
    if !env.is_git_repo(&request.repo_path)? {
        return Err(InitError::NotAGitRepo {
            path: request.repo_path.clone(),
        });
    }

    // Step 2: detect GitHub remote
    let remote_url = env.remote_url(&request.repo_path)?;
    if !is_github_url(&remote_url) {
        return Err(InitError::NotAGithubRemote { url: remote_url });
    }

    // Step 3: detect default branch
    let default_branch = env.default_branch(&request.repo_path)?;

    // Derive repo name
    let repo_name = env
        .repo_name_from_url(&remote_url)
        .unwrap_or_else(|| "unknown".to_string());

    let calypso_dir = request.repo_path.join(".calypso");

    // Step 4: check for existing .calypso/
    if env.path_exists(&calypso_dir) && !request.allow_reinit {
        return Err(InitError::AlreadyInitialized {
            calypso_dir: calypso_dir.clone(),
        });
    }

    // Step 4 cont: create directory
    env.create_dir(&calypso_dir)?;

    // From here, rollback on failure
    let result = do_init_steps(
        request,
        env,
        &calypso_dir,
        &repo_name,
        &remote_url,
        &default_branch,
    );

    if result.is_err() {
        let _ = env.remove_dir_all(&calypso_dir);
    }

    result
}

fn do_init_steps(
    request: &InitRequest,
    env: &impl InitEnvironment,
    calypso_dir: &Path,
    repo_name: &str,
    remote_url: &str,
    default_branch: &str,
) -> Result<InitResult, InitError> {
    let state_path = calypso_dir.join("repository-state.json");

    // Step 5: write initial RepositoryState
    let identity = RepositoryIdentity {
        name: repo_name.to_string(),
        github_remote_url: remote_url.to_string(),
        default_branch: default_branch.to_string(),
    };

    let providers = if let Some(ref provider) = request.provider {
        vec![provider.clone()]
    } else {
        vec![]
    };

    let state = RepositoryState {
        version: 1,
        repo_id: repo_name.to_string(),
        schema_version: 1,
        identity,
        providers,
        github_auth_ref: None,
        secure_key_refs: vec![],
        active_features: vec![],
        known_worktrees: vec![],
        current_feature: default_feature_state(),
        releases: vec![],
        deployments: vec![],
    };

    let json = serde_json::to_string_pretty(&state).map_err(InitError::StateSerialize)?;
    env.write_file(&state_path, &json)?;

    // Step 6: copy default template files
    let mut templates_written = Vec::new();

    let sm_path = calypso_dir.join("state-machine.yml");
    env.write_file(&sm_path, DEFAULT_STATE_MACHINE_YAML)?;
    templates_written.push("state-machine.yml".to_string());

    let agents_path = calypso_dir.join("agents.yml");
    env.write_file(&agents_path, DEFAULT_AGENTS_YAML)?;
    templates_written.push("agents.yml".to_string());

    let prompts_path = calypso_dir.join("prompts.yml");
    env.write_file(&prompts_path, DEFAULT_PROMPTS_YAML)?;
    templates_written.push("prompts.yml".to_string());

    // Step 7: install git hook
    let hooks_dir = request.repo_path.join(".git").join("hooks");
    env.create_dir(&hooks_dir)?;
    let hook_path = hooks_dir.join("pre-push");
    env.write_file(&hook_path, PRE_PUSH_HOOK)?;
    env.set_executable(&hook_path)?;
    let hooks_installed = vec!["pre-push".to_string()];

    Ok(InitResult {
        calypso_dir: calypso_dir.to_path_buf(),
        state_path,
        hooks_installed,
        templates_written,
    })
}

fn default_feature_state() -> crate::state::FeatureState {
    use crate::state::{FeatureState, FeatureType, PullRequestRef, SchedulingMeta, WorkflowState};

    FeatureState {
        feature_id: String::new(),
        branch: String::new(),
        worktree_path: String::new(),
        pull_request: PullRequestRef {
            number: 0,
            url: String::new(),
        },
        github_snapshot: None,
        github_error: None,
        workflow_state: WorkflowState::New,
        gate_groups: vec![],
        active_sessions: vec![],
        feature_type: FeatureType::Feat,
        roles: vec![],
        scheduling: SchedulingMeta::default(),
        artifact_refs: vec![],
        transcript_refs: vec![],
        clarification_history: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extract_repo_name_https() {
        assert_eq!(
            extract_repo_name("https://github.com/org/myrepo.git"),
            Some("myrepo".to_string())
        );
    }

    #[test]
    fn extract_repo_name_ssh() {
        assert_eq!(
            extract_repo_name("git@github.com:org/myrepo.git"),
            Some("myrepo".to_string())
        );
    }

    #[test]
    fn extract_repo_name_no_git_suffix() {
        assert_eq!(
            extract_repo_name("https://github.com/org/myrepo"),
            Some("myrepo".to_string())
        );
    }

    #[test]
    fn is_github_url_recognizes_github() {
        assert!(is_github_url("https://github.com/org/repo.git"));
        assert!(is_github_url("git@github.com:org/repo.git"));
    }

    #[test]
    fn is_github_url_rejects_other() {
        assert!(!is_github_url("https://gitlab.com/org/repo.git"));
        assert!(!is_github_url("https://bitbucket.org/org/repo.git"));
    }
}
