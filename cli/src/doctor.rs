use std::path::Path;
use std::process::Command;

use crate::state::BuiltinEvidence;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DoctorCheckId {
    GhInstalled,
    CodexInstalled,
    GhAuthenticated,
    GithubRemoteConfigured,
}

impl DoctorCheckId {
    fn builtin_key(self) -> &'static str {
        match self {
            DoctorCheckId::GhInstalled => "builtin.doctor.gh_installed",
            DoctorCheckId::CodexInstalled => "builtin.doctor.codex_installed",
            DoctorCheckId::GhAuthenticated => "builtin.doctor.gh_authenticated",
            DoctorCheckId::GithubRemoteConfigured => "builtin.doctor.github_remote_configured",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorStatus {
    Passing,
    Failing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCheck {
    pub id: DoctorCheckId,
    pub status: DoctorStatus,
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
}

pub fn collect_doctor_report(
    environment: &impl DoctorEnvironment,
    repo_root: &Path,
) -> DoctorReport {
    DoctorReport {
        checks: vec![
            DoctorCheck {
                id: DoctorCheckId::GhInstalled,
                status: status_from_bool(environment.command_exists("gh")),
            },
            DoctorCheck {
                id: DoctorCheckId::CodexInstalled,
                status: status_from_bool(environment.command_exists("codex")),
            },
            DoctorCheck {
                id: DoctorCheckId::GhAuthenticated,
                status: status_from_bool(environment.gh_authenticated()),
            },
            DoctorCheck {
                id: DoctorCheckId::GithubRemoteConfigured,
                status: status_from_bool(environment.has_github_remote(repo_root)),
            },
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

pub fn git_remote_output_has_github_remote(output: &str) -> bool {
    output.lines().any(|line| {
        line.contains("github.com/")
            || line.contains("github.com:")
            || line.contains("git@github.com")
    })
}
