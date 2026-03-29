use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use nightshift_core::doctor::{
    DoctorCheckId, DoctorCheckScope, DoctorEnvironment, DoctorStatus, collect_doctor_report,
};

#[derive(Default)]
struct FakeEnvironment {
    is_git: bool,
    commands: BTreeSet<String>,
    claude_reachable: bool,
    gh_authenticated: bool,
    github_remote_roots: BTreeSet<PathBuf>,
    missing_workflow_files: BTreeMap<PathBuf, Vec<String>>,
    missing_git_hooks: BTreeMap<PathBuf, Vec<String>>,
    git_hooks_path: Option<PathBuf>,
    git_hooks_path_configured: bool,
    github_user: Option<String>,
}

impl FakeEnvironment {
    fn with_git(mut self) -> Self {
        self.is_git = true;
        self
    }

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

    fn with_missing_workflow_files(mut self, root: &Path, files: &[&str]) -> Self {
        self.missing_workflow_files.insert(
            root.to_path_buf(),
            files.iter().map(|file| file.to_string()).collect(),
        );
        self
    }

    fn with_missing_git_hooks(mut self, root: &Path, hooks: &[&str]) -> Self {
        self.missing_git_hooks.insert(
            root.to_path_buf(),
            hooks.iter().map(|h| h.to_string()).collect(),
        );
        self
    }

    fn with_git_hooks_path(mut self, path: &Path) -> Self {
        self.git_hooks_path = Some(path.to_path_buf());
        self
    }

    fn with_git_hooks_path_configured(mut self, configured: bool) -> Self {
        self.git_hooks_path_configured = configured;
        self
    }

    fn with_github_user(mut self, user: &str) -> Self {
        self.github_user = Some(user.to_string());
        self
    }
}

impl DoctorEnvironment for FakeEnvironment {
    fn is_git_repo(&self, _repo_root: &Path) -> bool {
        self.is_git
    }

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

    fn missing_workflow_files(&self, repo_root: &Path) -> Vec<String> {
        self.missing_workflow_files
            .get(repo_root)
            .cloned()
            .unwrap_or_default()
    }

    fn github_user(&self) -> Option<String> {
        self.github_user.clone()
    }

    fn missing_git_hooks(&self, repo_root: &Path) -> Vec<String> {
        self.missing_git_hooks
            .get(repo_root)
            .cloned()
            .unwrap_or_default()
    }

    fn git_hooks_path(&self, _repo_root: &Path) -> Option<PathBuf> {
        self.git_hooks_path.clone()
    }

    fn git_hooks_path_configured(&self, _repo_root: &Path) -> bool {
        self.git_hooks_path_configured
    }
}

fn status_map(
    report: &nightshift_core::doctor::DoctorReport,
) -> BTreeMap<DoctorCheckId, DoctorStatus> {
    report
        .checks
        .iter()
        .map(|check| (check.id, check.status))
        .collect()
}

fn check_for(
    report: &nightshift_core::doctor::DoctorReport,
    id: DoctorCheckId,
) -> &nightshift_core::doctor::DoctorCheck {
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
            .with_git()
            .with_command("gh")
            .with_command("codex")
            .with_gh_authenticated(true)
            .with_git_hooks_path_configured(true)
            .with_github_remote_root(repo_root),
        repo_root,
    );

    let statuses = status_map(&report);

    assert_eq!(
        statuses[&DoctorCheckId::GitInitialized],
        DoctorStatus::Passing
    );
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
        statuses[&DoctorCheckId::RequiredWorkflowFilesPresent],
        DoctorStatus::Passing
    );
    assert_eq!(
        statuses[&DoctorCheckId::GitHooksPathConfigured],
        DoctorStatus::Passing
    );
}

#[test]
fn doctor_report_marks_missing_dependencies_and_remote_checks_as_failing() {
    let report = collect_doctor_report(&FakeEnvironment::default(), Path::new("/tmp/calypso"));
    let statuses = status_map(&report);

    assert_eq!(
        statuses[&DoctorCheckId::GitInitialized],
        DoctorStatus::Failing
    );
    assert_eq!(statuses[&DoctorCheckId::GhInstalled], DoctorStatus::Failing);
    assert_eq!(
        statuses[&DoctorCheckId::CodexInstalled],
        DoctorStatus::Warning
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
        statuses[&DoctorCheckId::RequiredWorkflowFilesPresent],
        DoctorStatus::Passing
    );
}

#[test]
fn doctor_report_converts_check_results_into_builtin_evidence() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_git()
            .with_command("gh")
            .with_gh_authenticated(true)
            .with_github_remote_root(repo_root),
        repo_root,
    );

    let evidence = report.to_builtin_evidence();

    assert_eq!(
        evidence.result_for("builtin.doctor.git_initialized"),
        Some(true)
    );
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
        evidence.result_for("builtin.doctor.required_workflows_present"),
        Some(true)
    );
}

#[test]
fn doctor_report_labels_external_auth_failures_separately_from_local_setup_failures() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_command("gh")
            .with_command("codex")
            .with_github_remote_root(repo_root),
        repo_root,
    );

    assert_eq!(
        check_for(&report, DoctorCheckId::GhAuthenticated).scope,
        DoctorCheckScope::ExternalAuth
    );
    assert_eq!(
        check_for(&report, DoctorCheckId::GithubRemoteConfigured).scope,
        DoctorCheckScope::LocalConfiguration
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

    let rendered = nightshift_core::doctor::render_doctor_report(&report);

    assert!(rendered.contains("required-workflows-present"));
    assert!(rendered.contains(
        "Missing workflow files will be written and pushed: release-cli.yml, rust-quality.yml"
    ));
}

#[test]
fn doctor_fix_is_populated_for_failing_checks() {
    use nightshift_core::doctor::DoctorFix;

    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(&FakeEnvironment::default(), repo_root);

    for check in &report.checks {
        if check.status == nightshift_core::doctor::DoctorStatus::Passing {
            assert!(
                check.fix.is_none(),
                "passing check {:?} should not have a fix",
                check.id
            );
        } else {
            // Failing and Warning checks should have a fix (except
            // RequiredGitHooksInstalled without a hooks path).
            if check.id == DoctorCheckId::RequiredGitHooksInstalled {
                continue;
            }
            assert!(
                check.fix.is_some(),
                "non-passing check {:?} ({:?}) should have a fix",
                check.id,
                check.status
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

    // GitInitialized should have an auto RunCommand fix
    let git_init = check_for(&report, DoctorCheckId::GitInitialized);
    assert!(
        matches!(&git_init.fix, Some(DoctorFix::RunCommand { command, .. }) if command == "git")
    );

    // GhInstalled should have a Manual fix
    let gh_installed = check_for(&report, DoctorCheckId::GhInstalled);
    assert!(matches!(gh_installed.fix, Some(DoctorFix::Manual { .. })));
}

#[test]
fn apply_fix_returns_instructions_for_manual_fix() {
    use nightshift_core::doctor::{DoctorFix, apply_fix};

    let fix = DoctorFix::Manual {
        instructions: "Install gh from https://cli.github.com".to_string(),
    };

    let result = apply_fix(&fix, Path::new("/tmp"));

    assert_eq!(
        result,
        Ok("Install gh from https://cli.github.com".to_string())
    );
}

#[test]
fn doctor_github_remote_fix_uses_gh_user_and_dirname_when_user_is_known() {
    use nightshift_core::doctor::DoctorFix;

    let repo_root = Path::new("/tmp/myproject");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_git()
            .with_gh_authenticated(true)
            .with_github_user("acme"),
        repo_root,
    );

    let check = check_for(&report, DoctorCheckId::GithubRemoteConfigured);
    assert!(
        matches!(&check.fix, Some(DoctorFix::RunCommand { command, args })
            if command == "gh" && args.iter().any(|a| a == "acme/myproject")),
        "fix should contain acme/myproject slug: {:?}",
        check.fix
    );
}

#[test]
fn doctor_github_remote_fix_falls_back_to_manual_when_no_user() {
    use nightshift_core::doctor::DoctorFix;

    let repo_root = Path::new("/tmp/myproject");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_git()
            .with_gh_authenticated(true),
        repo_root,
    );

    let check = check_for(&report, DoctorCheckId::GithubRemoteConfigured);
    assert!(
        matches!(&check.fix, Some(DoctorFix::Manual { .. })),
        "should fall back to Manual when no gh user: {:?}",
        check.fix
    );
}

#[test]
fn doctor_workflow_fix_is_a_sequence_with_write_and_git_steps() {
    use nightshift_core::doctor::DoctorFix;

    let repo_root = Path::new("/tmp/myproject");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_git()
            .with_missing_workflow_files(repo_root, &["rust-unit.yml"]),
        repo_root,
    );

    let check = check_for(&report, DoctorCheckId::RequiredWorkflowFilesPresent);
    assert!(
        matches!(&check.fix, Some(DoctorFix::Sequence(_))),
        "workflow fix should be a Sequence: {:?}",
        check.fix
    );
    if let Some(DoctorFix::Sequence(steps)) = &check.fix {
        assert!(
            steps
                .iter()
                .any(|s| matches!(s, DoctorFix::WriteFile { .. })),
            "sequence should include WriteFile steps"
        );
        assert!(
            steps
                .iter()
                .any(|s| matches!(s, DoctorFix::RunCommand { command, .. } if command == "git")),
            "sequence should include git commands"
        );
    }
}

#[test]
fn render_doctor_report_verbose_shows_auto_fix_for_gh_auth() {
    use nightshift_core::doctor::render_doctor_report_verbose;

    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(&FakeEnvironment::default(), repo_root);

    let rendered = render_doctor_report_verbose(&report);

    assert!(rendered.contains("auto-fix: gh auth login"));
}

#[test]
fn doctor_report_marks_missing_git_hooks_as_failing() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_missing_git_hooks(repo_root, &["pre-commit", "commit-msg"])
            .with_git_hooks_path(Path::new("/tmp/calypso/.git/hooks")),
        repo_root,
    );
    let statuses = status_map(&report);

    assert_eq!(
        statuses[&DoctorCheckId::RequiredGitHooksInstalled],
        DoctorStatus::Failing
    );

    let check = check_for(&report, DoctorCheckId::RequiredGitHooksInstalled);
    assert_eq!(check.detail.as_deref(), Some("commit-msg, pre-commit"));
    assert!(check.remediation.is_some());
    assert!(
        check
            .remediation
            .as_ref()
            .unwrap()
            .contains("scripts/hooks/")
    );
}

#[test]
fn doctor_report_marks_git_hooks_as_passing_when_none_missing() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default().with_git_hooks_path(Path::new("/tmp/calypso/.git/hooks")),
        repo_root,
    );
    let statuses = status_map(&report);

    assert_eq!(
        statuses[&DoctorCheckId::RequiredGitHooksInstalled],
        DoctorStatus::Passing
    );
}

#[test]
fn codex_installed_uses_warning_severity_when_missing() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_git()
            .with_command("gh")
            .with_gh_authenticated(true)
            .with_github_remote_root(repo_root),
        repo_root,
    );

    let codex = check_for(&report, DoctorCheckId::CodexInstalled);
    assert_eq!(codex.status, DoctorStatus::Warning);
    assert!(
        codex.remediation.is_some(),
        "warning check should still have remediation"
    );
}

#[test]
fn doctor_report_has_failures_excludes_warnings() {
    let repo_root = Path::new("/tmp/calypso");
    // Build a report where all required checks pass but codex (advisory) is missing.
    let report = collect_doctor_report(
        &FakeEnvironment::default()
            .with_git()
            .with_command("gh")
            .with_gh_authenticated(true)
            .with_github_remote_root(repo_root)
            .with_git_hooks_path(Path::new("/tmp/calypso/.git/hooks")),
        repo_root,
    );

    // CodexInstalled and ClaudeInstalled are the only non-passing.
    // CodexInstalled is advisory (Warning), ClaudeInstalled is required (Failing).
    assert!(report.has_failures(), "claude-installed should be failing");
    assert!(report.has_warnings(), "codex-installed should be warning");
}

#[test]
fn doctor_report_render_shows_warn_for_advisory_checks() {
    let repo_root = Path::new("/tmp/calypso");
    let report = collect_doctor_report(&FakeEnvironment::default(), repo_root);
    let rendered = nightshift_core::doctor::render_doctor_report(&report);

    assert!(
        rendered.contains("[WARN] codex-installed"),
        "advisory check should render as WARN"
    );
    assert!(
        rendered.contains("[FAIL] gh-installed"),
        "required check should render as FAIL"
    );
}

// ── LocalWorkflowLayout doctor check ─────────────────────────────────────────

fn unique_temp_dir(label: &str) -> PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let path = std::env::temp_dir().join(format!("calypso-doctor-layout-{label}-{nanos}"));
    std::fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

#[test]
fn doctor_local_workflow_layout_passes_when_no_legacy_files() {
    // A repo with no .calypso/ dir — check must pass (no legacy files).
    let tmp = unique_temp_dir("no-legacy");
    let report = collect_doctor_report(&FakeEnvironment::default(), &tmp);
    let statuses = status_map(&report);
    assert_eq!(
        statuses[&DoctorCheckId::LocalWorkflowLayout],
        DoctorStatus::Passing,
        "expected Passing when no .calypso/ dir"
    );
    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn doctor_local_workflow_layout_warns_when_legacy_files_present() {
    // A repo with .calypso/my-workflow.yml placed at the root level (legacy layout).
    let tmp = unique_temp_dir("has-legacy");
    let calypso_dir = tmp.join(".calypso");
    std::fs::create_dir_all(&calypso_dir).expect(".calypso dir");
    std::fs::write(calypso_dir.join("my-workflow.yml"), "name: my-workflow").unwrap();

    let report = collect_doctor_report(&FakeEnvironment::default(), &tmp);
    let statuses = status_map(&report);
    assert_eq!(
        statuses[&DoctorCheckId::LocalWorkflowLayout],
        DoctorStatus::Warning,
        "expected Warning when legacy workflow files are present"
    );

    // Detail must mention the legacy file.
    let check = check_for(&report, DoctorCheckId::LocalWorkflowLayout);
    let detail = check.detail.as_deref().unwrap_or("");
    assert!(
        detail.contains("my-workflow.yml"),
        "expected legacy filename in detail: {detail}"
    );

    // Remediation must include migration guidance.
    let remediation = check.remediation.as_deref().unwrap_or("");
    assert!(
        remediation.contains(".calypso/workflows/"),
        "expected migration path in remediation: {remediation}"
    );

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn doctor_local_workflow_layout_passes_when_workflows_in_correct_subdir() {
    // A repo with .calypso/workflows/my-workflow.yml — correct layout, must pass.
    let tmp = unique_temp_dir("correct-layout");
    let workflows_dir = tmp.join(".calypso").join("workflows");
    std::fs::create_dir_all(&workflows_dir).expect("workflows dir");
    std::fs::write(workflows_dir.join("my-workflow.yml"), "name: my-workflow").unwrap();

    let report = collect_doctor_report(&FakeEnvironment::default(), &tmp);
    let statuses = status_map(&report);
    assert_eq!(
        statuses[&DoctorCheckId::LocalWorkflowLayout],
        DoctorStatus::Passing,
        "expected Passing when workflows are in .calypso/workflows/"
    );

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn doctor_local_workflow_layout_passes_for_known_non_workflow_calypso_files() {
    // Known config files like state-machine.yml, agents.yml, prompts.yml must
    // not be flagged as legacy workflow files.
    let tmp = unique_temp_dir("known-files");
    let calypso_dir = tmp.join(".calypso");
    std::fs::create_dir_all(&calypso_dir).expect(".calypso dir");
    for filename in &[
        "state-machine.yml",
        "agents.yml",
        "prompts.yml",
        "headless-state.json",
    ] {
        std::fs::write(calypso_dir.join(filename), "content").unwrap();
    }

    let report = collect_doctor_report(&FakeEnvironment::default(), &tmp);
    let statuses = status_map(&report);
    assert_eq!(
        statuses[&DoctorCheckId::LocalWorkflowLayout],
        DoctorStatus::Passing,
        "expected Passing — known config files must not be flagged"
    );

    std::fs::remove_dir_all(&tmp).ok();
}

#[test]
fn doctor_local_workflow_layout_renders_as_warn_in_report() {
    let tmp = unique_temp_dir("render-warn");
    let calypso_dir = tmp.join(".calypso");
    std::fs::create_dir_all(&calypso_dir).expect(".calypso dir");
    std::fs::write(calypso_dir.join("legacy.yml"), "name: legacy").unwrap();

    let report = collect_doctor_report(&FakeEnvironment::default(), &tmp);
    let rendered = nightshift_core::doctor::render_doctor_report(&report);
    assert!(
        rendered.contains("[WARN] local-workflow-layout"),
        "expected WARN for local-workflow-layout in rendered output: {rendered}"
    );

    std::fs::remove_dir_all(&tmp).ok();
}
