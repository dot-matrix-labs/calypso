use std::path::Path;
use std::process::Command;

use crate::claude::{ClaudeConfig, ClaudeSession};
use crate::state::BuiltinEvidence;

const REQUIRED_WORKFLOW_FILES: [&str; 6] = [
    "rust-quality.yml",
    "rust-unit.yml",
    "rust-integration.yml",
    "rust-e2e.yml",
    "rust-coverage.yml",
    "release-cli.yml",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DoctorCheckId {
    GhInstalled,
    CodexInstalled,
    ClaudeInstalled,
    GhAuthenticated,
    GithubRemoteConfigured,
    RequiredWorkflowFilesPresent,
}

impl DoctorCheckId {
    fn builtin_key(self) -> &'static str {
        match self {
            DoctorCheckId::GhInstalled => "builtin.doctor.gh_installed",
            DoctorCheckId::CodexInstalled => "builtin.doctor.codex_installed",
            DoctorCheckId::ClaudeInstalled => "builtin.doctor.claude_installed",
            DoctorCheckId::GhAuthenticated => "builtin.doctor.gh_authenticated",
            DoctorCheckId::GithubRemoteConfigured => "builtin.doctor.github_remote_configured",
            DoctorCheckId::RequiredWorkflowFilesPresent => {
                "builtin.doctor.required_workflows_present"
            }
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DoctorCheckId::GhInstalled => "gh-installed",
            DoctorCheckId::CodexInstalled => "codex-installed",
            DoctorCheckId::ClaudeInstalled => "claude-installed",
            DoctorCheckId::GhAuthenticated => "gh-authenticated",
            DoctorCheckId::GithubRemoteConfigured => "github-remote-configured",
            DoctorCheckId::RequiredWorkflowFilesPresent => "required-workflows-present",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorStatus {
    Passing,
    Failing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorCheckScope {
    LocalConfiguration,
    ExternalAuth,
}

/// An automated or manual fix for a failing doctor check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorFix {
    /// Execute a command to resolve the issue.
    RunCommand { command: String, args: Vec<String> },
    /// Provide human-readable instructions only; no automated action available.
    Manual { instructions: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCheck {
    pub id: DoctorCheckId,
    pub scope: DoctorCheckScope,
    pub status: DoctorStatus,
    pub detail: Option<String>,
    pub remediation: Option<String>,
    pub fix: Option<DoctorFix>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorReport {
    pub checks: Vec<DoctorCheck>,
}

impl DoctorReport {
    pub fn to_builtin_evidence(&self) -> BuiltinEvidence {
        self.checks
            .iter()
            .fold(BuiltinEvidence::new(), |evidence, check| {
                evidence.with_result(
                    check.id.builtin_key(),
                    check.status == DoctorStatus::Passing,
                )
            })
    }
}

pub trait DoctorEnvironment {
    fn command_exists(&self, command: &str) -> bool;
    fn claude_reachable(&self) -> bool;
    fn gh_authenticated(&self) -> bool;
    fn has_github_remote(&self, repo_root: &Path) -> bool;
    fn missing_workflow_files(&self, repo_root: &Path) -> Vec<String>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HostDoctorEnvironment;

impl DoctorEnvironment for HostDoctorEnvironment {
    fn command_exists(&self, command: &str) -> bool {
        Command::new("which")
            .arg(command)
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn claude_reachable(&self) -> bool {
        ClaudeSession::check_auth(&ClaudeConfig::default())
    }

    fn gh_authenticated(&self) -> bool {
        Command::new("gh")
            .args(["auth", "status"])
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn has_github_remote(&self, repo_root: &Path) -> bool {
        Command::new("gh")
            .args(["repo", "view", "--json", "nameWithOwner"])
            .current_dir(repo_root)
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn missing_workflow_files(&self, repo_root: &Path) -> Vec<String> {
        let workflows_root = repo_root.join(".github/workflows");
        let mut missing = REQUIRED_WORKFLOW_FILES
            .iter()
            .filter(|file| !workflows_root.join(file).is_file())
            .map(|file| (*file).to_string())
            .collect::<Vec<_>>();
        missing.sort();
        missing
    }
}

pub fn collect_doctor_report(
    environment: &impl DoctorEnvironment,
    repo_root: &Path,
) -> DoctorReport {
    let mut missing_workflow_files = environment.missing_workflow_files(repo_root);
    missing_workflow_files.sort();

    DoctorReport {
        checks: vec![
            make_check(
                DoctorCheckId::GhInstalled,
                DoctorCheckScope::LocalConfiguration,
                environment.command_exists("gh"),
                None,
            ),
            make_check(
                DoctorCheckId::CodexInstalled,
                DoctorCheckScope::LocalConfiguration,
                environment.command_exists("codex"),
                None,
            ),
            make_check(
                DoctorCheckId::ClaudeInstalled,
                DoctorCheckScope::LocalConfiguration,
                environment.claude_reachable(),
                None,
            ),
            make_check(
                DoctorCheckId::GhAuthenticated,
                DoctorCheckScope::ExternalAuth,
                environment.gh_authenticated(),
                None,
            ),
            make_check(
                DoctorCheckId::GithubRemoteConfigured,
                DoctorCheckScope::LocalConfiguration,
                environment.has_github_remote(repo_root),
                None,
            ),
            make_check(
                DoctorCheckId::RequiredWorkflowFilesPresent,
                DoctorCheckScope::LocalConfiguration,
                missing_workflow_files.is_empty(),
                (!missing_workflow_files.is_empty()).then_some(missing_workflow_files.join(", ")),
            ),
        ],
    }
}

fn status_from_bool(passing: bool) -> DoctorStatus {
    if passing {
        DoctorStatus::Passing
    } else {
        DoctorStatus::Failing
    }
}

fn make_check(
    id: DoctorCheckId,
    scope: DoctorCheckScope,
    passing: bool,
    detail: Option<String>,
) -> DoctorCheck {
    let status = status_from_bool(passing);
    let remediation = (status == DoctorStatus::Failing)
        .then(|| failing_fix(id, detail.as_deref()))
        .flatten();
    let fix = (status == DoctorStatus::Failing)
        .then(|| failing_doctor_fix(id, detail.as_deref()))
        .flatten();

    DoctorCheck {
        id,
        scope,
        status,
        detail,
        remediation,
        fix,
    }
}

pub fn render_doctor_report(report: &DoctorReport) -> String {
    let mut lines = vec!["Doctor checks".to_string()];

    for check in &report.checks {
        let status = if matches!(check.status, DoctorStatus::Passing) {
            "PASS"
        } else {
            "FAIL"
        };
        lines.push(format!("- [{status}] {}", check.id.label()));

        if let Some(detail) = &check.detail {
            lines.push(format!("  detail: {detail}"));
        }

        if let Some(fix) = &check.remediation {
            lines.push(format!("  fix: {fix}"));
        }
    }

    lines.join("\n")
}

/// Render a verbose doctor report that includes remediation steps for every failing check.
pub fn render_doctor_report_verbose(report: &DoctorReport) -> String {
    let mut lines = vec!["Doctor checks (verbose)".to_string()];

    for check in &report.checks {
        let status = if matches!(check.status, DoctorStatus::Passing) {
            "PASS"
        } else {
            "FAIL"
        };
        lines.push(format!("- [{status}] {}", check.id.label()));

        if let Some(detail) = &check.detail {
            lines.push(format!("  detail: {detail}"));
        }

        if let Some(fix) = &check.remediation {
            lines.push(format!("  fix: {fix}"));
        }

        if let Some(doctor_fix) = &check.fix {
            match doctor_fix {
                DoctorFix::RunCommand { command, args } => {
                    lines.push(format!("  auto-fix: {command} {}", args.join(" ")));
                }
                DoctorFix::Manual { instructions } => {
                    lines.push(format!("  manual-fix: {instructions}"));
                }
            }
        }
    }

    lines.join("\n")
}

/// Apply a `DoctorFix`, executing automated fixes or returning manual instructions.
///
/// Returns `Ok(output)` on success (with command stdout for `RunCommand`, or the
/// instructions text for `Manual`).  Returns `Err(message)` if a `RunCommand` fix
/// fails to execute or exits with a non-zero status.
pub fn apply_fix(fix: &DoctorFix) -> Result<String, String> {
    match fix {
        DoctorFix::Manual { instructions } => Ok(instructions.clone()),
        DoctorFix::RunCommand { command, args } => {
            let output = Command::new(command)
                .args(args)
                .output()
                .map_err(|error| format!("failed to spawn `{command}`: {error}"))?;

            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();

            if output.status.success() {
                Ok(if stdout.is_empty() { stderr } else { stdout })
            } else {
                Err(if stderr.is_empty() {
                    format!("`{command}` exited with status {}", output.status)
                } else {
                    stderr
                })
            }
        }
    }
}

fn failing_doctor_fix(id: DoctorCheckId, detail: Option<&str>) -> Option<DoctorFix> {
    match id {
        DoctorCheckId::GhInstalled => Some(DoctorFix::Manual {
            instructions: "Install gh from https://cli.github.com".to_string(),
        }),
        DoctorCheckId::CodexInstalled => Some(DoctorFix::Manual {
            instructions: "Install codex from https://openai.com/codex".to_string(),
        }),
        DoctorCheckId::ClaudeInstalled => Some(DoctorFix::Manual {
            instructions: "Install claude from https://claude.ai/code".to_string(),
        }),
        DoctorCheckId::GhAuthenticated => Some(DoctorFix::RunCommand {
            command: "gh".to_string(),
            args: vec!["auth".to_string(), "login".to_string()],
        }),
        DoctorCheckId::GithubRemoteConfigured => Some(DoctorFix::Manual {
            instructions:
                "Add a GitHub remote: git remote add origin https://github.com/org/repo.git"
                    .to_string(),
        }),
        DoctorCheckId::RequiredWorkflowFilesPresent => Some(DoctorFix::Manual {
            instructions: format!(
                "Run calypso init to scaffold required GitHub Actions workflows (missing: {})",
                detail.unwrap_or("unknown")
            ),
        }),
    }
}

fn failing_fix(id: DoctorCheckId, detail: Option<&str>) -> Option<String> {
    match id {
        DoctorCheckId::GhInstalled => {
            Some("Install GitHub CLI and ensure `gh` is available on PATH.".to_string())
        }
        DoctorCheckId::CodexInstalled => {
            Some("Install Codex CLI and ensure `codex` is available on PATH.".to_string())
        }
        DoctorCheckId::ClaudeInstalled => {
            Some("Install Claude CLI and ensure `claude` is available on PATH. Set ANTHROPIC_API_KEY to authenticate.".to_string())
        }
        DoctorCheckId::GhAuthenticated => {
            Some("Run `gh auth login` and confirm the active account can access this repository.".to_string())
        }
        DoctorCheckId::GithubRemoteConfigured => Some(
            "Add a GitHub remote such as `git remote add origin git@github.com:<owner>/<repo>.git`."
                .to_string(),
        ),
        DoctorCheckId::RequiredWorkflowFilesPresent => Some(format!(
            "Add the missing workflow files under .github/workflows: {}",
            detail.unwrap_or("unknown")
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("calypso-doctor-{label}-{nanos}"));
        std::fs::create_dir_all(path.join(".github/workflows"))
            .expect("workflow dir should be created");
        path
    }

    #[test]
    fn host_environment_reports_missing_required_workflow_files() {
        let repo_root = unique_temp_dir("missing-workflows");
        std::fs::write(
            repo_root.join(".github/workflows/rust-quality.yml"),
            "name: quality\n",
        )
        .expect("workflow file should be written");

        let missing = HostDoctorEnvironment.missing_workflow_files(&repo_root);

        assert_eq!(
            missing,
            vec![
                "release-cli.yml".to_string(),
                "rust-coverage.yml".to_string(),
                "rust-e2e.yml".to_string(),
                "rust-integration.yml".to_string(),
                "rust-unit.yml".to_string(),
            ]
        );

        std::fs::remove_dir_all(repo_root).expect("temp dir should be removed");
    }
}
