use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::error::{CalypsoError, Recoverability};
use crate::state::{BuiltinEvidence, PullRequestRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutput {
    Success(String),
    Failure(String),
}

pub fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<CommandOutput, CalypsoError> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        // Unset GIT_DIR / GIT_WORK_TREE so git subcommands discover the repo
        // from `cwd` rather than inheriting a parent hook's git context.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .map_err(|error| CalypsoError::subprocess_spawn(format!("failed to spawn `{program}`: {error}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            format!("`{program}` exited with status {}", output.status)
        } else {
            stderr
        };
        return Ok(CommandOutput::Failure(message));
    }

    String::from_utf8(output.stdout)
        .map(|stdout| CommandOutput::Success(stdout.trim().to_string()))
        .map_err(|error| {
            CalypsoError::new(
                crate::error::codes::MALFORMED_PROVIDER_OUTPUT,
                format!("`{program}` produced invalid UTF-8: {error}"),
                Recoverability::Unrecoverable,
            )
        })
}

pub fn resolve_repo_root(cwd: &Path) -> Result<PathBuf, CalypsoError> {
    match run_command(cwd, "git", &["rev-parse", "--show-toplevel"])? {
        CommandOutput::Success(output) => Ok(PathBuf::from(output)),
        CommandOutput::Failure(message) => Err(CalypsoError::repo_root_not_found(format!(
            "not inside a git repository: {message}"
        ))),
    }
}

pub fn resolve_current_branch(repo_root: &Path) -> Result<String, CalypsoError> {
    match run_command(repo_root, "git", &["branch", "--show-current"])? {
        CommandOutput::Success(output) => Ok(output),
        CommandOutput::Failure(message) => Err(CalypsoError::git(format!(
            "could not determine current branch: {message}"
        ))),
    }
}

pub fn resolve_current_pull_request(repo_root: &Path) -> Result<Option<PullRequestRef>, CalypsoError> {
    resolve_current_pull_request_with_program(repo_root, "gh")
}

pub fn resolve_current_pull_request_with_program(
    repo_root: &Path,
    program: &str,
) -> Result<Option<PullRequestRef>, CalypsoError> {
    // Resolve owner/repo and current branch, then query the REST API.
    let (owner, repo_name) = crate::github::resolve_owner_repo(repo_root)
        .map_err(|e| CalypsoError::git(format!("could not resolve owner/repo: {e}")))?;
    let branch = resolve_current_branch(repo_root)?;

    let endpoint =
        format!("repos/{owner}/{repo_name}/pulls?head={owner}:{branch}&per_page=1&state=open");
    let output = run_command(repo_root, program, &["api", &endpoint])?;
    match output {
        CommandOutput::Success(output) => parse_pull_request_list_json(&output),
        CommandOutput::Failure(error) => {
            if error.contains("no pull requests found") {
                Ok(None)
            } else {
                Err(CalypsoError::github_api(error))
            }
        }
    }
}

/// Parse a JSON array of pull requests (REST format) and return the first one.
fn parse_pull_request_list_json(json: &str) -> Result<Option<PullRequestRef>, CalypsoError> {
    #[derive(Deserialize)]
    struct RestPr {
        number: u64,
        #[serde(default)]
        html_url: Option<String>,
        url: String,
    }

    let prs: Vec<RestPr> = serde_json::from_str(json)
        .map_err(|_| CalypsoError::malformed_provider_output("gh returned malformed pull request JSON"))?;

    Ok(prs.into_iter().next().map(|pr| PullRequestRef {
        number: pr.number,
        url: pr.html_url.unwrap_or(pr.url),
    }))
}

pub fn missing_pull_request_ref() -> PullRequestRef {
    PullRequestRef {
        number: 0,
        url: String::new(),
    }
}

pub fn missing_pull_request_evidence() -> BuiltinEvidence {
    BuiltinEvidence::new().with_result("builtin.github.pr_exists", false)
}

pub fn parse_pull_request_ref(json: &str) -> Option<PullRequestRef> {
    #[derive(Deserialize)]
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
