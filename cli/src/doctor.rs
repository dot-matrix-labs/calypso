use std::path::Path;
use std::process::Command;

use crate::runtime::discover_current_repository_context;
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
    GhAuthenticated,
    GithubRemoteConfigured,
    FeatureBindingResolved,
    RequiredWorkflowFilesPresent,
}

impl DoctorCheckId {
    fn builtin_key(self) -> &'static str {
        match self {
            DoctorCheckId::GhInstalled => "builtin.doctor.gh_installed",
            DoctorCheckId::CodexInstalled => "builtin.doctor.codex_installed",
            DoctorCheckId::GhAuthenticated => "builtin.doctor.gh_authenticated",
            DoctorCheckId::GithubRemoteConfigured => "builtin.doctor.github_remote_configured",
            DoctorCheckId::FeatureBindingResolved => "builtin.doctor.feature_binding_resolved",
            DoctorCheckId::RequiredWorkflowFilesPresent => {
                "builtin.doctor.required_workflows_present"
            }
        }
    }

    fn label(self) -> &'static str {
        match self {
            DoctorCheckId::GhInstalled => "gh-installed",
            DoctorCheckId::CodexInstalled => "codex-installed",
            DoctorCheckId::GhAuthenticated => "gh-authenticated",
            DoctorCheckId::GithubRemoteConfigured => "github-remote-configured",
            DoctorCheckId::FeatureBindingResolved => "feature-binding-resolved",
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCheck {
    pub id: DoctorCheckId,
    pub scope: DoctorCheckScope,
    pub status: DoctorStatus,
    pub detail: Option<String>,
    pub remediation: Option<String>,
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
    fn gh_authenticated(&self) -> bool;
    fn has_github_remote(&self, repo_root: &Path) -> bool;
    fn feature_binding_status(&self, repo_root: &Path) -> Result<(), String>;
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

    fn gh_authenticated(&self) -> bool {
        Command::new("gh")
            .args(["auth", "status"])
            .output()
            .is_ok_and(|output| output.status.success())
    }

    fn has_github_remote(&self, repo_root: &Path) -> bool {
        Command::new("git")
            .args(["remote", "-v"])
            .current_dir(repo_root)
            .output()
            .ok()
            .filter(|output| output.status.success())
            .is_some_and(|output| {
                git_remote_output_has_github_remote(&String::from_utf8_lossy(&output.stdout))
            })
    }

    fn feature_binding_status(&self, repo_root: &Path) -> Result<(), String> {
        discover_current_repository_context(repo_root)
            .map(|_| ())
            .map_err(|error| error.to_string())
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
    let feature_binding_status = environment.feature_binding_status(repo_root);
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
                DoctorCheckId::FeatureBindingResolved,
                DoctorCheckScope::LocalConfiguration,
                feature_binding_status.is_ok(),
                feature_binding_status.err(),
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

    DoctorCheck {
        id,
        scope,
        status,
        detail,
        remediation,
    }
}

pub fn git_remote_output_has_github_remote(output: &str) -> bool {
    output.lines().any(|line| {
        line.contains("github.com/")
            || line.contains("github.com:")
            || line.contains("git@github.com")
    })
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

fn failing_fix(id: DoctorCheckId, detail: Option<&str>) -> Option<String> {
    match id {
        DoctorCheckId::GhInstalled => {
            Some("Install GitHub CLI and ensure `gh` is available on PATH.".to_string())
        }
        DoctorCheckId::CodexInstalled => {
            Some("Install Codex CLI and ensure `codex` is available on PATH.".to_string())
        }
        DoctorCheckId::GhAuthenticated => {
            Some("Run `gh auth login` and confirm the active account can access this repository.".to_string())
        }
        DoctorCheckId::GithubRemoteConfigured => Some(
            "Add a GitHub remote such as `git remote add origin git@github.com:<owner>/<repo>.git`."
                .to_string(),
        ),
        DoctorCheckId::FeatureBindingResolved => Some(
            "Ensure this worktree is on a feature branch with an open GitHub pull request."
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
