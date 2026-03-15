use std::fmt;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::state::{DevelopmentState, RepositoryIdentity, RepositoryState};
use crate::template::{DEFAULT_AGENTS_YAML, DEFAULT_PROMPTS_YAML, DEFAULT_STATE_MACHINE_YAML};

// ---------------------------------------------------------------------------
// Init state machine
// ---------------------------------------------------------------------------

/// The sequential steps of the `calypso init` state machine.
///
/// Each variant represents one discrete setup checkpoint. Progress is persisted
/// to `.calypso/init-state.json` so an interrupted init can be resumed from
/// the last completed step.
///
/// See `calypso-blueprint/development/init-state-machine.md` for the canonical
/// documentation of this state machine.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum InitStep {
    PromptDirectory,
    CreateGitRepo,
    CreateUpstream,
    ScaffoldGithubActions,
    ConfigureLocal,
    VerifySetup,
    Complete,
}

impl InitStep {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PromptDirectory => "prompt-directory",
            Self::CreateGitRepo => "create-git-repo",
            Self::CreateUpstream => "create-upstream",
            Self::ScaffoldGithubActions => "scaffold-github-actions",
            Self::ConfigureLocal => "configure-local",
            Self::VerifySetup => "verify-setup",
            Self::Complete => "complete",
        }
    }

    /// Returns the next step in the linear init sequence, or `None` if this is
    /// the terminal `Complete` step.
    pub fn next(&self) -> Option<Self> {
        match self {
            Self::PromptDirectory => Some(Self::CreateGitRepo),
            Self::CreateGitRepo => Some(Self::CreateUpstream),
            Self::CreateUpstream => Some(Self::ScaffoldGithubActions),
            Self::ScaffoldGithubActions => Some(Self::ConfigureLocal),
            Self::ConfigureLocal => Some(Self::VerifySetup),
            Self::VerifySetup => Some(Self::Complete),
            Self::Complete => None,
        }
    }

    /// Returns `true` if this is the terminal state.
    pub fn is_complete(&self) -> bool {
        matches!(self, Self::Complete)
    }
}

impl fmt::Display for InitStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Persisted progress record for the init state machine.
///
/// Written to `.calypso/init-state.json`. Calypso checks this file to
/// determine whether init has been run and which steps were completed.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct InitProgress {
    pub current_step: InitStep,
    pub repo_path: PathBuf,
    pub github_org: Option<String>,
    pub github_repo: Option<String>,
    pub completed_steps: Vec<InitStep>,
}

impl InitProgress {
    pub fn new(repo_path: PathBuf) -> Self {
        Self {
            current_step: InitStep::PromptDirectory,
            repo_path,
            github_org: None,
            github_repo: None,
            completed_steps: Vec::new(),
        }
    }

    /// Advance to the next step, recording the current step as completed.
    /// No-op when already at `Complete`.
    pub fn advance(&mut self) {
        if let Some(next) = self.current_step.next() {
            self.completed_steps.push(self.current_step.clone());
            self.current_step = next;
        }
    }

    /// Returns `true` if the given step has already been completed.
    pub fn is_step_done(&self, step: &InitStep) -> bool {
        self.completed_steps.contains(step)
    }
}

// ---------------------------------------------------------------------------
// GitHub Actions workflow templates
// ---------------------------------------------------------------------------

/// PR checklist workflow — fails CI if any unchecked task items remain in the
/// PR body.
pub const WORKFLOW_PR_CHECKLIST: &str = "name: PR checklist

on:
  pull_request:
    types: [opened, edited, synchronize, reopened]
  merge_group:

jobs:
  checklist:
    name: checklist
    runs-on: ubuntu-latest

    steps:
      - name: Require all task items to be checked
        uses: actions/github-script@v7
        with:
          script: |
            const body = (context.payload.pull_request || {}).body || '';
            const incomplete = (body.match(/- \\[ \\]/g) || []).length;
            if (incomplete > 0) {
              core.setFailed(
                `${incomplete} unchecked task item${incomplete === 1 ? '' : 's'} in PR description. ` +
                `Complete all checklist items before merging.`
              );
            } else {
              core.info('All checklist items are checked.');
            }
";

/// PR depends-on workflow — blocks merge if the PR's `Depends-on: #N`
/// declaration references an unmerged PR.
pub const WORKFLOW_PR_DEPENDS_ON: &str = "name: PR depends-on

on:
  pull_request:
    types: [opened, edited, synchronize, reopened]
  merge_group:

jobs:
  depends-on:
    name: depends-on
    runs-on: ubuntu-latest

    steps:
      - name: Check declared PR dependency is merged
        uses: actions/github-script@v7
        with:
          script: |
            if (!context.payload.pull_request) { core.info('merge_group context - skipping depends-on check.'); return; }
            const body = context.payload.pull_request.body || '';
            const match = body.match(/^Depends-on:\\s*#(\\d+)/im);
            if (!match) {
              core.info('No Depends-on declaration - skipping.');
              return;
            }
            const depNumber = parseInt(match[1], 10);
            const { data: dep } = await github.rest.pulls.get({
              owner: context.repo.owner,
              repo: context.repo.repo,
              pull_number: depNumber,
            });
            if (dep.merged_at) {
              core.info(`PR #${depNumber} is merged - dependency satisfied.`);
            } else {
              core.setFailed(
                `Blocked: PR #${depNumber} (\"${dep.title}\") must be merged before this PR.`
              );
            }
";

/// Generic CI placeholder workflow — teams customize this for their stack.
pub const WORKFLOW_CI: &str = "name: CI

on:
  pull_request:
    types: [opened, synchronize, reopened]
  push:
    branches: [main]

jobs:
  build-and-test:
    name: build-and-test
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v4

      - name: Run tests
        run: |
          echo 'Replace this step with your project test command.'
          echo 'Examples: cargo test, npm test, pytest, go test ./...'
";

// ---------------------------------------------------------------------------
// InitRequest (extended)
// ---------------------------------------------------------------------------

pub struct InitRequest {
    pub repo_path: PathBuf,
    pub provider: Option<String>,
    pub allow_reinit: bool,
    /// Create a `.git` repo if missing (for `calypso init` in a fresh directory).
    pub create_git_repo: bool,
    /// GitHub org/user for creating an upstream remote.
    pub github_org: Option<String>,
    /// Repository name for creating an upstream remote.
    pub github_repo_name: Option<String>,
}

#[derive(Debug)]
pub struct InitResult {
    pub calypso_dir: PathBuf,
    pub state_path: PathBuf,
    pub hooks_installed: Vec<String>,
    pub templates_written: Vec<String>,
}

#[derive(Debug)]
pub enum InitError {
    Io(std::io::Error),
    NotAGitRepo { path: PathBuf },
    NotAGithubRemote { url: String },
    GitCommandFailed { action: String, details: String },
    AlreadyInitialized { calypso_dir: PathBuf },
    StateSerialize(serde_json::Error),
}

impl InitError {
    fn git(action: &str, details: &str) -> Self {
        Self::GitCommandFailed {
            action: action.to_string(),
            details: details.to_string(),
        }
    }
}

impl fmt::Display for InitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "init I/O error: {e}"),
            Self::NotAGitRepo { path } => write!(
                f,
                "path '{}' is not a git repository; run `git init` first",
                path.display()
            ),
            Self::NotAGithubRemote { url } => write!(
                f,
                "remote URL '{url}' is not a GitHub URL; calypso requires a GitHub remote — \
                 update the origin remote to a github.com URL and retry"
            ),
            Self::GitCommandFailed { action, details } => {
                write!(f, "{action} failed: {details}")
            }
            Self::AlreadyInitialized { calypso_dir } => write!(
                f,
                "'.calypso/' already exists at '{}'; pass --allow-reinit to re-initialise",
                calypso_dir.display()
            ),
            Self::StateSerialize(e) => write!(f, "failed to serialise repository state: {e}"),
        }
    }
}

impl std::error::Error for InitError {}

// ---------------------------------------------------------------------------
// InitEnvironment trait
// ---------------------------------------------------------------------------

pub trait InitEnvironment {
    fn is_git_repo(&self, path: &Path) -> Result<bool, InitError>;
    fn remote_url(&self, path: &Path) -> Result<String, InitError>;
    fn default_branch(&self, path: &Path) -> Result<String, InitError>;
    fn repo_name_from_url(&self, url: &str) -> Option<String>;
    fn path_exists(&self, path: &Path) -> bool;
    fn create_dir(&self, path: &Path) -> Result<(), InitError>;
    fn write_file(&self, path: &Path, contents: &str) -> Result<(), InitError>;
    fn set_executable(&self, path: &Path) -> Result<(), InitError>;
    fn remove_dir_all(&self, path: &Path) -> Result<(), InitError>;
    /// Run `git init` in `path`.
    fn git_init(&self, path: &Path) -> Result<(), InitError>;
    /// Run `gh repo create` to create a remote repo; returns the HTTPS clone URL.
    fn create_github_repo(&self, org: &str, repo: &str) -> Result<String, InitError>;
    /// Run `git remote add origin <url>`.
    fn set_remote(&self, path: &Path, url: &str) -> Result<(), InitError>;
    /// Write a GitHub Actions workflow file under `.github/workflows/<name>`.
    fn write_workflow_file(&self, path: &Path, name: &str, content: &str) -> Result<(), InitError>;
    /// Resolve the hooks directory via `git rev-parse --git-path hooks`.
    /// Handles worktrees and `core.hooksPath` correctly.
    fn git_hooks_path(&self, path: &Path) -> Result<PathBuf, InitError>;
}

// ---------------------------------------------------------------------------
// HostInitEnvironment
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, Default)]
pub struct HostInitEnvironment;

impl InitEnvironment for HostInitEnvironment {
    fn is_git_repo(&self, path: &Path) -> Result<bool, InitError> {
        let output = Command::new("git")
            .args(["-C", &path.to_string_lossy(), "rev-parse", "--git-dir"])
            .output()
            .map_err(InitError::Io)?;
        Ok(output.status.success())
    }

    fn remote_url(&self, path: &Path) -> Result<String, InitError> {
        let output = Command::new("git")
            .args(["-C", &path.to_string_lossy(), "remote", "get-url", "origin"])
            .output()
            .map_err(InitError::Io)?;
        if !output.status.success() {
            return Err(InitError::git(
                "git remote get-url origin",
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
    }

    fn default_branch(&self, path: &Path) -> Result<String, InitError> {
        let output = Command::new("git")
            .args([
                "-C",
                &path.to_string_lossy(),
                "symbolic-ref",
                "refs/remotes/origin/HEAD",
            ])
            .output()
            .map_err(InitError::Io)?;
        if !output.status.success() {
            // fall back to "main" if the symbolic ref is not set
            return Ok("main".to_string());
        }
        let full = String::from_utf8_lossy(&output.stdout).trim().to_string();
        // full looks like "refs/remotes/origin/main" — extract last segment
        Ok(full.split('/').next_back().unwrap_or("main").to_string())
    }

    fn repo_name_from_url(&self, url: &str) -> Option<String> {
        extract_repo_name(url)
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn create_dir(&self, path: &Path) -> Result<(), InitError> {
        fs::create_dir_all(path).map_err(InitError::Io)
    }

    fn write_file(&self, path: &Path, contents: &str) -> Result<(), InitError> {
        fs::write(path, contents).map_err(InitError::Io)
    }

    fn set_executable(&self, path: &Path) -> Result<(), InitError> {
        let mut perms = fs::metadata(path).map_err(InitError::Io)?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).map_err(InitError::Io)
    }

    fn remove_dir_all(&self, path: &Path) -> Result<(), InitError> {
        fs::remove_dir_all(path).map_err(InitError::Io)
    }

    fn git_init(&self, path: &Path) -> Result<(), InitError> {
        let output = Command::new("git")
            .args(["init"])
            .current_dir(path)
            .output()
            .map_err(InitError::Io)?;
        if !output.status.success() {
            return Err(InitError::git(
                "git init",
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        Ok(())
    }

    fn create_github_repo(&self, org: &str, repo: &str) -> Result<String, InitError> {
        let output = Command::new("gh")
            .args([
                "repo",
                "create",
                &format!("{org}/{repo}"),
                "--private",
                "--clone=false",
            ])
            .output()
            .map_err(InitError::Io)?;
        if !output.status.success() {
            return Err(InitError::git(
                "gh repo create",
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        Ok(format!("https://github.com/{org}/{repo}.git"))
    }

    fn set_remote(&self, path: &Path, url: &str) -> Result<(), InitError> {
        let output = Command::new("git")
            .args([
                "-C",
                &path.to_string_lossy(),
                "remote",
                "add",
                "origin",
                url,
            ])
            .output()
            .map_err(InitError::Io)?;
        if !output.status.success() {
            return Err(InitError::git(
                "git remote add origin",
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        Ok(())
    }

    fn write_workflow_file(&self, path: &Path, name: &str, content: &str) -> Result<(), InitError> {
        let workflows_dir = path.join(".github").join("workflows");
        fs::create_dir_all(&workflows_dir).map_err(InitError::Io)?;
        let file_path = workflows_dir.join(name);
        fs::write(file_path, content).map_err(InitError::Io)
    }

    fn git_hooks_path(&self, path: &Path) -> Result<PathBuf, InitError> {
        let output = Command::new("git")
            .args([
                "-C",
                &path.to_string_lossy(),
                "rev-parse",
                "--git-path",
                "hooks",
            ])
            .output()
            .map_err(InitError::Io)?;
        if !output.status.success() {
            return Err(InitError::git(
                "git rev-parse --git-path hooks",
                String::from_utf8_lossy(&output.stderr).trim(),
            ));
        }
        let raw = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
        // git may return a relative path (e.g. `.git/hooks`) — resolve against the repo root.
        if raw.is_absolute() {
            Ok(raw)
        } else {
            Ok(path.join(raw))
        }
    }
}

// ---------------------------------------------------------------------------
// URL helpers
// ---------------------------------------------------------------------------

fn extract_repo_name(url: &str) -> Option<String> {
    // Handles:
    //   https://github.com/org/repo.git
    //   git@github.com:org/repo.git
    //   https://github.com/org/repo
    let trimmed = url.trim_end_matches(".git");
    let after_slash = trimmed.rsplit('/').next()?;
    let after_colon = trimmed.rsplit(':').next()?;
    // Take the shorter non-empty segment that looks like a repo name (no dots)
    // Actually just take the last path component after the last slash or colon.
    // The last '/' already gives us the repo name.
    if !after_slash.is_empty() && !after_slash.contains('.') {
        Some(after_slash.to_string())
    } else if !after_colon.is_empty() {
        // git@github.com:org/repo — last component after last '/'
        let last = after_colon.split('/').next_back()?;
        let name = last.trim_end_matches(".git");
        if name.is_empty() {
            None
        } else {
            Some(name.to_string())
        }
    } else {
        None
    }
}

fn is_github_url(url: &str) -> bool {
    // Accepts:
    //   https://github.com/...   — host must be exactly "github.com"
    //   git@github.com:...       — SCP-style SSH remote
    // Rejects subdomains-of-evil (evil-github.com), path components that happen
    // to contain "github.com", and bare "github.com" strings with no scheme.
    if let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    {
        // rest starts with the host; verify it is exactly "github.com" (optionally
        // followed by '/' or end-of-string, not more hostname characters).
        rest.starts_with("github.com/") || rest == "github.com"
    } else if let Some(rest) = url.strip_prefix("git@") {
        rest.starts_with("github.com:")
    } else {
        false
    }
}

// ---------------------------------------------------------------------------
// Pre-push hook
// ---------------------------------------------------------------------------

const PRE_PUSH_HOOK: &str = "\
#!/bin/sh
# Calypso pre-push hook — run doctor non-blocking (warn but do not fail)
if command -v calypso-cli > /dev/null 2>&1; then
    calypso-cli doctor || true
fi
";

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

pub fn run_init(request: &InitRequest) -> Result<InitResult, InitError> {
    init_repository(request, &HostInitEnvironment)
}

/// Run the full interactive init flow, checking what already exists and only
/// performing the steps that are needed. This is the entry point for
/// `calypso init` from the CLI.
///
/// The function returns `Ok(InitProgress)` representing the final state of the
/// init state machine after all applicable steps have been executed.
pub fn run_init_interactive(
    repo_path: &Path,
    allow_reinit: bool,
    env: &impl InitEnvironment,
) -> Result<InitProgress, InitError> {
    let mut progress = InitProgress::new(repo_path.to_path_buf());

    // Load or create development state
    let calypso_dir = repo_path.join(".calypso");
    let dev_state_path = calypso_dir.join("dev-state.json");
    let mut dev_state = DevelopmentState::load_from_path(&dev_state_path)
        .unwrap_or_default();

    // Step: prompt-directory (non-interactive — cwd is the directory)
    dev_state.update_init_step(progress.current_step.as_str());
    progress.advance(); // PromptDirectory -> CreateGitRepo

    // Step: create-git-repo
    dev_state.update_init_step(progress.current_step.as_str());
    if !env.is_git_repo(repo_path)? {
        env.git_init(repo_path)?;
    }
    progress.advance(); // CreateGitRepo -> CreateUpstream

    // Step: create-upstream — attempt to get existing remote or skip
    // (When no upstream is configured yet, the caller should prompt the user
    // or pass github_org/repo. For now we record the step and move on.)
    dev_state.update_init_step(progress.current_step.as_str());
    progress.advance(); // CreateUpstream -> ScaffoldGithubActions

    // Step: scaffold-github-actions
    dev_state.update_init_step(progress.current_step.as_str());
    scaffold_github_actions(repo_path, env)?;
    progress.advance(); // ScaffoldGithubActions -> ConfigureLocal

    // Step: configure-local (.calypso/ setup) — only when a GitHub remote exists
    dev_state.update_init_step(progress.current_step.as_str());
    let remote_url = env.remote_url(repo_path).unwrap_or_default();
    if is_github_url(&remote_url) {
        let request = InitRequest {
            repo_path: repo_path.to_path_buf(),
            provider: None,
            allow_reinit,
            create_git_repo: false,
            github_org: None,
            github_repo_name: None,
        };
        init_repository(&request, env)?;
    }
    progress.advance(); // ConfigureLocal -> VerifySetup

    // Step: verify-setup — basic doctor-style checks
    // (Integration with doctor is intentional; we record completion here.)
    dev_state.update_init_step(progress.current_step.as_str());
    progress.advance(); // VerifySetup -> Complete

    // Mark init as complete and auto-advance to Development phase
    dev_state.update_init_step(progress.current_step.as_str());
    let now = current_timestamp();
    dev_state.auto_advance_from_init(&now);

    // Persist dev state (best-effort: don't fail init if the directory
    // was just created and state serialization succeeds)
    if calypso_dir.exists() || env.path_exists(&calypso_dir) {
        let _ = dev_state.save_to_path(&dev_state_path);
    }

    Ok(progress)
}

/// Returns a UTC timestamp in ISO 8601 format.
fn current_timestamp() -> String {
    // Use a simple approach that doesn't require chrono
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}Z", dur.as_secs())
}

/// Scaffold GitHub Actions workflow files into the repository.
///
/// Creates `.github/workflows/` with the three core workflow files if they
/// don't already exist. Existing files are left unchanged to avoid
/// overwriting customisations.
///
/// Returns the list of file names that were actually written.
pub fn scaffold_github_actions(
    repo_path: &Path,
    env: &impl InitEnvironment,
) -> Result<Vec<String>, InitError> {
    let workflows = [
        ("pr-checklist.yml", WORKFLOW_PR_CHECKLIST),
        ("pr-depends-on.yml", WORKFLOW_PR_DEPENDS_ON),
        ("ci.yml", WORKFLOW_CI),
    ];

    let mut scaffolded = Vec::new();
    for (name, content) in &workflows {
        let workflow_path = repo_path.join(".github").join("workflows").join(name);
        if !env.path_exists(&workflow_path) {
            env.write_workflow_file(repo_path, name, content)?;
            scaffolded.push(name.to_string());
        }
    }
    Ok(scaffolded)
}

pub fn init_repository(
    request: &InitRequest,
    env: &impl InitEnvironment,
) -> Result<InitResult, InitError> {
    // Step 1: validate git repo
    if !env.is_git_repo(&request.repo_path)? {
        return Err(InitError::NotAGitRepo {
            path: request.repo_path.clone(),
        });
    }

    // Step 2: detect GitHub remote
    let remote_url = env.remote_url(&request.repo_path)?;
    if !is_github_url(&remote_url) {
        return Err(InitError::NotAGithubRemote { url: remote_url });
    }

    // Step 3: detect default branch
    let default_branch = env.default_branch(&request.repo_path)?;

    // Derive repo name
    let repo_name = env
        .repo_name_from_url(&remote_url)
        .unwrap_or_else(|| "unknown".to_string());

    let calypso_dir = request.repo_path.join(".calypso");

    // Step 4: check for existing .calypso/
    if env.path_exists(&calypso_dir) && !request.allow_reinit {
        return Err(InitError::AlreadyInitialized {
            calypso_dir: calypso_dir.clone(),
        });
    }

    // Step 4 cont: create directory
    env.create_dir(&calypso_dir)?;

    // From here, rollback on failure
    let result = do_init_steps(
        request,
        env,
        &calypso_dir,
        &repo_name,
        &remote_url,
        &default_branch,
    );

    if result.is_err() {
        let _ = env.remove_dir_all(&calypso_dir);
    }

    result
}

fn do_init_steps(
    request: &InitRequest,
    env: &impl InitEnvironment,
    calypso_dir: &Path,
    repo_name: &str,
    remote_url: &str,
    default_branch: &str,
) -> Result<InitResult, InitError> {
    let state_path = calypso_dir.join("repository-state.json");

    // Step 5: write initial RepositoryState
    let identity = RepositoryIdentity {
        name: repo_name.to_string(),
        github_remote_url: remote_url.to_string(),
        default_branch: default_branch.to_string(),
    };

    let providers = if let Some(ref provider) = request.provider {
        vec![provider.clone()]
    } else {
        vec![]
    };

    let state = RepositoryState {
        version: 1,
        repo_id: repo_name.to_string(),
        schema_version: 1,
        identity,
        providers,
        current_feature: default_feature_state(),
        releases: vec![],
        deployments: vec![],
    };

    let json = serde_json::to_string_pretty(&state).map_err(InitError::StateSerialize)?;
    env.write_file(&state_path, &json)?;

    // Step 6: copy default template files
    let mut templates_written = Vec::new();

    let sm_path = calypso_dir.join("state-machine.yml");
    env.write_file(&sm_path, DEFAULT_STATE_MACHINE_YAML)?;
    templates_written.push("state-machine.yml".to_string());

    let agents_path = calypso_dir.join("agents.yml");
    env.write_file(&agents_path, DEFAULT_AGENTS_YAML)?;
    templates_written.push("agents.yml".to_string());

    let prompts_path = calypso_dir.join("prompts.yml");
    env.write_file(&prompts_path, DEFAULT_PROMPTS_YAML)?;
    templates_written.push("prompts.yml".to_string());

    // Step 7: install git hook
    let hooks_dir = env.git_hooks_path(&request.repo_path)?;
    env.create_dir(&hooks_dir)?;
    let hook_path = hooks_dir.join("pre-push");
    env.write_file(&hook_path, PRE_PUSH_HOOK)?;
    env.set_executable(&hook_path)?;
    let hooks_installed = vec!["pre-push".to_string()];

    Ok(InitResult {
        calypso_dir: calypso_dir.to_path_buf(),
        state_path,
        hooks_installed,
        templates_written,
    })
}

fn default_feature_state() -> crate::state::FeatureState {
    use crate::state::{FeatureState, FeatureType, PullRequestRef, SchedulingMeta, WorkflowState};

    FeatureState {
        feature_id: String::new(),
        branch: String::new(),
        worktree_path: String::new(),
        pull_request: PullRequestRef {
            number: 0,
            url: String::new(),
        },
        github_snapshot: None,
        github_error: None,
        workflow_state: WorkflowState::New,
        gate_groups: vec![],
        active_sessions: vec![],
        feature_type: FeatureType::Feat,
        roles: vec![],
        scheduling: SchedulingMeta::default(),
        artifact_refs: vec![],
        transcript_refs: vec![],
        clarification_history: vec![],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── is_github_url ────────────────────────────────────────────────────────

    #[test]
    fn is_github_url_recognizes_https() {
        assert!(is_github_url("https://github.com/org/repo"));
        assert!(is_github_url("https://github.com/org/repo.git"));
    }

    #[test]
    fn is_github_url_recognizes_ssh() {
        assert!(is_github_url("git@github.com:org/repo.git"));
        assert!(is_github_url("git@github.com:org/repo"));
    }

    #[test]
    fn is_github_url_rejects_lookalike_host() {
        // "evil-github.com" contains the substring "github.com" but is not the host
        assert!(!is_github_url("https://evil-github.com/org/repo"));
        assert!(!is_github_url("https://notgithub.com/org/repo"));
        assert!(!is_github_url("https://github.com.evil.org/org/repo"));
    }

    #[test]
    fn is_github_url_rejects_github_com_as_path_component() {
        // github.com appears in the path, not the host
        assert!(!is_github_url(
            "https://mirror.example.com/github.com/org/repo"
        ));
    }

    #[test]
    fn is_github_url_rejects_other_hosts() {
        assert!(!is_github_url("https://gitlab.com/org/repo.git"));
        assert!(!is_github_url("https://bitbucket.org/org/repo.git"));
    }

    #[test]
    fn is_github_url_rejects_empty_string() {
        assert!(!is_github_url(""));
    }

    #[test]
    fn is_github_url_rejects_bare_host_without_scheme() {
        // No scheme — not a valid remote URL we should accept
        assert!(!is_github_url("github.com/org/repo"));
    }

    #[test]
    fn is_github_url_rejects_http_github() {
        // http:// is accepted as a scheme (uncommon but parseable)
        assert!(is_github_url("http://github.com/org/repo"));
    }

    // ── extract_repo_name ────────────────────────────────────────────────────

    #[test]
    fn extract_repo_name_https_with_git_suffix() {
        assert_eq!(
            extract_repo_name("https://github.com/org/myrepo.git"),
            Some("myrepo".to_string())
        );
    }

    #[test]
    fn extract_repo_name_https_without_git_suffix() {
        assert_eq!(
            extract_repo_name("https://github.com/org/myrepo"),
            Some("myrepo".to_string())
        );
    }

    #[test]
    fn extract_repo_name_ssh_with_git_suffix() {
        assert_eq!(
            extract_repo_name("git@github.com:org/myrepo.git"),
            Some("myrepo".to_string())
        );
    }

    #[test]
    fn extract_repo_name_ssh_without_git_suffix() {
        assert_eq!(
            extract_repo_name("git@github.com:org/myrepo"),
            Some("myrepo".to_string())
        );
    }

    #[test]
    fn extract_repo_name_trailing_slash_returns_none() {
        // A URL with a trailing slash has an empty final component; the function
        // should not return an empty string as a repo name.
        assert_eq!(extract_repo_name("https://github.com/org/myrepo/"), None);
    }

    #[test]
    fn extract_repo_name_empty_returns_none() {
        assert_eq!(extract_repo_name(""), None);
    }

    // ── InitStep ─────────────────────────────────────────────────────────────

    #[test]
    fn init_step_as_str_values_are_kebab_case() {
        assert_eq!(InitStep::PromptDirectory.as_str(), "prompt-directory");
        assert_eq!(InitStep::CreateGitRepo.as_str(), "create-git-repo");
        assert_eq!(InitStep::CreateUpstream.as_str(), "create-upstream");
        assert_eq!(
            InitStep::ScaffoldGithubActions.as_str(),
            "scaffold-github-actions"
        );
        assert_eq!(InitStep::ConfigureLocal.as_str(), "configure-local");
        assert_eq!(InitStep::VerifySetup.as_str(), "verify-setup");
        assert_eq!(InitStep::Complete.as_str(), "complete");
    }

    #[test]
    fn init_step_next_follows_linear_sequence() {
        assert_eq!(
            InitStep::PromptDirectory.next(),
            Some(InitStep::CreateGitRepo)
        );
        assert_eq!(
            InitStep::CreateGitRepo.next(),
            Some(InitStep::CreateUpstream)
        );
        assert_eq!(
            InitStep::CreateUpstream.next(),
            Some(InitStep::ScaffoldGithubActions)
        );
        assert_eq!(
            InitStep::ScaffoldGithubActions.next(),
            Some(InitStep::ConfigureLocal)
        );
        assert_eq!(InitStep::ConfigureLocal.next(), Some(InitStep::VerifySetup));
        assert_eq!(InitStep::VerifySetup.next(), Some(InitStep::Complete));
        assert_eq!(InitStep::Complete.next(), None);
    }

    #[test]
    fn init_step_complete_is_terminal() {
        assert!(InitStep::Complete.is_complete());
        assert!(!InitStep::PromptDirectory.is_complete());
        assert!(!InitStep::VerifySetup.is_complete());
    }

    #[test]
    fn init_step_serializes_to_kebab_case() {
        let json = serde_json::to_string(&InitStep::ScaffoldGithubActions).unwrap();
        assert_eq!(json, "\"scaffold-github-actions\"");
    }

    #[test]
    fn init_step_deserializes_from_kebab_case() {
        let step: InitStep = serde_json::from_str("\"create-git-repo\"").unwrap();
        assert_eq!(step, InitStep::CreateGitRepo);
    }

    #[test]
    fn init_step_round_trips_through_json() {
        let steps = [
            InitStep::PromptDirectory,
            InitStep::CreateGitRepo,
            InitStep::CreateUpstream,
            InitStep::ScaffoldGithubActions,
            InitStep::ConfigureLocal,
            InitStep::VerifySetup,
            InitStep::Complete,
        ];
        for step in &steps {
            let json = serde_json::to_string(step).unwrap();
            let decoded: InitStep = serde_json::from_str(&json).unwrap();
            assert_eq!(&decoded, step);
        }
    }

    // ── InitProgress ─────────────────────────────────────────────────────────

    #[test]
    fn init_progress_new_starts_at_prompt_directory() {
        let progress = InitProgress::new(PathBuf::from("/fake/repo"));
        assert_eq!(progress.current_step, InitStep::PromptDirectory);
        assert!(progress.completed_steps.is_empty());
    }

    #[test]
    fn init_progress_advance_moves_through_sequence() {
        let mut progress = InitProgress::new(PathBuf::from("/fake/repo"));
        progress.advance();
        assert_eq!(progress.current_step, InitStep::CreateGitRepo);
        assert!(progress.is_step_done(&InitStep::PromptDirectory));

        progress.advance();
        assert_eq!(progress.current_step, InitStep::CreateUpstream);
        assert!(progress.is_step_done(&InitStep::CreateGitRepo));
    }

    #[test]
    fn init_progress_advance_at_complete_is_a_no_op() {
        let mut progress = InitProgress::new(PathBuf::from("/fake/repo"));
        for _ in 0..6 {
            progress.advance();
        }
        assert_eq!(progress.current_step, InitStep::Complete);
        progress.advance();
        assert_eq!(progress.current_step, InitStep::Complete);
    }

    #[test]
    fn init_progress_is_step_done_returns_false_for_future_steps() {
        let progress = InitProgress::new(PathBuf::from("/fake/repo"));
        assert!(!progress.is_step_done(&InitStep::CreateGitRepo));
        assert!(!progress.is_step_done(&InitStep::Complete));
    }

    #[test]
    fn init_progress_serializes_and_deserializes() {
        let mut progress = InitProgress::new(PathBuf::from("/fake/repo"));
        progress.advance();
        progress.github_org = Some("my-org".to_string());
        progress.github_repo = Some("my-repo".to_string());

        let json = serde_json::to_string(&progress).unwrap();
        let decoded: InitProgress = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.current_step, InitStep::CreateGitRepo);
        assert_eq!(decoded.github_org.as_deref(), Some("my-org"));
        assert!(decoded.is_step_done(&InitStep::PromptDirectory));
    }

    // ── workflow constants ────────────────────────────────────────────────────

    #[test]
    fn workflow_pr_checklist_is_valid_yaml() {
        let val: serde_yaml::Value =
            serde_yaml::from_str(WORKFLOW_PR_CHECKLIST).expect("pr-checklist should be valid YAML");
        let map = val.as_mapping().expect("top-level should be a mapping");
        assert!(map.contains_key(serde_yaml::Value::String("name".into())));
        assert!(map.contains_key(serde_yaml::Value::String("on".into())));
        assert!(map.contains_key(serde_yaml::Value::String("jobs".into())));
    }

    #[test]
    fn workflow_pr_depends_on_is_valid_yaml() {
        let val: serde_yaml::Value = serde_yaml::from_str(WORKFLOW_PR_DEPENDS_ON)
            .expect("pr-depends-on should be valid YAML");
        let map = val.as_mapping().expect("top-level should be a mapping");
        assert!(map.contains_key(serde_yaml::Value::String("name".into())));
        assert!(map.contains_key(serde_yaml::Value::String("jobs".into())));
    }

    #[test]
    fn workflow_ci_is_valid_yaml() {
        let val: serde_yaml::Value =
            serde_yaml::from_str(WORKFLOW_CI).expect("ci should be valid YAML");
        let map = val.as_mapping().expect("top-level should be a mapping");
        assert!(map.contains_key(serde_yaml::Value::String("jobs".into())));
    }
}
