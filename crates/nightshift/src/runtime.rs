use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::state::{
    FeatureState, GateInitializationError, PullRequestRef, RepositoryState, StateError,
};
use crate::template::{TemplateError, TemplateSet, resolve_template_set_for_path};

const STATE_DIRECTORY: &str = ".calypso";
const STATE_FILE_NAME: &str = "repository-state.json";
const STATE_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureBinding {
    pub feature_id: String,
    pub branch: String,
    pub worktree_path: String,
    pub pull_request: PullRequestRef,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepositoryContext {
    pub repo_root: PathBuf,
    pub repo_id: String,
    pub branch: String,
    pub worktree_path: PathBuf,
    pub feature: FeatureBinding,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeState {
    pub context: RepositoryContext,
    pub template: TemplateSet,
    pub state: RepositoryState,
    pub state_path: PathBuf,
}

impl RuntimeState {
    pub fn save(&self) -> Result<(), RuntimeError> {
        if let Some(parent) = self.state_path.parent() {
            fs::create_dir_all(parent).map_err(RuntimeError::Io)?;
        }

        self.state
            .save_to_path(&self.state_path)
            .map_err(RuntimeError::State)
    }
}

pub trait PullRequestResolver {
    fn resolve_for_branch(
        &self,
        repo_root: &Path,
        branch: &str,
    ) -> Result<PullRequestRef, RuntimeError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct GhCliPullRequestResolver;

impl PullRequestResolver for GhCliPullRequestResolver {
    fn resolve_for_branch(
        &self,
        repo_root: &Path,
        branch: &str,
    ) -> Result<PullRequestRef, RuntimeError> {
        let (owner, repo_name) = crate::github::resolve_owner_repo(repo_root).map_err(|e| {
            RuntimeError::CommandFailed {
                program: "git".to_string(),
                details: e.to_string(),
            }
        })?;

        let endpoint =
            format!("repos/{owner}/{repo_name}/pulls?head={owner}:{branch}&per_page=1&state=open");
        let output = Command::new("gh")
            .args(["api", &endpoint])
            .current_dir(repo_root)
            .output()
            .map_err(RuntimeError::Io)?;

        if !output.status.success() {
            return Err(RuntimeError::CommandFailed {
                program: "gh".to_string(),
                details: String::from_utf8_lossy(&output.stderr).trim().to_string(),
            });
        }

        let pull_requests: Vec<GhPullRequestSummary> =
            serde_json::from_slice(&output.stdout).map_err(RuntimeError::Json)?;
        let pull_request = pull_requests
            .into_iter()
            .next()
            .ok_or_else(|| RuntimeError::PullRequestNotFound(branch.to_string()))?;

        Ok(PullRequestRef {
            number: pull_request.number,
            url: pull_request.browser_url().to_string(),
        })
    }
}

pub fn discover_current_repository_context(
    start_path: &Path,
) -> Result<RepositoryContext, RuntimeError> {
    discover_repository_context(start_path, &GhCliPullRequestResolver)
}

pub fn discover_repository_context(
    start_path: &Path,
    pull_request_resolver: &impl PullRequestResolver,
) -> Result<RepositoryContext, RuntimeError> {
    let repo_root = git_output(start_path, &["rev-parse", "--show-toplevel"])?;
    let repo_root = PathBuf::from(repo_root)
        .canonicalize()
        .map_err(RuntimeError::Io)?;
    let branch = git_output(&repo_root, &["branch", "--show-current"])?;

    if branch.is_empty() {
        return Err(RuntimeError::DetachedHead);
    }

    let pull_request = pull_request_resolver.resolve_for_branch(&repo_root, &branch)?;
    let repo_id = repo_root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .ok_or(RuntimeError::MissingRepositoryName)?;
    let worktree_path = repo_root.clone();

    Ok(RepositoryContext {
        repo_root: repo_root.clone(),
        repo_id,
        branch: branch.clone(),
        worktree_path: worktree_path.clone(),
        feature: FeatureBinding {
            feature_id: branch.clone(),
            branch,
            worktree_path: worktree_path.display().to_string(),
            pull_request,
        },
    })
}

pub fn load_or_initialize_current_runtime(start_path: &Path) -> Result<RuntimeState, RuntimeError> {
    load_or_initialize_runtime(start_path, &GhCliPullRequestResolver)
}

pub fn load_or_initialize_runtime(
    start_path: &Path,
    pull_request_resolver: &impl PullRequestResolver,
) -> Result<RuntimeState, RuntimeError> {
    let context = discover_repository_context(start_path, pull_request_resolver)?;
    let template =
        resolve_template_set_for_path(&context.repo_root).map_err(RuntimeError::Template)?;
    let state_path = state_file_path(&context.repo_root);

    let state = if state_path.exists() {
        let state = RepositoryState::load_from_path(&state_path).map_err(RuntimeError::State)?;

        if state.current_feature.branch != context.branch {
            return Err(RuntimeError::StateBranchMismatch {
                expected: context.branch.clone(),
                actual: state.current_feature.branch,
            });
        }

        state
    } else {
        let feature = FeatureState::from_template(
            &context.feature.feature_id,
            &context.feature.branch,
            &context.feature.worktree_path,
            context.feature.pull_request.clone(),
            &template,
        )
        .map_err(RuntimeError::GateInitialization)?;

        let state = RepositoryState {
            version: STATE_VERSION,
            schema_version: 2,
            repo_id: context.repo_id.clone(),
            current_feature: feature,
            identity: Default::default(),
            providers: Vec::new(),
            releases: Vec::new(),
            deployments: Vec::new(),
        };

        let runtime = RuntimeState {
            context,
            template,
            state,
            state_path,
        };
        runtime.save()?;
        return Ok(runtime);
    };

    Ok(RuntimeState {
        context,
        template,
        state,
        state_path,
    })
}

fn state_file_path(repo_root: &Path) -> PathBuf {
    repo_root.join(STATE_DIRECTORY).join(STATE_FILE_NAME)
}

fn git_output(current_dir: &Path, args: &[&str]) -> Result<String, RuntimeError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(current_dir)
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .map_err(RuntimeError::Io)?;

    if !output.status.success() {
        return Err(RuntimeError::CommandFailed {
            program: "git".to_string(),
            details: String::from_utf8_lossy(&output.stderr).trim().to_string(),
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[derive(Debug, Deserialize)]
struct GhPullRequestSummary {
    number: u64,
    /// REST API returns both `url` (API URL) and `html_url` (browser URL).
    /// We prefer `html_url` when present, falling back to `url`.
    #[serde(default)]
    html_url: Option<String>,
    url: String,
}

impl GhPullRequestSummary {
    fn browser_url(&self) -> &str {
        self.html_url.as_deref().unwrap_or(&self.url)
    }
}

#[derive(Debug)]
pub enum RuntimeError {
    Io(std::io::Error),
    Json(serde_json::Error),
    Template(TemplateError),
    State(StateError),
    GateInitialization(GateInitializationError),
    CommandFailed { program: String, details: String },
    PullRequestNotFound(String),
    MissingRepositoryName,
    DetachedHead,
    StateBranchMismatch { expected: String, actual: String },
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::Io(error) => write!(f, "runtime I/O error: {error}"),
            RuntimeError::Json(error) => write!(f, "runtime JSON error: {error}"),
            RuntimeError::Template(error) => write!(f, "runtime template error: {error}"),
            RuntimeError::State(error) => write!(f, "runtime state error: {error}"),
            RuntimeError::GateInitialization(error) => {
                write!(f, "runtime gate initialization error: {error}")
            }
            RuntimeError::CommandFailed { program, details } => {
                write!(f, "{program} command failed: {details}")
            }
            RuntimeError::PullRequestNotFound(branch) => {
                write!(f, "no pull request found for branch '{branch}'")
            }
            RuntimeError::MissingRepositoryName => {
                write!(
                    f,
                    "could not derive a repository identifier from the repo root"
                )
            }
            RuntimeError::DetachedHead => {
                write!(
                    f,
                    "current git checkout is detached; an active branch is required"
                )
            }
            RuntimeError::StateBranchMismatch { expected, actual } => write!(
                f,
                "repository state belongs to branch '{actual}', but the current branch is '{expected}'"
            ),
        }
    }
}

impl std::error::Error for RuntimeError {}
