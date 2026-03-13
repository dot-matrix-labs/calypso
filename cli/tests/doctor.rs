use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use calypso_cli::doctor::{
    DoctorCheckId, DoctorEnvironment, DoctorStatus, collect_doctor_report,
    git_remote_output_has_github_remote,
};

#[derive(Default)]
struct FakeEnvironment {
    commands: BTreeSet<String>,
    gh_authenticated: bool,
    github_remote_roots: BTreeSet<PathBuf>,
}

impl FakeEnvironment {
    fn with_command(mut self, command: &str) -> Self {
        self.commands.insert(command.to_string());
        self
    }

    fn with_gh_authenticated(mut self, authenticated: bool) -> Self {
        self.gh_authenticated = authenticated;
        self
    }

    fn with_github_remote_root(mut self, root: &Path) -> Self {
        self.github_remote_roots.insert(root.to_path_buf());
        self
    }
}

impl DoctorEnvironment for FakeEnvironment {
    fn command_exists(&self, command: &str) -> bool {
        self.commands.contains(command)
    }

    fn gh_authenticated(&self) -> bool {
        self.gh_authenticated
    }

    fn has_github_remote(&self, repo_root: &Path) -> bool {
        self.github_remote_roots.contains(repo_root)
    }
}

fn status_map(report: &calypso_cli::doctor::DoctorReport) -> BTreeMap<DoctorCheckId, DoctorStatus> {
    report
        .checks
        .iter()
        .map(|check| (check.id, check.status))
        .collect()
}

#[test]
fn doctor_report_collects_expected_check_results() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_command("gh")
            .with_command("codex")
            .with_gh_authenticated(true)
            .with_github_remote_root(repo_root),
        repo_root,
    );

    let statuses = status_map(&report);

    assert_eq!(statuses[&DoctorCheckId::GhInstalled], DoctorStatus::Passing);
    assert_eq!(
        statuses[&DoctorCheckId::CodexInstalled],
        DoctorStatus::Passing
    );
    assert_eq!(
        statuses[&DoctorCheckId::GhAuthenticated],
        DoctorStatus::Passing
    );
    assert_eq!(
        statuses[&DoctorCheckId::GithubRemoteConfigured],
        DoctorStatus::Passing
    );
}

#[test]
fn doctor_report_marks_missing_dependencies_and_remote_checks_as_failing() {
    let report = collect_doctor_report(&FakeEnvironment::default(), Path::new("/tmp/calypso"));
    let statuses = status_map(&report);

    assert_eq!(statuses[&DoctorCheckId::GhInstalled], DoctorStatus::Failing);
    assert_eq!(
        statuses[&DoctorCheckId::CodexInstalled],
        DoctorStatus::Failing
    );
    assert_eq!(
        statuses[&DoctorCheckId::GhAuthenticated],
        DoctorStatus::Failing
    );
    assert_eq!(
        statuses[&DoctorCheckId::GithubRemoteConfigured],
        DoctorStatus::Failing
    );
}

#[test]
fn doctor_report_converts_check_results_into_builtin_evidence() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_command("gh")
            .with_gh_authenticated(true)
            .with_github_remote_root(repo_root),
        repo_root,
    );

    let evidence = report.to_builtin_evidence();

    assert_eq!(
        evidence.result_for("builtin.doctor.gh_installed"),
        Some(true)
    );
    assert_eq!(
        evidence.result_for("builtin.doctor.codex_installed"),
        Some(false)
    );
    assert_eq!(
        evidence.result_for("builtin.doctor.gh_authenticated"),
        Some(true)
    );
    assert_eq!(
        evidence.result_for("builtin.doctor.github_remote_configured"),
        Some(true)
    );
}

#[test]
fn git_remote_parser_detects_github_https_and_ssh_remotes() {
    assert!(git_remote_output_has_github_remote(
        "origin\thttps://github.com/acme/calypso.git (fetch)\n"
    ));
    assert!(git_remote_output_has_github_remote(
        "origin\tgit@github.com:acme/calypso.git (push)\n"
    ));
}

#[test]
fn git_remote_parser_rejects_non_github_remotes() {
    assert!(!git_remote_output_has_github_remote(
        "origin\thttps://gitlab.com/acme/calypso.git (fetch)\n"
    ));
    assert!(!git_remote_output_has_github_remote(""));
}
