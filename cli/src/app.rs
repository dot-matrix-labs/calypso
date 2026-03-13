use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::doctor::{HostDoctorEnvironment, collect_doctor_report, render_doctor_report};
use crate::github::{HostGithubEnvironment, collect_github_report};
use crate::policy::{HostPolicyEnvironment, collect_policy_evidence};
use crate::state::{
    BuiltinEvidence, EvidenceStatus, FeatureState, GateStatus, GithubMergeability,
    GithubReviewStatus, PullRequestChecklistItem, PullRequestRef,
};
use crate::template::load_embedded_template_set;

pub fn run_doctor(cwd: &Path) -> String {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);

    render_doctor_report(&report)
}

pub fn run_status(cwd: &Path) -> Result<String, String> {
    let repo_root =
        resolve_repo_root(cwd).ok_or_else(|| "not inside a git repository".to_string())?;
    let branch = resolve_current_branch(&repo_root)
        .expect("git repositories should report the current branch");
    let template = load_embedded_template_set().expect("embedded templates should remain valid");
    let pull_request_lookup = resolve_current_pull_request(&repo_root);
    let pull_request = match &pull_request_lookup {
        Ok(pull_request) => pull_request.clone(),
        Err(_) => None,
    };
    let mut feature = FeatureState::from_template(
        branch.as_str(),
        branch.as_str(),
        repo_root.to_string_lossy().as_ref(),
        pull_request
            .clone()
            .unwrap_or_else(missing_pull_request_ref),
        &template,
    )
    .expect("embedded templates should initialize feature state");

    let doctor_evidence =
        collect_doctor_report(&HostDoctorEnvironment, &repo_root).to_builtin_evidence();
    let github_report = pull_request
        .as_ref()
        .map(|pr| collect_github_report(&HostGithubEnvironment, pr));
    let github_evidence = github_report
        .as_ref()
        .map(|report| report.to_builtin_evidence())
        .unwrap_or_else(missing_pull_request_evidence);
    let policy_evidence = collect_policy_evidence(&HostPolicyEnvironment, &repo_root, &template);
    let evidence = doctor_evidence
        .merge(&github_evidence)
        .merge(&policy_evidence);
    feature.github_snapshot = github_report
        .as_ref()
        .and_then(|report| report.snapshot.clone());
    feature.github_error = match &pull_request_lookup {
        Err(error) => Some(error.clone()),
        Ok(_) => None,
    }
    .or_else(|| {
        github_report
            .as_ref()
            .and_then(|report| report.error.clone())
    });

    feature
        .evaluate_gates(&template, &evidence)
        .expect("embedded templates should evaluate known builtin gates");

    Ok(render_feature_status(
        &repo_root,
        &branch,
        pull_request.as_ref(),
        &feature,
    ))
}

pub fn render_feature_status(
    repo_root: &Path,
    branch: &str,
    pull_request: Option<&PullRequestRef>,
    feature: &FeatureState,
) -> String {
    let mut lines = vec![
        "Feature status".to_string(),
        format!("Repo: {}", repo_root.display()),
        format!("Branch: {branch}"),
        format!(
            "Pull request: {}",
            pull_request
                .map(|pr| format!("#{} {}", pr.number, pr.url))
                .unwrap_or_else(|| "missing".to_string())
        ),
        format!("Workflow state: {:?}", feature.workflow_state),
    ];

    for group in &feature.gate_groups {
        lines.push(String::new());
        lines.push(group.label.clone());
        for gate in &group.gates {
            lines.push(format!(
                "- [{}] {}",
                gate_status_label(&gate.status),
                gate.label
            ));
        }
    }

    lines.push(String::new());
    lines.push("PR checklist".to_string());
    for item in feature.pull_request_checklist() {
        lines.push(format!("- [{}] {}", checklist_marker(&item), item.label));
    }

    let blocking = feature.blocking_gate_ids();
    lines.push(String::new());
    if let Some(snapshot) = &feature.github_snapshot {
        lines.push("GitHub".to_string());
        lines.push(format!(
            "- PR state: {}",
            if snapshot.is_draft {
                "draft"
            } else {
                "ready-for-review"
            }
        ));
        lines.push(format!(
            "- Review: {}",
            github_review_label(&snapshot.review_status)
        ));
        lines.push(format!(
            "- Checks: {}",
            evidence_status_label(&snapshot.checks)
        ));
        lines.push(format!(
            "- Mergeability: {}",
            github_mergeability_label(&snapshot.mergeability)
        ));
        lines.push(String::new());
    } else if let Some(error) = &feature.github_error {
        lines.push("GitHub".to_string());
        lines.push(format!("- Error: {error}"));
        lines.push(String::new());
    }

    if blocking.is_empty() {
        lines.push("Blocking gates: none".to_string());
    } else {
        lines.push(format!("Blocking gates: {}", blocking.join(", ")));
    }

    lines.join("\n")
}

fn checklist_marker(item: &PullRequestChecklistItem) -> &'static str {
    if item.checked { "x" } else { " " }
}

pub fn gate_status_label(status: &GateStatus) -> &'static str {
    match status {
        GateStatus::Pending => "pending",
        GateStatus::Passing => "passing",
        GateStatus::Failing => "failing",
        GateStatus::Manual => "manual",
    }
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
    let output = run_command(repo_root, program, &["pr", "view", "--json", "number,url"])?;
    match output {
        CommandOutput::Success(output) => parse_pull_request_ref(&output)
            .map(Some)
            .ok_or_else(|| "gh returned malformed pull request JSON".to_string()),
        CommandOutput::Failure(error) => {
            if error.contains("no pull requests found") {
                Ok(None)
            } else {
                Err(error)
            }
        }
    }
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
        url: String,
    }

    let pull_request: GhPullRequest = serde_json::from_str(json).ok()?;

    Some(PullRequestRef {
        number: pull_request.number,
        url: pull_request.url,
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutput {
    Success(String),
    Failure(String),
}

pub fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<CommandOutput, String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
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

fn github_review_label(status: &GithubReviewStatus) -> &'static str {
    match status {
        GithubReviewStatus::Approved => "approved",
        GithubReviewStatus::ReviewRequired => "review-required",
        GithubReviewStatus::ChangesRequested => "changes-requested",
    }
}

fn github_mergeability_label(status: &GithubMergeability) -> &'static str {
    match status {
        GithubMergeability::Mergeable => "mergeable",
        GithubMergeability::Conflicting => "conflicting",
        GithubMergeability::Blocked => "blocked",
        GithubMergeability::Unknown => "unknown",
    }
}

fn evidence_status_label(status: &EvidenceStatus) -> &'static str {
    match status {
        EvidenceStatus::Passing => "passing",
        EvidenceStatus::Failing => "failing",
        EvidenceStatus::Pending => "pending",
        EvidenceStatus::Manual => "manual",
    }
}
