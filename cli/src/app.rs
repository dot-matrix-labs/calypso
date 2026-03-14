use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::doctor::{HostDoctorEnvironment, collect_doctor_report, render_doctor_report};
use crate::github::{HostGithubEnvironment, collect_github_report};
use crate::policy::{HostPolicyEnvironment, collect_policy_evidence};
use crate::state::{
    BuiltinEvidence, FeatureState, GateStatus, PullRequestChecklistItem, PullRequestRef,
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
    let pull_request = resolve_current_pull_request(&repo_root);
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
    let github_evidence = pull_request
        .as_ref()
        .map(|pr| collect_github_report(&HostGithubEnvironment, pr).to_builtin_evidence())
        .unwrap_or_else(missing_pull_request_evidence);
    let policy_evidence = collect_policy_evidence(&HostPolicyEnvironment, &repo_root, &template);
    let evidence = doctor_evidence
        .merge(&github_evidence)
        .merge(&policy_evidence);

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
    run_command(cwd, "git", &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

pub fn resolve_current_branch(repo_root: &Path) -> Option<String> {
    run_command(repo_root, "git", &["branch", "--show-current"])
}

pub fn resolve_current_pull_request(repo_root: &Path) -> Option<PullRequestRef> {
    resolve_current_pull_request_with_program(repo_root, "gh")
}

pub fn resolve_current_pull_request_with_program(
    repo_root: &Path,
    program: &str,
) -> Option<PullRequestRef> {
    let output = run_command(repo_root, program, &["pr", "view", "--json", "number,url"])?;
    parse_pull_request_ref(&output)
}

pub fn missing_pull_request_ref() -> PullRequestRef {
    PullRequestRef {
        number: 0,
        url: String::new(),
    }
}

pub fn missing_pull_request_evidence() -> BuiltinEvidence {
    BuiltinEvidence::new()
        .with_result("builtin.github.pr_exists", false)
        .with_result("builtin.github.pr_merged", false)
        .with_result("builtin.github.pr_checks_green", false)
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

pub fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    String::from_utf8(output.stdout)
        .ok()
        .map(|stdout| stdout.trim().to_string())
}
