use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::state::{BuiltinEvidence, PullRequestRef};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutput {
    Success(String),
    Failure(String),
}

pub fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<CommandOutput, String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        // Unset GIT_DIR / GIT_WORK_TREE so git subcommands discover the repo
        // from `cwd` rather than inheriting a parent hook's git context.
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .map_err(|error| format!("failed to spawn `{program}`: {error}"))?;

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
        .map_err(|error| format!("`{program}` produced invalid UTF-8: {error}"))
}

pub fn resolve_repo_root(cwd: &Path) -> Option<PathBuf> {
    match run_command(cwd, "git", &["rev-parse", "--show-toplevel"]) {
        Ok(CommandOutput::Success(output)) => Some(PathBuf::from(output)),
        Ok(CommandOutput::Failure(_)) | Err(_) => None,
    }
}

pub fn resolve_current_branch(repo_root: &Path) -> Option<String> {
    match run_command(repo_root, "git", &["branch", "--show-current"]) {
        Ok(CommandOutput::Success(output)) => Some(output),
        Ok(CommandOutput::Failure(_)) | Err(_) => None,
    }
}

pub fn resolve_current_pull_request(repo_root: &Path) -> Result<Option<PullRequestRef>, String> {
    resolve_current_pull_request_with_program(repo_root, "gh")
}

pub fn resolve_current_pull_request_with_program(
    repo_root: &Path,
    program: &str,
) -> Result<Option<PullRequestRef>, String> {
    // Resolve owner/repo and current branch, then query the REST API.
    let (owner, repo_name) = crate::github::resolve_owner_repo(repo_root)
        .map_err(|e| format!("could not resolve owner/repo: {e}"))?;
    let branch = resolve_current_branch(repo_root)
        .ok_or_else(|| "could not determine current branch".to_string())?;

    let endpoint =
        format!("repos/{owner}/{repo_name}/pulls?head={owner}:{branch}&per_page=1&state=open");
    let output = run_command(repo_root, program, &["api", &endpoint])?;
    match output {
        CommandOutput::Success(output) => parse_pull_request_list_json(&output),
        CommandOutput::Failure(error) => {
            if error.contains("no pull requests found") {
                Ok(None)
            } else {
                Err(error)
            }
        }
    }
}

/// Parse a JSON array of pull requests (REST format) and return the first one.
fn parse_pull_request_list_json(json: &str) -> Result<Option<PullRequestRef>, String> {
    #[derive(Deserialize)]
    struct RestPr {
        number: u64,
        #[serde(default)]
        html_url: Option<String>,
        url: String,
    }

    let prs: Vec<RestPr> = serde_json::from_str(json)
        .map_err(|_| "gh returned malformed pull request JSON".to_string())?;

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
