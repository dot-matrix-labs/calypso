use std::path::{Path, PathBuf};
use std::process::Command;

use crate::claude::{ClaudeConfig, ClaudeSession};
use crate::sm_audit;
use crate::state::BuiltinEvidence;
use crate::workflows;

const REQUIRED_WORKFLOW_FILES: [&str; 6] = [
    "rust-quality.yml",
    "rust-unit.yml",
    "rust-integration.yml",
    "rust-e2e.yml",
    "rust-coverage.yml",
    "release-cli.yml",
];

const REQUIRED_GIT_HOOKS: [&str; 6] = [
    "commit-msg",
    "post-checkout",
    "post-commit",
    "pre-commit",
    "pre-push",
    "prepare-commit-msg",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DoctorCheckId {
    GitInitialized,
    GhInstalled,
    CodexInstalled,
    ClaudeInstalled,
    GhAuthenticated,
    GithubRemoteConfigured,
    RequiredWorkflowFilesPresent,
    RequiredGitHooksInstalled,
    StateMachineIntegrity,
}

/// Severity of a failing check — determines whether a non-passing result is
/// classified as a hard `Failing` or a softer `Warning`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckSeverity {
    /// The check is critical; failure blocks normal operation.
    Required,
    /// The check is advisory; failure is surfaced but does not block.
    Advisory,
}

impl DoctorCheckId {
    /// Returns the severity of this check when it is not passing.
    pub fn severity(self) -> CheckSeverity {
        match self {
            // Optional tooling — nice-to-have, not required for core workflow.
            DoctorCheckId::CodexInstalled => CheckSeverity::Advisory,
            // All other checks are hard requirements.
            _ => CheckSeverity::Required,
        }
    }

    fn builtin_key(self) -> &'static str {
        match self {
            DoctorCheckId::GitInitialized => "builtin.doctor.git_initialized",
            DoctorCheckId::GhInstalled => "builtin.doctor.gh_installed",
            DoctorCheckId::CodexInstalled => "builtin.doctor.codex_installed",
            DoctorCheckId::ClaudeInstalled => "builtin.doctor.claude_installed",
            DoctorCheckId::GhAuthenticated => "builtin.doctor.gh_authenticated",
            DoctorCheckId::GithubRemoteConfigured => "builtin.doctor.github_remote_configured",
            DoctorCheckId::RequiredWorkflowFilesPresent => {
                "builtin.doctor.required_workflows_present"
            }
            DoctorCheckId::RequiredGitHooksInstalled => {
                "builtin.doctor.required_git_hooks_installed"
            }
            DoctorCheckId::StateMachineIntegrity => "builtin.doctor.sm_integrity",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            DoctorCheckId::GitInitialized => "git-initialized",
            DoctorCheckId::GhInstalled => "gh-installed",
            DoctorCheckId::CodexInstalled => "codex-installed",
            DoctorCheckId::ClaudeInstalled => "claude-installed",
            DoctorCheckId::GhAuthenticated => "gh-authenticated",
            DoctorCheckId::GithubRemoteConfigured => "github-remote-configured",
            DoctorCheckId::RequiredWorkflowFilesPresent => "required-workflows-present",
            DoctorCheckId::RequiredGitHooksInstalled => "required-git-hooks-installed",
            DoctorCheckId::StateMachineIntegrity => "state-machine-integrity",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoctorStatus {
    Passing,
    Warning,
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
    /// Execute a single command to resolve the issue.
    RunCommand { command: String, args: Vec<String> },
    /// Write a file at the given absolute path with the given content.
    WriteFile { path: PathBuf, content: String },
    /// Run multiple fix steps in order; stops on first failure.
    Sequence(Vec<DoctorFix>),
    /// Provide human-readable instructions only; no automated action available.
    Manual { instructions: String },
}

impl DoctorFix {
    /// Returns `true` for all variants that can be applied automatically.
    pub fn is_automatic(&self) -> bool {
        !matches!(self, DoctorFix::Manual { .. })
    }
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

    /// Returns `true` if any check has a hard `Failing` status (excludes warnings).
    pub fn has_failures(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == DoctorStatus::Failing)
    }

    /// Returns `true` if any check has a `Warning` status.
    pub fn has_warnings(&self) -> bool {
        self.checks
            .iter()
            .any(|check| check.status == DoctorStatus::Warning)
    }
}

pub trait DoctorEnvironment {
    fn is_git_repo(&self, repo_root: &Path) -> bool;
    fn command_exists(&self, command: &str) -> bool;
    fn claude_reachable(&self) -> bool;
    fn gh_authenticated(&self) -> bool;
    fn has_github_remote(&self, repo_root: &Path) -> bool;
    fn missing_workflow_files(&self, repo_root: &Path) -> Vec<String>;
    /// Returns the GitHub username of the currently authenticated user, if available.
    fn github_user(&self) -> Option<String>;
    /// Returns the names of git hooks from `REQUIRED_GIT_HOOKS` that are missing,
    /// not executable, or have content that differs from `scripts/hooks/<name>`.
    fn missing_git_hooks(&self, repo_root: &Path) -> Vec<String>;
    /// Resolves the git hooks directory for the given repo root.
    fn git_hooks_path(&self, repo_root: &Path) -> Option<PathBuf>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HostDoctorEnvironment;

impl DoctorEnvironment for HostDoctorEnvironment {
    fn is_git_repo(&self, repo_root: &Path) -> bool {
        Command::new("git")
            .args(["-C", &repo_root.to_string_lossy(), "rev-parse", "--git-dir"])
            .output()
            .is_ok_and(|output| output.status.success())
    }

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

    fn github_user(&self) -> Option<String> {
        let output = Command::new("gh")
            .args(["api", "user", "--jq", ".login"])
            .output()
            .ok()?;
        if output.status.success() {
            let user = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if user.is_empty() { None } else { Some(user) }
        } else {
            None
        }
    }

    fn missing_git_hooks(&self, repo_root: &Path) -> Vec<String> {
        let hooks_dir = match self.git_hooks_path(repo_root) {
            Some(p) => p,
            None => {
                return REQUIRED_GIT_HOOKS
                    .iter()
                    .map(|h| (*h).to_string())
                    .collect();
            }
        };
        let source_dir = repo_root.join("scripts").join("hooks");

        let mut missing: Vec<String> = REQUIRED_GIT_HOOKS
            .iter()
            .filter(|hook| {
                let installed = hooks_dir.join(hook);
                let source = source_dir.join(hook);

                // Source must exist to compare against.
                let source_content = match std::fs::read(&source) {
                    Ok(c) => c,
                    Err(_) => return false, // no source → skip this hook
                };

                // Installed must exist.
                let installed_content = match std::fs::read(&installed) {
                    Ok(c) => c,
                    Err(_) => return true, // missing → report
                };

                // Must be executable (Unix).
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    if let Ok(meta) = std::fs::metadata(&installed)
                        && meta.permissions().mode() & 0o111 == 0
                    {
                        return true; // not executable → report
                    }
                }

                // Content must match.
                installed_content != source_content
            })
            .map(|h| (*h).to_string())
            .collect();
        missing.sort();
        missing
    }

    fn git_hooks_path(&self, repo_root: &Path) -> Option<PathBuf> {
        let output = Command::new("git")
            .args([
                "-C",
                &repo_root.to_string_lossy(),
                "rev-parse",
                "--git-path",
                "hooks",
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let raw = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
        if raw.is_absolute() {
            Some(raw)
        } else {
            Some(repo_root.join(raw))
        }
    }
}

pub fn collect_doctor_report(
    environment: &impl DoctorEnvironment,
    repo_root: &Path,
) -> DoctorReport {
    let is_git = environment.is_git_repo(repo_root);
    let mut missing_workflow_files = environment.missing_workflow_files(repo_root);
    missing_workflow_files.sort();
    let mut missing_git_hooks = environment.missing_git_hooks(repo_root);
    missing_git_hooks.sort();
    let hooks_path = environment
        .git_hooks_path(repo_root)
        .map(|p| p.to_string_lossy().into_owned());
    let sm_audit = sm_audit::run_audit(repo_root);
    // Only fetch the github user when it may be needed for fix construction.
    let github_user = if !environment.has_github_remote(repo_root) {
        environment.github_user()
    } else {
        None
    };

    let repo_name = repo_root
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| "repo".to_string());

    DoctorReport {
        checks: vec![
            make_check(
                DoctorCheckId::GitInitialized,
                DoctorCheckScope::LocalConfiguration,
                is_git,
                None,
                repo_root,
                None,
            ),
            make_check(
                DoctorCheckId::GhInstalled,
                DoctorCheckScope::LocalConfiguration,
                environment.command_exists("gh"),
                None,
                repo_root,
                None,
            ),
            make_check(
                DoctorCheckId::CodexInstalled,
                DoctorCheckScope::LocalConfiguration,
                environment.command_exists("codex"),
                None,
                repo_root,
                None,
            ),
            make_check(
                DoctorCheckId::ClaudeInstalled,
                DoctorCheckScope::LocalConfiguration,
                environment.claude_reachable(),
                None,
                repo_root,
                None,
            ),
            make_check(
                DoctorCheckId::GhAuthenticated,
                DoctorCheckScope::ExternalAuth,
                environment.gh_authenticated(),
                None,
                repo_root,
                None,
            ),
            make_check(
                DoctorCheckId::GithubRemoteConfigured,
                DoctorCheckScope::LocalConfiguration,
                environment.has_github_remote(repo_root),
                None,
                repo_root,
                github_user.as_deref().map(|u| format!("{u}/{repo_name}")),
            ),
            make_check(
                DoctorCheckId::RequiredWorkflowFilesPresent,
                DoctorCheckScope::LocalConfiguration,
                missing_workflow_files.is_empty(),
                (!missing_workflow_files.is_empty()).then_some(missing_workflow_files.join(", ")),
                repo_root,
                None,
            ),
            make_check(
                DoctorCheckId::RequiredGitHooksInstalled,
                DoctorCheckScope::LocalConfiguration,
                missing_git_hooks.is_empty(),
                (!missing_git_hooks.is_empty()).then_some(missing_git_hooks.join(", ")),
                repo_root,
                hooks_path,
            ),
            make_check(
                DoctorCheckId::StateMachineIntegrity,
                DoctorCheckScope::LocalConfiguration,
                sm_audit.error_count() == 0,
                if sm_audit.findings.is_empty() {
                    None
                } else {
                    Some(sm_audit::render_audit(&sm_audit))
                },
                repo_root,
                None,
            ),
        ],
    }
}

fn status_from_bool(passing: bool, severity: CheckSeverity) -> DoctorStatus {
    if passing {
        DoctorStatus::Passing
    } else {
        match severity {
            CheckSeverity::Required => DoctorStatus::Failing,
            CheckSeverity::Advisory => DoctorStatus::Warning,
        }
    }
}

/// `extra` carries fix-construction context that varies per check:
/// - For `GithubRemoteConfigured`: `Some("<user>/<repo>")` when a gh user is known.
/// - For `RequiredWorkflowFilesPresent`: the `detail` field carries the missing file list.
/// - For `RequiredGitHooksInstalled`: `Some("<hooks_dir>")` — the resolved git hooks path.
/// - For all others: `None`.
fn make_check(
    id: DoctorCheckId,
    scope: DoctorCheckScope,
    passing: bool,
    detail: Option<String>,
    repo_root: &Path,
    extra: Option<String>,
) -> DoctorCheck {
    let status = status_from_bool(passing, id.severity());
    let is_not_passing = status != DoctorStatus::Passing;
    let remediation = is_not_passing
        .then(|| failing_fix(id, detail.as_deref(), extra.as_deref()))
        .flatten();
    let fix = is_not_passing
        .then(|| failing_doctor_fix(id, detail.as_deref(), repo_root, extra.as_deref()))
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
        let status = match check.status {
            DoctorStatus::Passing => "PASS",
            DoctorStatus::Warning => "WARN",
            DoctorStatus::Failing => "FAIL",
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
        let status = match check.status {
            DoctorStatus::Passing => "PASS",
            DoctorStatus::Warning => "WARN",
            DoctorStatus::Failing => "FAIL",
        };
        lines.push(format!("- [{status}] {}", check.id.label()));

        if let Some(detail) = &check.detail {
            lines.push(format!("  detail: {detail}"));
        }

        if let Some(fix) = &check.remediation {
            lines.push(format!("  fix: {fix}"));
        }

        if let Some(doctor_fix) = &check.fix {
            render_fix_verbose(doctor_fix, &mut lines, "  ");
        }
    }

    lines.join("\n")
}

fn render_fix_verbose(fix: &DoctorFix, lines: &mut Vec<String>, indent: &str) {
    match fix {
        DoctorFix::RunCommand { command, args } => {
            lines.push(format!("  auto-fix: {command} {}", args.join(" ")));
        }
        DoctorFix::Manual { instructions } => {
            lines.push(format!("  manual-fix: {instructions}"));
        }
        DoctorFix::WriteFile { path, .. } => {
            lines.push(format!("  auto-fix: write {}", path.display()));
        }
        DoctorFix::Sequence(steps) => {
            lines.push(format!("{indent}auto-fix (sequence):"));
            for step in steps {
                render_fix_verbose(step, lines, &format!("{indent}  "));
            }
        }
    }
}

/// Apply a `DoctorFix` in the given working directory.
///
/// Returns `Ok(output)` on success.  Returns `Err(message)` on failure.
pub fn apply_fix(fix: &DoctorFix, cwd: &Path) -> Result<String, String> {
    match fix {
        DoctorFix::Manual { instructions } => Ok(instructions.clone()),
        DoctorFix::RunCommand { command, args } => run_command(command, args, cwd),
        DoctorFix::WriteFile { path, content } => {
            if let Some(parent) = path.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| format!("failed to create directory {}: {e}", parent.display()))?;
            }
            std::fs::write(path, content)
                .map_err(|e| format!("failed to write {}: {e}", path.display()))?;
            Ok(format!("wrote {}", path.display()))
        }
        DoctorFix::Sequence(steps) => {
            let mut outputs = Vec::new();
            for step in steps {
                match apply_fix(step, cwd) {
                    Ok(out) => {
                        if !out.is_empty() {
                            outputs.push(out);
                        }
                    }
                    Err(err) => return Err(err),
                }
            }
            Ok(outputs.join("\n"))
        }
    }
}

fn run_command(command: &str, args: &[String], cwd: &Path) -> Result<String, String> {
    let output = Command::new(command)
        .args(args)
        .current_dir(cwd)
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

fn failing_doctor_fix(
    id: DoctorCheckId,
    detail: Option<&str>,
    repo_root: &Path,
    extra: Option<&str>,
) -> Option<DoctorFix> {
    match id {
        // Binary installs are manual — no auto-download.
        DoctorCheckId::GhInstalled => Some(DoctorFix::Manual {
            instructions: "Install gh from https://cli.github.com".to_string(),
        }),
        DoctorCheckId::CodexInstalled => Some(DoctorFix::Manual {
            instructions: "Install codex from https://openai.com/codex".to_string(),
        }),
        DoctorCheckId::ClaudeInstalled => Some(DoctorFix::Manual {
            instructions: "Install claude from https://claude.ai/code".to_string(),
        }),

        // `git init` in the target directory.
        DoctorCheckId::GitInitialized => Some(DoctorFix::RunCommand {
            command: "git".to_string(),
            args: vec![
                "-C".to_string(),
                repo_root.to_string_lossy().into_owned(),
                "init".to_string(),
            ],
        }),

        // Authentication can be triggered automatically.
        DoctorCheckId::GhAuthenticated => Some(DoctorFix::RunCommand {
            command: "gh".to_string(),
            args: vec!["auth".to_string(), "login".to_string()],
        }),

        // Create the GitHub repo and wire up the remote.
        // `extra` carries "<user>/<repo>" when a gh user is known at collection time.
        DoctorCheckId::GithubRemoteConfigured => {
            if let Some(slug) = extra {
                Some(DoctorFix::RunCommand {
                    command: "gh".to_string(),
                    args: vec![
                        "repo".to_string(),
                        "create".to_string(),
                        slug.to_string(),
                        "--private".to_string(),
                        "--source=.".to_string(),
                        "--remote=origin".to_string(),
                    ],
                })
            } else {
                Some(DoctorFix::Manual {
                    instructions:
                        "Authenticate with `gh auth login`, then re-run doctor to auto-create the repository."
                            .to_string(),
                })
            }
        }

        // Copy missing/outdated hooks from scripts/hooks/ into the git hooks dir.
        // `extra` carries the resolved hooks directory path.
        DoctorCheckId::RequiredGitHooksInstalled => {
            let hooks_dir = extra.map(PathBuf::from)?;
            let missing: Vec<&str> = detail
                .unwrap_or("")
                .split(", ")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();

            if missing.is_empty() {
                return None;
            }

            let steps: Vec<DoctorFix> = missing
                .iter()
                .flat_map(|hook| {
                    let source = repo_root.join("scripts").join("hooks").join(hook);
                    let content = std::fs::read_to_string(&source).ok()?;
                    let dest = hooks_dir.join(hook);
                    Some(vec![
                        DoctorFix::WriteFile {
                            path: dest.clone(),
                            content,
                        },
                        DoctorFix::RunCommand {
                            command: "chmod".to_string(),
                            args: vec!["+x".to_string(), dest.to_string_lossy().into_owned()],
                        },
                    ])
                })
                .flatten()
                .collect();

            if steps.is_empty() {
                None
            } else {
                Some(DoctorFix::Sequence(steps))
            }
        }

        // Write every missing workflow file then commit and push.
        DoctorCheckId::RequiredWorkflowFilesPresent => {
            let workflows_dir = repo_root.join(".github").join("workflows");
            let missing: Vec<&str> = detail
                .unwrap_or("")
                .split(", ")
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .collect();

            if missing.is_empty() {
                return None;
            }

            let mut steps: Vec<DoctorFix> = missing
                .iter()
                .filter_map(|filename| {
                    workflows::content_for(filename).map(|content| DoctorFix::WriteFile {
                        path: workflows_dir.join(filename),
                        content: content.to_string(),
                    })
                })
                .collect();

            // Stage, commit, and push the new workflow files.
            steps.push(DoctorFix::RunCommand {
                command: "git".to_string(),
                args: vec!["add".to_string(), ".github/workflows/".to_string()],
            });
            steps.push(DoctorFix::RunCommand {
                command: "git".to_string(),
                args: vec![
                    "commit".to_string(),
                    "-m".to_string(),
                    "chore: add required GitHub Actions workflows".to_string(),
                ],
            });
            steps.push(DoctorFix::RunCommand {
                command: "git".to_string(),
                args: vec!["push".to_string()],
            });

            Some(DoctorFix::Sequence(steps))
        }

        // No automated fix for SM integrity — manual review required.
        DoctorCheckId::StateMachineIntegrity => Some(DoctorFix::Manual {
            instructions:
                "Review the state machine audit output and correct workflow references in blueprint YAML files."
                    .to_string(),
        }),
    }
}

fn failing_fix(id: DoctorCheckId, detail: Option<&str>, extra: Option<&str>) -> Option<String> {
    match id {
        DoctorCheckId::GitInitialized => {
            Some("Run `git init` to initialise a git repository in this directory.".to_string())
        }
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
        DoctorCheckId::GithubRemoteConfigured => {
            if let Some(slug) = extra {
                Some(format!(
                    "Will create private GitHub repository `{slug}` and configure it as the `origin` remote."
                ))
            } else {
                Some(
                    "Authenticate with `gh auth login` first, then re-run doctor to auto-create the repository."
                        .to_string(),
                )
            }
        }
        DoctorCheckId::RequiredGitHooksInstalled => Some(format!(
            "Missing or outdated git hooks will be installed from scripts/hooks/: {}",
            detail.unwrap_or("unknown")
        )),
        DoctorCheckId::RequiredWorkflowFilesPresent => Some(format!(
            "Missing workflow files will be written and pushed: {}",
            detail.unwrap_or("unknown")
        )),
        DoctorCheckId::StateMachineIntegrity => {
            Some("Review the state machine audit findings and fix workflow references.".to_string())
        }
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

    fn unique_hooks_temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("calypso-doctor-hooks-{label}-{nanos}"));
        std::fs::create_dir_all(path.join("scripts/hooks"))
            .expect("scripts/hooks dir should be created");
        std::fs::create_dir_all(path.join(".git/hooks")).expect(".git/hooks dir should be created");
        // Write source hook files.
        for hook in REQUIRED_GIT_HOOKS {
            std::fs::write(
                path.join("scripts/hooks").join(hook),
                format!("#!/usr/bin/env bash\n# {hook}\n"),
            )
            .expect("source hook should be written");
        }
        // Initialize a git repo so git rev-parse works.
        Command::new("git")
            .args(["-C", &path.to_string_lossy(), "init"])
            .output()
            .expect("git init should succeed");
        path
    }

    #[test]
    fn host_environment_reports_missing_git_hooks() {
        let repo_root = unique_hooks_temp_dir("missing");
        // Install only one hook correctly.
        let hooks_dir = repo_root.join(".git/hooks");
        let source = std::fs::read_to_string(repo_root.join("scripts/hooks/pre-commit"))
            .expect("source hook should be readable");
        std::fs::write(hooks_dir.join("pre-commit"), &source).expect("hook should be written");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(
                hooks_dir.join("pre-commit"),
                std::fs::Permissions::from_mode(0o755),
            )
            .expect("permissions should be set");
        }

        let missing = HostDoctorEnvironment.missing_git_hooks(&repo_root);

        assert_eq!(
            missing,
            vec![
                "commit-msg",
                "post-checkout",
                "post-commit",
                "pre-push",
                "prepare-commit-msg",
            ]
        );

        std::fs::remove_dir_all(repo_root).ok();
    }

    #[test]
    fn host_environment_reports_outdated_git_hooks() {
        let repo_root = unique_hooks_temp_dir("outdated");
        let hooks_dir = repo_root.join(".git/hooks");
        // Install all hooks, but with wrong content for commit-msg.
        for hook in REQUIRED_GIT_HOOKS {
            let content = if hook == "commit-msg" {
                "#!/usr/bin/env bash\n# OUTDATED\n".to_string()
            } else {
                std::fs::read_to_string(repo_root.join("scripts/hooks").join(hook))
                    .expect("source hook should be readable")
            };
            std::fs::write(hooks_dir.join(hook), &content).expect("hook should be written");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    hooks_dir.join(hook),
                    std::fs::Permissions::from_mode(0o755),
                )
                .expect("permissions should be set");
            }
        }

        let missing = HostDoctorEnvironment.missing_git_hooks(&repo_root);

        assert_eq!(missing, vec!["commit-msg"]);

        std::fs::remove_dir_all(repo_root).ok();
    }

    #[test]
    fn host_environment_reports_all_hooks_present_when_content_matches() {
        let repo_root = unique_hooks_temp_dir("all-present");
        let hooks_dir = repo_root.join(".git/hooks");
        // Install all hooks with matching content.
        for hook in REQUIRED_GIT_HOOKS {
            let content = std::fs::read_to_string(repo_root.join("scripts/hooks").join(hook))
                .expect("source hook should be readable");
            std::fs::write(hooks_dir.join(hook), &content).expect("hook should be written");
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                std::fs::set_permissions(
                    hooks_dir.join(hook),
                    std::fs::Permissions::from_mode(0o755),
                )
                .expect("permissions should be set");
            }
        }

        let missing = HostDoctorEnvironment.missing_git_hooks(&repo_root);

        assert!(
            missing.is_empty(),
            "expected no missing hooks but got: {missing:?}"
        );

        std::fs::remove_dir_all(repo_root).ok();
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

    #[test]
    fn doctor_fix_is_automatic_for_non_manual_variants() {
        assert!(
            DoctorFix::RunCommand {
                command: "git".to_string(),
                args: vec!["init".to_string()]
            }
            .is_automatic()
        );
        assert!(
            DoctorFix::WriteFile {
                path: PathBuf::from("/tmp/foo"),
                content: "x".to_string()
            }
            .is_automatic()
        );
        assert!(DoctorFix::Sequence(vec![]).is_automatic());
        assert!(
            !DoctorFix::Manual {
                instructions: "do it manually".to_string()
            }
            .is_automatic()
        );
    }

    #[test]
    fn apply_fix_write_file_creates_parent_and_writes_content() {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("calypso-apply-fix-{nanos}"));
        let file_path = dir.join("sub").join("file.txt");
        let fix = DoctorFix::WriteFile {
            path: file_path.clone(),
            content: "hello".to_string(),
        };
        let result = apply_fix(&fix, &dir);
        assert!(result.is_ok(), "write fix should succeed: {result:?}");
        assert_eq!(
            std::fs::read_to_string(&file_path).expect("file should exist"),
            "hello"
        );
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn apply_fix_sequence_stops_on_first_error() {
        let dir = std::env::temp_dir();
        let fix = DoctorFix::Sequence(vec![
            DoctorFix::RunCommand {
                command: "false".to_string(),
                args: vec![],
            },
            // This second step should never run.
            DoctorFix::Manual {
                instructions: "should not appear".to_string(),
            },
        ]);
        let result = apply_fix(&fix, &dir);
        assert!(result.is_err(), "sequence should fail on first error");
    }
}
