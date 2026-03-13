use std::path::{Path, PathBuf};
use std::process::Command;

use calypso_cli::doctor::{
    DoctorCheckId, DoctorReport, HostDoctorEnvironment, collect_doctor_report,
};
use calypso_cli::github::{HostGithubEnvironment, collect_github_report};
use calypso_cli::state::{BuiltinEvidence, FeatureState, GateStatus, PullRequestRef};
use calypso_cli::template::load_embedded_template_set;
use calypso_cli::{BuildInfo, render_help, render_version};
use serde::Deserialize;

fn build_info() -> BuildInfo<'static> {
    const VERSION: &str = concat!(
        env!("CARGO_PKG_VERSION"),
        "+",
        env!("CALYPSO_BUILD_GIT_HASH")
    );

    BuildInfo {
        version: VERSION,
        git_hash: env!("CALYPSO_BUILD_GIT_HASH"),
        build_time: env!("CALYPSO_BUILD_TIME"),
        git_tags: env!("CALYPSO_BUILD_GIT_TAGS"),
    }
}

fn main() {
    let info = build_info();
    let arg = std::env::args().nth(1);

    match arg.as_deref() {
        Some("-v") | Some("--version") => {
            println!("{}", render_version(info));
        }
        Some("doctor") => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            println!("{}", run_doctor(&cwd));
        }
        Some("status") => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            match run_status(&cwd) {
                Ok(output) => println!("{output}"),
                Err(error) => {
                    eprintln!("status error: {error}");
                    std::process::exit(1);
                }
            }
        }
        _ => {
            println!("{}", render_help(info));
        }
    }
}

fn run_doctor(cwd: &Path) -> String {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);

    render_doctor_report(&report)
}

fn run_status(cwd: &Path) -> Result<String, String> {
    let repo_root =
        resolve_repo_root(cwd).ok_or_else(|| "not inside a git repository".to_string())?;
    let branch = resolve_current_branch(&repo_root)
        .ok_or_else(|| "unable to resolve current branch".to_string())?;
    let template = load_embedded_template_set().map_err(|error| error.to_string())?;
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
    .map_err(|error| error.to_string())?;

    let doctor_evidence =
        collect_doctor_report(&HostDoctorEnvironment, &repo_root).to_builtin_evidence();
    let github_evidence = pull_request
        .as_ref()
        .map(|pr| collect_github_report(&HostGithubEnvironment, pr).to_builtin_evidence())
        .unwrap_or_else(missing_pull_request_evidence);
    let evidence = doctor_evidence.merge(&github_evidence);

    feature
        .evaluate_gates(&template, &evidence)
        .map_err(|error| error.to_string())?;

    Ok(render_feature_status(
        &repo_root,
        &branch,
        pull_request.as_ref(),
        &feature,
    ))
}

fn render_doctor_report(report: &DoctorReport) -> String {
    let mut lines = vec!["Doctor checks".to_string()];

    for check in &report.checks {
        let status = if matches!(check.status, calypso_cli::doctor::DoctorStatus::Passing) {
            "PASS"
        } else {
            "FAIL"
        };
        let name = match check.id {
            DoctorCheckId::GhInstalled => "gh-installed",
            DoctorCheckId::CodexInstalled => "codex-installed",
            DoctorCheckId::GhAuthenticated => "gh-authenticated",
            DoctorCheckId::GithubRemoteConfigured => "github-remote-configured",
        };
        lines.push(format!("- [{status}] {name}"));
    }

    lines.join("\n")
}

fn render_feature_status(
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

    let blocking = feature.blocking_gate_ids();
    lines.push(String::new());
    if blocking.is_empty() {
        lines.push("Blocking gates: none".to_string());
    } else {
        lines.push(format!("Blocking gates: {}", blocking.join(", ")));
    }

    lines.join("\n")
}

fn gate_status_label(status: &GateStatus) -> &'static str {
    match status {
        GateStatus::Pending => "pending",
        GateStatus::Passing => "passing",
        GateStatus::Failing => "failing",
        GateStatus::Manual => "manual",
    }
}

fn resolve_repo_root(cwd: &Path) -> Option<PathBuf> {
    run_command(cwd, "git", &["rev-parse", "--show-toplevel"]).map(PathBuf::from)
}

fn resolve_current_branch(repo_root: &Path) -> Option<String> {
    run_command(repo_root, "git", &["branch", "--show-current"])
}

fn resolve_current_pull_request(repo_root: &Path) -> Option<PullRequestRef> {
    #[derive(Deserialize)]
    struct GhPullRequest {
        number: u64,
        url: String,
    }

    let output = run_command(repo_root, "gh", &["pr", "view", "--json", "number,url"])?;
    let pull_request: GhPullRequest = serde_json::from_str(&output).ok()?;

    Some(PullRequestRef {
        number: pull_request.number,
        url: pull_request.url,
    })
}

fn missing_pull_request_ref() -> PullRequestRef {
    PullRequestRef {
        number: 0,
        url: String::new(),
    }
}

fn missing_pull_request_evidence() -> BuiltinEvidence {
    BuiltinEvidence::new()
        .with_result("builtin.github.pr_exists", false)
        .with_result("builtin.github.pr_merged", false)
        .with_result("builtin.github.pr_checks_green", false)
}

fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Option<String> {
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
