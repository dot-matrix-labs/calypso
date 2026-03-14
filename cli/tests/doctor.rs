use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use calypso_cli::doctor::{
    DoctorCheckId, DoctorCheckScope, DoctorEnvironment, DoctorStatus, collect_doctor_report,
    git_remote_output_has_github_remote,
};

#[derive(Default)]
struct FakeEnvironment {
    commands: BTreeSet<String>,
    claude_reachable: bool,
    gh_authenticated: bool,
    github_remote_roots: BTreeSet<PathBuf>,
    feature_binding_roots: BTreeSet<PathBuf>,
    feature_binding_errors: BTreeMap<PathBuf, String>,
    missing_workflow_files: BTreeMap<PathBuf, Vec<String>>,
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

    fn with_feature_binding_root(mut self, root: &Path) -> Self {
        self.feature_binding_roots.insert(root.to_path_buf());
        self
    }

    fn with_feature_binding_error(mut self, root: &Path, detail: &str) -> Self {
        self.feature_binding_errors
            .insert(root.to_path_buf(), detail.to_string());
        self
    }

    fn with_missing_workflow_files(mut self, root: &Path, files: &[&str]) -> Self {
        self.missing_workflow_files.insert(
            root.to_path_buf(),
            files.iter().map(|file| file.to_string()).collect(),
        );
        self
    }
}

impl DoctorEnvironment for FakeEnvironment {
    fn command_exists(&self, command: &str) -> bool {
        self.commands.contains(command)
    }

    fn claude_reachable(&self) -> bool {
        self.claude_reachable
    }

    fn gh_authenticated(&self) -> bool {
        self.gh_authenticated
    }

    fn has_github_remote(&self, repo_root: &Path) -> bool {
        self.github_remote_roots.contains(repo_root)
    }

    fn feature_binding_status(&self, repo_root: &Path) -> Result<(), String> {
        if self.feature_binding_roots.contains(repo_root) {
            return Ok(());
        }

        Err(self
            .feature_binding_errors
            .get(repo_root)
            .cloned()
            .unwrap_or_else(|| "branch is not mapped to an open pull request".to_string()))
    }

    fn missing_workflow_files(&self, repo_root: &Path) -> Vec<String> {
        self.missing_workflow_files
            .get(repo_root)
            .cloned()
            .unwrap_or_default()
    }
}

fn status_map(report: &calypso_cli::doctor::DoctorReport) -> BTreeMap<DoctorCheckId, DoctorStatus> {
    report
        .checks
        .iter()
        .map(|check| (check.id, check.status))
        .collect()
}

fn check_for(
    report: &calypso_cli::doctor::DoctorReport,
    id: DoctorCheckId,
) -> &calypso_cli::doctor::DoctorCheck {
    report
        .checks
        .iter()
        .find(|check| check.id == id)
        .expect("check should exist")
}

#[test]
fn doctor_report_collects_expected_check_results() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_command("gh")
            .with_command("codex")
            .with_gh_authenticated(true)
            .with_github_remote_root(repo_root)
            .with_feature_binding_root(repo_root),
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
    assert_eq!(
        statuses[&DoctorCheckId::FeatureBindingResolved],
        DoctorStatus::Passing
    );
    assert_eq!(
        statuses[&DoctorCheckId::RequiredWorkflowFilesPresent],
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
    assert_eq!(
        statuses[&DoctorCheckId::FeatureBindingResolved],
        DoctorStatus::Failing
    );
    assert_eq!(
        statuses[&DoctorCheckId::RequiredWorkflowFilesPresent],
        DoctorStatus::Passing
    );
}

#[test]
fn doctor_report_converts_check_results_into_builtin_evidence() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_command("gh")
            .with_gh_authenticated(true)
            .with_github_remote_root(repo_root)
            .with_feature_binding_root(repo_root),
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
    assert_eq!(
        evidence.result_for("builtin.doctor.feature_binding_resolved"),
        Some(true)
    );
    assert_eq!(
        evidence.result_for("builtin.doctor.required_workflows_present"),
        Some(true)
    );
}

#[test]
fn doctor_report_marks_missing_feature_binding_as_failing() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default().with_feature_binding_error(
            repo_root,
            "current branch is not mapped to an open pull request",
        ),
        repo_root,
    );
    let statuses = status_map(&report);

    assert_eq!(
        statuses[&DoctorCheckId::FeatureBindingResolved],
        DoctorStatus::Failing
    );
}

#[test]
fn doctor_report_labels_external_auth_failures_separately_from_local_setup_failures() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_command("gh")
            .with_command("codex")
            .with_github_remote_root(repo_root)
            .with_feature_binding_root(repo_root),
        repo_root,
    );

    assert_eq!(
        check_for(&report, DoctorCheckId::GhAuthenticated).scope,
        DoctorCheckScope::ExternalAuth
    );
    assert_eq!(
        check_for(&report, DoctorCheckId::FeatureBindingResolved).scope,
        DoctorCheckScope::LocalConfiguration
    );
}

#[test]
fn doctor_report_exposes_actionable_remediation_in_the_result_model() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default().with_feature_binding_error(
            repo_root,
            "current branch is not mapped to an open pull request",
        ),
        repo_root,
    );

    assert_eq!(
        check_for(&report, DoctorCheckId::FeatureBindingResolved)
            .remediation
            .as_deref(),
        Some("Ensure this worktree is on a feature branch with an open GitHub pull request.")
    );
}

#[test]
fn doctor_report_marks_missing_required_workflows_as_failing() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_missing_workflow_files(repo_root, &["rust-quality.yml", "rust-unit.yml"]),
        repo_root,
    );
    let statuses = status_map(&report);

    assert_eq!(
        statuses[&DoctorCheckId::RequiredWorkflowFilesPresent],
        DoctorStatus::Failing
    );
}

#[test]
fn doctor_report_render_includes_actionable_fix_for_missing_workflows() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_missing_workflow_files(repo_root, &["rust-quality.yml", "release-cli.yml"]),
        repo_root,
    );

    let rendered = calypso_cli::doctor::render_doctor_report(&report);

    assert!(rendered.contains("required-workflows-present"));
    assert!(rendered.contains(
        "Add the missing workflow files under .github/workflows: release-cli.yml, rust-quality.yml"
    ));
}

#[test]
fn doctor_report_render_includes_actionable_fix_for_missing_feature_binding() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default().with_feature_binding_error(
            repo_root,
            "current branch is not mapped to an open pull request",
        ),
        repo_root,
    );

    let rendered = calypso_cli::doctor::render_doctor_report(&report);

    assert!(rendered.contains("feature-binding-resolved"));
    assert!(
        rendered.contains(
            "Ensure this worktree is on a feature branch with an open GitHub pull request."
        )
    );
    assert!(rendered.contains("current branch is not mapped to an open pull request"));
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

#[test]
fn doctor_fix_is_populated_for_failing_checks() {
    use calypso_cli::doctor::DoctorFix;

    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(&FakeEnvironment::default(), repo_root);

    for check in &report.checks {
        if check.status == calypso_cli::doctor::DoctorStatus::Failing {
            assert!(
                check.fix.is_some(),
                "failing check {:?} should have a fix",
                check.id
            );
        } else {
            assert!(
                check.fix.is_none(),
                "passing check {:?} should not have a fix",
                check.id
            );
        }
    }

    // GhAuthenticated should have a RunCommand fix (automated)
    let gh_auth = check_for(&report, DoctorCheckId::GhAuthenticated);
    assert_eq!(
        gh_auth.fix,
        Some(DoctorFix::RunCommand {
            command: "gh".to_string(),
            args: vec!["auth".to_string(), "login".to_string()],
        })
    );

    // GhInstalled should have a Manual fix
    let gh_installed = check_for(&report, DoctorCheckId::GhInstalled);
    assert!(matches!(gh_installed.fix, Some(DoctorFix::Manual { .. })));
}

#[test]
fn apply_fix_returns_instructions_for_manual_fix() {
    use calypso_cli::doctor::{DoctorFix, apply_fix};

    let fix = DoctorFix::Manual {
        instructions: "Install gh from https://cli.github.com".to_string(),
    };

    let result = apply_fix(&fix);

    assert_eq!(
        result,
        Ok("Install gh from https://cli.github.com".to_string())
    );
}

#[test]
fn render_doctor_report_verbose_includes_remediation_text() {
    use calypso_cli::doctor::render_doctor_report_verbose;

    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default().with_feature_binding_error(
            repo_root,
            "current branch is not mapped to an open pull request",
        ),
        repo_root,
    );

    let rendered = render_doctor_report_verbose(&report);

    assert!(rendered.contains("Doctor checks (verbose)"));
    assert!(rendered.contains("feature-binding-resolved"));
    assert!(rendered.contains("Ensure this worktree is on a feature branch"));
    assert!(rendered.contains("manual-fix:"));
}

#[test]
fn render_doctor_report_verbose_shows_auto_fix_for_gh_auth() {
    use calypso_cli::doctor::render_doctor_report_verbose;

    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(&FakeEnvironment::default(), repo_root);

    let rendered = render_doctor_report_verbose(&report);

    assert!(rendered.contains("auto-fix: gh auth login"));
}
