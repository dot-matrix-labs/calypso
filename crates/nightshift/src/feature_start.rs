use std::fmt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::pr_checklist::seed_pr_body;
use crate::runtime::{PullRequestResolver, RuntimeError, load_or_initialize_runtime};
use crate::state::PullRequestRef;
use crate::template::resolve_template_set_for_path;

const DEFAULT_BRANCH_PREFIX: &str = "feat";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureStartRequest {
    pub feature_id: String,
    pub worktree_base: PathBuf,
    pub title: Option<String>,
    pub body: Option<String>,
    pub allow_dirty: bool,
    pub allow_non_main: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureStartResult {
    pub branch: String,
    pub worktree_path: PathBuf,
    pub pull_request: PullRequestRef,
    pub state_path: PathBuf,
}

pub trait FeatureStartEnvironment {
    fn resolve_repo_root(&self, start_path: &Path) -> Result<PathBuf, FeatureStartError>;
    fn current_branch(&self, repo_root: &Path) -> Result<String, FeatureStartError>;
    fn is_working_tree_clean(&self, repo_root: &Path) -> Result<bool, FeatureStartError>;
    fn main_branch_exists(&self, repo_root: &Path) -> Result<bool, FeatureStartError>;
    fn branch_exists(&self, repo_root: &Path, branch: &str) -> Result<bool, FeatureStartError>;
    fn path_exists(&self, path: &Path) -> bool;
    fn create_branch_from_main(
        &self,
        repo_root: &Path,
        branch: &str,
    ) -> Result<(), FeatureStartError>;
    fn create_worktree(
        &self,
        repo_root: &Path,
        branch: &str,
        worktree_path: &Path,
    ) -> Result<(), FeatureStartError>;
    fn push_branch(&self, worktree_path: &Path, branch: &str) -> Result<(), FeatureStartError>;
    fn create_draft_pull_request(
        &self,
        worktree_path: &Path,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<PullRequestRef, FeatureStartError>;
    fn bootstrap_state(
        &self,
        worktree_path: &Path,
        pull_request: PullRequestRef,
    ) -> Result<PathBuf, FeatureStartError>;
    fn remove_worktree(
        &self,
        repo_root: &Path,
        worktree_path: &Path,
    ) -> Result<(), FeatureStartError>;
    fn remove_branch(&self, repo_root: &Path, branch: &str) -> Result<(), FeatureStartError>;
    fn update_pull_request_body(
        &self,
        worktree_path: &Path,
        pr_number: u64,
        body: &str,
    ) -> Result<(), FeatureStartError>;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HostFeatureStartEnvironment;

impl FeatureStartEnvironment for HostFeatureStartEnvironment {
    fn resolve_repo_root(&self, start_path: &Path) -> Result<PathBuf, FeatureStartError> {
        let output = git_output(start_path, &["rev-parse", "--show-toplevel"])?;
        PathBuf::from(output)
            .canonicalize()
            .map_err(FeatureStartError::Io)
    }

    fn current_branch(&self, repo_root: &Path) -> Result<String, FeatureStartError> {
        git_output(repo_root, &["branch", "--show-current"])
    }

    fn is_working_tree_clean(&self, repo_root: &Path) -> Result<bool, FeatureStartError> {
        Ok(git_output(repo_root, &["status", "--porcelain"])?.is_empty())
    }

    fn main_branch_exists(&self, repo_root: &Path) -> Result<bool, FeatureStartError> {
        Ok(run_git(
            repo_root,
            &["show-ref", "--verify", "--quiet", "refs/heads/main"],
        )
        .is_ok())
    }

    fn branch_exists(&self, repo_root: &Path, branch: &str) -> Result<bool, FeatureStartError> {
        Ok(run_git(
            repo_root,
            &[
                "show-ref",
                "--verify",
                "--quiet",
                &format!("refs/heads/{branch}"),
            ],
        )
        .is_ok())
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_branch_from_main(
        &self,
        repo_root: &Path,
        branch: &str,
    ) -> Result<(), FeatureStartError> {
        run_git(repo_root, &["branch", branch, "main"]).map(|_| ())
    }

    fn create_worktree(
        &self,
        repo_root: &Path,
        branch: &str,
        worktree_path: &Path,
    ) -> Result<(), FeatureStartError> {
        if let Some(parent) = worktree_path.parent() {
            std::fs::create_dir_all(parent).map_err(FeatureStartError::Io)?;
        }

        run_git(
            repo_root,
            &["worktree", "add", &path_string(worktree_path), branch],
        )
        .map(|_| ())
    }

    fn push_branch(&self, worktree_path: &Path, branch: &str) -> Result<(), FeatureStartError> {
        run_git(worktree_path, &["push", "-u", "origin", branch]).map(|_| ())
    }

    fn create_draft_pull_request(
        &self,
        worktree_path: &Path,
        branch: &str,
        title: &str,
        body: &str,
    ) -> Result<PullRequestRef, FeatureStartError> {
        let (owner, repo_name) = crate::github::resolve_owner_repo(worktree_path)
            .map_err(|e| FeatureStartError::github("resolve owner/repo", &e.to_string()))?;

        // Create the draft PR via REST API.
        let endpoint = format!("repos/{owner}/{repo_name}/pulls");
        let create_json = run_gh(
            worktree_path,
            &[
                "api",
                "--method",
                "POST",
                &endpoint,
                "-f",
                &format!("head={branch}"),
                "-f",
                "base=main",
                "-f",
                &format!("title={title}"),
                "-f",
                &format!("body={body}"),
                "-F",
                "draft=true",
            ],
        )?;

        // Parse the response directly — it contains number and html_url.
        parse_pull_request_rest_response(&create_json).ok_or_else(|| {
            FeatureStartError::github("gh api create PR", "failed to parse pull request response")
        })
    }

    fn bootstrap_state(
        &self,
        worktree_path: &Path,
        pull_request: PullRequestRef,
    ) -> Result<PathBuf, FeatureStartError> {
        let runtime =
            load_or_initialize_runtime(worktree_path, &StaticPullRequestResolver { pull_request })
                .map_err(FeatureStartError::Runtime)?;
        Ok(runtime.state_path)
    }

    fn remove_worktree(
        &self,
        repo_root: &Path,
        worktree_path: &Path,
    ) -> Result<(), FeatureStartError> {
        run_git(
            repo_root,
            &["worktree", "remove", "--force", &path_string(worktree_path)],
        )
        .map(|_| ())
    }

    fn remove_branch(&self, repo_root: &Path, branch: &str) -> Result<(), FeatureStartError> {
        run_git(repo_root, &["branch", "-D", branch]).map(|_| ())
    }

    fn update_pull_request_body(
        &self,
        worktree_path: &Path,
        pr_number: u64,
        body: &str,
    ) -> Result<(), FeatureStartError> {
        let (owner, repo_name) = crate::github::resolve_owner_repo(worktree_path)
            .map_err(|e| FeatureStartError::github("resolve owner/repo", &e.to_string()))?;

        let endpoint = format!("repos/{owner}/{repo_name}/pulls/{pr_number}");
        run_gh(
            worktree_path,
            &[
                "api",
                "--method",
                "PATCH",
                &endpoint,
                "-f",
                &format!("body={body}"),
            ],
        )
        .map(|_| ())
    }
}

pub fn run_feature_start(
    cwd: &Path,
    request: &FeatureStartRequest,
) -> Result<FeatureStartResult, FeatureStartError> {
    start_feature(cwd, request, &HostFeatureStartEnvironment)
}

pub fn start_feature(
    start_path: &Path,
    request: &FeatureStartRequest,
    environment: &impl FeatureStartEnvironment,
) -> Result<FeatureStartResult, FeatureStartError> {
    let repo_root = environment.resolve_repo_root(start_path)?;
    let current_branch = environment.current_branch(&repo_root)?;

    if current_branch.is_empty() {
        return Err(FeatureStartError::DetachedHead);
    }

    if !request.allow_non_main && current_branch != "main" {
        return Err(FeatureStartError::InvalidBaseBranch {
            expected: "main".to_string(),
            actual: current_branch,
        });
    }

    if !request.allow_dirty && !environment.is_working_tree_clean(&repo_root)? {
        return Err(FeatureStartError::DirtyWorkingTree);
    }

    if !environment.main_branch_exists(&repo_root)? {
        return Err(FeatureStartError::MissingMainBranch);
    }

    let branch = derive_feature_branch_name(&request.feature_id)?;
    let worktree_path = request.worktree_base.join(branch.replace('/', "-"));

    if environment.branch_exists(&repo_root, &branch)? {
        return Err(FeatureStartError::BranchAlreadyExists(branch));
    }

    if environment.path_exists(&worktree_path) {
        return Err(FeatureStartError::WorktreePathExists(worktree_path));
    }

    environment.create_branch_from_main(&repo_root, &branch)?;

    if let Err(error) = environment.create_worktree(&repo_root, &branch, &worktree_path) {
        let _ = environment.remove_branch(&repo_root, &branch);
        return Err(error);
    }

    if let Err(error) = environment.push_branch(&worktree_path, &branch) {
        rollback_local_feature(environment, &repo_root, &branch, &worktree_path);
        return Err(error);
    }

    let title = request
        .title
        .clone()
        .unwrap_or_else(|| default_pr_title(&request.feature_id));
    let body = request
        .body
        .clone()
        .unwrap_or_else(|| default_pr_body(&request.feature_id, &branch));

    let pull_request = match environment.create_draft_pull_request(
        &worktree_path,
        &branch,
        &title,
        &body,
    ) {
        Ok(pull_request) => pull_request,
        Err(error) => {
            rollback_local_feature(environment, &repo_root, &branch, &worktree_path);
            return Err(FeatureStartError::PartialFailure {
                message: error.to_string(),
                recovery: vec![
                    format!(
                        "Delete any pushed remote branch with `git push origin --delete {branch}`."
                    ),
                    "Re-run `calypso-cli feature-start` after the GitHub error is resolved."
                        .to_string(),
                ],
            });
        }
    };

    let state_path = match environment.bootstrap_state(&worktree_path, pull_request.clone()) {
        Ok(state_path) => state_path,
        Err(error) => {
            rollback_local_feature(environment, &repo_root, &branch, &worktree_path);
            return Err(FeatureStartError::PartialFailure {
                message: error.to_string(),
                recovery: vec![
                    format!(
                        "The draft pull request remains open at {}.",
                        pull_request.url
                    ),
                    format!(
                        "Delete the remote branch with `git push origin --delete {branch}` if you are abandoning this start."
                    ),
                    "Recreate the worktree and state after the runtime bootstrap issue is fixed."
                        .to_string(),
                ],
            });
        }
    };

    // Seed the PR checklist — best-effort; do not fail feature start on error.
    if let Ok(template) = resolve_template_set_for_path(&worktree_path) {
        // Load gate groups from bootstrapped state if available.
        let gate_groups =
            if let Ok(repo_state) = crate::state::RepositoryState::load_from_path(&state_path) {
                repo_state.current_feature.gate_groups
            } else {
                Vec::new()
            };

        let seeded_body = seed_pr_body(&request.feature_id, "feat", &gate_groups, &template);
        let _ =
            environment.update_pull_request_body(&worktree_path, pull_request.number, &seeded_body);
    }

    Ok(FeatureStartResult {
        branch,
        worktree_path,
        pull_request,
        state_path,
    })
}

pub fn derive_feature_branch_name(feature_id: &str) -> Result<String, FeatureStartError> {
    let trimmed = feature_id.trim();
    if trimmed.is_empty() {
        return Err(FeatureStartError::EmptyFeatureId);
    }

    let mut slug = String::with_capacity(trimmed.len());
    let mut previous_was_dash = false;

    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() {
            slug.push(ch.to_ascii_lowercase());
            previous_was_dash = false;
        } else if !previous_was_dash {
            slug.push('-');
            previous_was_dash = true;
        }
    }

    let slug = slug.trim_matches('-').to_string();
    if slug.is_empty() {
        return Err(FeatureStartError::EmptyFeatureId);
    }

    Ok(format!("{DEFAULT_BRANCH_PREFIX}/{slug}"))
}

fn default_pr_title(feature_id: &str) -> String {
    feature_id.trim().to_string()
}

fn default_pr_body(feature_id: &str, branch: &str) -> String {
    format!(
        "## Summary\n- start `{feature_id}`\n- create bound feature unit for `{branch}` from `main`\n\n## Test plan\n- cargo test -p calypso-cli"
    )
}

fn rollback_local_feature(
    environment: &impl FeatureStartEnvironment,
    repo_root: &Path,
    branch: &str,
    worktree_path: &Path,
) {
    let _ = environment.remove_worktree(repo_root, worktree_path);
    let _ = environment.remove_branch(repo_root, branch);
}

fn parse_pull_request_ref(json: &str) -> Option<PullRequestRef> {
    #[derive(serde::Deserialize)]
    struct GhPullRequest {
        number: u64,
        #[serde(default)]
        html_url: Option<String>,
        url: String,
    }

    let pull_request: GhPullRequest = serde_json::from_str(json).ok()?;

    Some(PullRequestRef {
        number: pull_request.number,
        url: pull_request.html_url.unwrap_or(pull_request.url),
    })
}

/// Parse a single PR JSON object from REST response (e.g., from POST /pulls).
fn parse_pull_request_rest_response(json: &str) -> Option<PullRequestRef> {
    parse_pull_request_ref(json)
}

fn git_output(current_dir: &Path, args: &[&str]) -> Result<String, FeatureStartError> {
    let output = run_git(current_dir, args)?;
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn run_git(current_dir: &Path, args: &[&str]) -> Result<std::process::Output, FeatureStartError> {
    let output = Command::new("git")
        .args(args)
        .current_dir(current_dir)
        .output()
        .map_err(FeatureStartError::Io)?;

    if output.status.success() {
        Ok(output)
    } else {
        Err(FeatureStartError::git(
            &format!("git {}", args.join(" ")),
            String::from_utf8_lossy(&output.stderr).trim(),
        ))
    }
}

fn run_gh(current_dir: &Path, args: &[&str]) -> Result<String, FeatureStartError> {
    let output = Command::new("gh")
        .args(args)
        .current_dir(current_dir)
        .output()
        .map_err(FeatureStartError::Io)?;

    if !output.status.success() {
        return Err(FeatureStartError::github(
            &format!("gh {}", args.join(" ")),
            String::from_utf8_lossy(&output.stderr).trim(),
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn path_string(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

struct StaticPullRequestResolver {
    pull_request: PullRequestRef,
}

impl PullRequestResolver for StaticPullRequestResolver {
    fn resolve_for_branch(
        &self,
        _repo_root: &Path,
        _branch: &str,
    ) -> Result<PullRequestRef, RuntimeError> {
        Ok(self.pull_request.clone())
    }
}

#[derive(Debug)]
pub enum FeatureStartError {
    Io(std::io::Error),
    Runtime(RuntimeError),
    EmptyFeatureId,
    DetachedHead,
    DirtyWorkingTree,
    MissingMainBranch,
    InvalidBaseBranch {
        expected: String,
        actual: String,
    },
    BranchAlreadyExists(String),
    WorktreePathExists(PathBuf),
    GitCommandFailed {
        action: String,
        details: String,
    },
    GithubCommandFailed {
        action: String,
        details: String,
    },
    PartialFailure {
        message: String,
        recovery: Vec<String>,
    },
}

impl FeatureStartError {
    fn git(action: &str, details: &str) -> Self {
        Self::GitCommandFailed {
            action: action.to_string(),
            details: details.to_string(),
        }
    }

    fn github(action: &str, details: &str) -> Self {
        Self::GithubCommandFailed {
            action: action.to_string(),
            details: details.to_string(),
        }
    }
}

impl fmt::Display for FeatureStartError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            FeatureStartError::Io(error) => write!(f, "feature start I/O error: {error}"),
            FeatureStartError::Runtime(error) => {
                write!(f, "feature start runtime error: {error}")
            }
            FeatureStartError::EmptyFeatureId => write!(f, "feature identifier must not be empty"),
            FeatureStartError::DetachedHead => {
                write!(
                    f,
                    "feature start requires a named base branch, not detached HEAD"
                )
            }
            FeatureStartError::DirtyWorkingTree => {
                write!(f, "feature start requires a clean working tree on `main`")
            }
            FeatureStartError::MissingMainBranch => {
                write!(f, "feature start requires a local `main` branch")
            }
            FeatureStartError::InvalidBaseBranch { expected, actual } => {
                write!(
                    f,
                    "feature start requires `{expected}` as the base branch, found `{actual}`"
                )
            }
            FeatureStartError::BranchAlreadyExists(branch) => {
                write!(f, "feature branch `{branch}` already exists")
            }
            FeatureStartError::WorktreePathExists(path) => {
                write!(f, "target worktree path already exists: {}", path.display())
            }
            FeatureStartError::GitCommandFailed { action, details } => {
                write!(f, "{action} failed: {details}")
            }
            FeatureStartError::GithubCommandFailed { action, details } => {
                write!(f, "{action} failed: {details}")
            }
            FeatureStartError::PartialFailure { message, recovery } => {
                write!(f, "feature start left partial remote state: {message}")?;
                if !recovery.is_empty() {
                    write!(f, "\nRecovery: {}", recovery.join(" "))?;
                }
                Ok(())
            }
        }
    }
}

impl std::error::Error for FeatureStartError {}
