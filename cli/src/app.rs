use std::path::{Path, PathBuf};
use std::process::Command;

use serde::Deserialize;

use crate::doctor::{
    DoctorReport, DoctorStatus, HostDoctorEnvironment, apply_fix, collect_doctor_report,
    render_doctor_report, render_doctor_report_verbose,
};
use crate::github::{HostGithubEnvironment, collect_github_report};
use crate::policy::{HostPolicyEnvironment, collect_policy_evidence};
use crate::report::{
    AgentJsonSession, AgentsJsonReport, DoctorJsonCheck, DoctorJsonReport, DoctorJsonSummary,
    StateJsonGate, StateJsonGateGroup, StateStatusJsonReport,
};
use crate::state::{
    AgentSession, AgentSessionStatus, BuiltinEvidence, DevelopmentState, EvidenceStatus,
    FeatureState, GateStatus, GithubMergeability, GithubReviewStatus, PullRequestChecklistItem,
    PullRequestRef,
};
use crate::template::load_embedded_template_set;

pub fn run_doctor(cwd: &Path) -> String {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);

    render_doctor_report(&report)
}

pub fn run_doctor_verbose(cwd: &Path) -> String {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);

    render_doctor_report_verbose(&report)
}

/// Result of attempting to fix a single doctor check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixAttemptResult {
    pub check_label: String,
    pub applied: bool,
    pub output: String,
    /// Whether re-validation after the fix showed the check passing.
    pub validated: Option<bool>,
}

/// Apply the fix for a single failing check, then re-run validation.
///
/// Returns `Ok(result)` with the fix output on success, `Err(message)` on failure.
pub fn run_doctor_fix_single(cwd: &Path, check_id: &str) -> Result<FixAttemptResult, String> {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);

    let check = report
        .checks
        .iter()
        .find(|c| c.id.label() == check_id)
        .ok_or_else(|| format!("unknown check id '{check_id}'"))?;

    if check.status == DoctorStatus::Passing {
        return Ok(FixAttemptResult {
            check_label: check_id.to_string(),
            applied: false,
            output: "already passing".to_string(),
            validated: Some(true),
        });
    }

    let fix = check
        .fix
        .as_ref()
        .ok_or_else(|| format!("no fix available for '{check_id}'"))?;

    let output = apply_fix(fix, &repo_root)?;
    let is_manual = !fix.is_automatic();

    // Re-run the doctor check to validate the fix worked.
    let validated = if is_manual {
        None
    } else {
        let post_report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);
        post_report
            .checks
            .iter()
            .find(|c| c.id.label() == check_id)
            .map(|c| c.status == DoctorStatus::Passing)
    };

    Ok(FixAttemptResult {
        check_label: check_id.to_string(),
        applied: true,
        output,
        validated,
    })
}

/// Apply fixes for all failing checks that have auto-fixes.
///
/// Returns a list of results, one per failing check that was attempted.
pub fn run_doctor_fix_all(cwd: &Path) -> Vec<FixAttemptResult> {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);

    let mut results = Vec::new();

    for check in &report.checks {
        if check.status == DoctorStatus::Passing {
            continue;
        }

        let label = check.id.label().to_string();

        match &check.fix {
            None => {
                results.push(FixAttemptResult {
                    check_label: label,
                    applied: false,
                    output: "no fix available".to_string(),
                    validated: None,
                });
            }
            Some(fix) if !fix.is_automatic() => {
                let instructions = match fix {
                    crate::doctor::DoctorFix::Manual { instructions } => instructions.clone(),
                    _ => "manual action required".to_string(),
                };
                results.push(FixAttemptResult {
                    check_label: label,
                    applied: false,
                    output: format!("manual fix: {instructions}"),
                    validated: None,
                });
            }
            Some(fix) => match apply_fix(fix, &repo_root) {
                Ok(output) => {
                    // Re-run check to validate the fix.
                    let post_report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);
                    let validated = post_report
                        .checks
                        .iter()
                        .find(|c| c.id.label() == label)
                        .map(|c| c.status == DoctorStatus::Passing);

                    results.push(FixAttemptResult {
                        check_label: label,
                        applied: true,
                        output,
                        validated,
                    });
                }
                Err(error) => {
                    results.push(FixAttemptResult {
                        check_label: label,
                        applied: false,
                        output: format!("fix failed: {error}"),
                        validated: Some(false),
                    });
                }
            },
        }
    }

    results
}

/// Render fix results into human-readable CLI output.
pub fn render_fix_results(results: &[FixAttemptResult]) -> String {
    let mut lines = Vec::new();

    if results.is_empty() {
        lines.push("All checks passing — nothing to fix.".to_string());
        return lines.join("\n");
    }

    lines.push("Doctor fix results".to_string());
    lines.push("─".repeat(42));

    for result in results {
        let status = if result.applied {
            match result.validated {
                Some(true) => "FIXED",
                Some(false) => "FAILED",
                None => "APPLIED",
            }
        } else if result.output == "already passing" {
            "PASS"
        } else {
            "SKIP"
        };

        lines.push(format!("- [{status}] {}", result.check_label));

        if !result.output.is_empty() {
            for line in result.output.lines() {
                lines.push(format!("  {line}"));
            }
        }
    }

    lines.join("\n")
}

pub fn run_status(cwd: &Path) -> Result<String, String> {
    let repo_root =
        resolve_repo_root(cwd).ok_or_else(|| "not inside a git repository".to_string())?;
    let branch = resolve_current_branch(&repo_root)
        .expect("git repositories should report the current branch");
    let template = load_embedded_template_set().expect("embedded templates should remain valid");
    let pull_request_lookup = resolve_current_pull_request(&repo_root);
    let pull_request = match &pull_request_lookup {
        Ok(pull_request) => pull_request.clone(),
        Err(_) => None,
    };
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
    let github_report = pull_request
        .as_ref()
        .map(|pr| collect_github_report(&HostGithubEnvironment, pr));
    let github_evidence = github_report
        .as_ref()
        .map(|report| report.to_builtin_evidence())
        .unwrap_or_else(missing_pull_request_evidence);
    let policy_evidence = collect_policy_evidence(&HostPolicyEnvironment, &repo_root, &template);
    let evidence = doctor_evidence
        .merge(&github_evidence)
        .merge(&policy_evidence);
    feature.github_snapshot = github_report
        .as_ref()
        .and_then(|report| report.snapshot.clone());
    feature.github_error = match &pull_request_lookup {
        Err(error) => Some(error.clone()),
        Ok(_) => None,
    }
    .or_else(|| {
        github_report
            .as_ref()
            .and_then(|report| report.error.clone())
    });

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
    // Include development phase if dev-state.json exists
    let dev_state_path = repo_root.join(".calypso").join("dev-state.json");
    let dev_phase = DevelopmentState::load_from_path(&dev_state_path)
        .map(|ds| ds.phase.to_string())
        .unwrap_or_else(|_| "unknown".to_string());

    let mut lines = vec![
        "Feature status".to_string(),
        format!("Repo: {}", repo_root.display()),
        format!("Branch: {branch}"),
        format!("Development phase: {dev_phase}"),
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
    if let Some(snapshot) = &feature.github_snapshot {
        lines.push("GitHub".to_string());
        lines.push(format!(
            "- PR state: {}",
            if snapshot.is_draft {
                "draft"
            } else {
                "ready-for-review"
            }
        ));
        lines.push(format!(
            "- Review: {}",
            github_review_label(&snapshot.review_status)
        ));
        lines.push(format!(
            "- Checks: {}",
            evidence_status_label(&snapshot.checks)
        ));
        lines.push(format!(
            "- Mergeability: {}",
            github_mergeability_label(&snapshot.mergeability)
        ));
        lines.push(String::new());
    } else if let Some(error) = &feature.github_error {
        lines.push("GitHub".to_string());
        lines.push(format!("- Error: {error}"));
        lines.push(String::new());
    }

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
    match run_command(cwd, "git", &["rev-parse", "--show-toplevel"]) {
        Ok(CommandOutput::Success(output)) => Some(PathBuf::from(output)),
        Ok(CommandOutput::Failure(_)) | Err(_) => None,
    }
}

pub fn resolve_current_branch(repo_root: &Path) -> Option<String> {
    match run_command(repo_root, "git", &["branch", "--show-current"]) {
        Ok(CommandOutput::Success(output)) => Some(output),
        Ok(CommandOutput::Failure(_)) | Err(_) => None,
    }
}

pub fn resolve_current_pull_request(repo_root: &Path) -> Result<Option<PullRequestRef>, String> {
    resolve_current_pull_request_with_program(repo_root, "gh")
}

pub fn resolve_current_pull_request_with_program(
    repo_root: &Path,
    program: &str,
) -> Result<Option<PullRequestRef>, String> {
    let output = run_command(repo_root, program, &["pr", "view", "--json", "number,url"])?;
    match output {
        CommandOutput::Success(output) => parse_pull_request_ref(&output)
            .map(Some)
            .ok_or_else(|| "gh returned malformed pull request JSON".to_string()),
        CommandOutput::Failure(error) => {
            if error.contains("no pull requests found") {
                Ok(None)
            } else {
                Err(error)
            }
        }
    }
}

pub fn missing_pull_request_ref() -> PullRequestRef {
    PullRequestRef {
        number: 0,
        url: String::new(),
    }
}

pub fn missing_pull_request_evidence() -> BuiltinEvidence {
    BuiltinEvidence::new().with_result("builtin.github.pr_exists", false)
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandOutput {
    Success(String),
    Failure(String),
}

pub fn run_command(cwd: &Path, program: &str, args: &[&str]) -> Result<CommandOutput, String> {
    let output = Command::new(program)
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| format!("failed to spawn `{program}`: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        let message = if stderr.is_empty() {
            format!("`{program}` exited with status {}", output.status)
        } else {
            stderr
        };
        return Ok(CommandOutput::Failure(message));
    }

    String::from_utf8(output.stdout)
        .map(|stdout| CommandOutput::Success(stdout.trim().to_string()))
        .map_err(|error| format!("`{program}` produced invalid UTF-8: {error}"))
}

fn github_review_label(status: &GithubReviewStatus) -> &'static str {
    match status {
        GithubReviewStatus::Approved => "approved",
        GithubReviewStatus::ReviewRequired => "review-required",
        GithubReviewStatus::ChangesRequested => "changes-requested",
    }
}

fn github_mergeability_label(status: &GithubMergeability) -> &'static str {
    match status {
        GithubMergeability::Mergeable => "mergeable",
        GithubMergeability::Conflicting => "conflicting",
        GithubMergeability::Blocked => "blocked",
        GithubMergeability::Unknown => "unknown",
    }
}

fn evidence_status_label(status: &EvidenceStatus) -> &'static str {
    match status {
        EvidenceStatus::Passing => "passing",
        EvidenceStatus::Failing => "failing",
        EvidenceStatus::Pending => "pending",
        EvidenceStatus::Manual => "manual",
    }
}

// ---------------------------------------------------------------------------
// Feature 1 — doctor --json
// ---------------------------------------------------------------------------

/// Build a `DoctorJsonReport` from a `DoctorReport`.
pub fn doctor_json_report(report: &DoctorReport) -> DoctorJsonReport {
    let checks: Vec<DoctorJsonCheck> = report
        .checks
        .iter()
        .map(|check| DoctorJsonCheck {
            id: check.id.label().to_string(),
            status: match check.status {
                DoctorStatus::Passing => "passing",
                DoctorStatus::Warning => "warning",
                DoctorStatus::Failing => "failing",
            },
            detail: check.detail.clone(),
            remediation: check.remediation.clone(),
            has_auto_fix: check.fix.as_ref().is_some_and(|f| f.is_automatic()),
        })
        .collect();

    let total = checks.len();
    let passing = checks.iter().filter(|c| c.status == "passing").count();
    let warnings = checks.iter().filter(|c| c.status == "warning").count();
    let failing = total - passing - warnings;

    DoctorJsonReport {
        checks,
        summary: DoctorJsonSummary {
            total,
            passing,
            warnings,
            failing,
        },
    }
}

/// Run the doctor check and return the JSON report as a pretty-printed string.
/// Returns `Ok(json)` when all checks pass, `Err(json)` when any fail.
pub fn run_doctor_json(cwd: &Path) -> Result<String, String> {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);
    let json_report = doctor_json_report(&report);
    let json = serde_json::to_string_pretty(&json_report).expect("DoctorJsonReport must serialize");
    if json_report.summary.failing == 0 {
        Ok(json)
    } else {
        Err(json)
    }
}

// ---------------------------------------------------------------------------
// Feature 2 — state status (plain text and --json)
// ---------------------------------------------------------------------------

fn gate_status_str(status: &GateStatus) -> &'static str {
    match status {
        GateStatus::Passing => "passing",
        GateStatus::Failing => "failing",
        GateStatus::Pending => "pending",
        GateStatus::Manual => "manual",
    }
}

/// Build a `StateStatusJsonReport` from a loaded `FeatureState`.
pub fn state_status_json_report(feature: &FeatureState) -> StateStatusJsonReport {
    let gate_groups = feature
        .gate_groups
        .iter()
        .map(|group| {
            let rollup = group.rollup();
            let group_status = match rollup.status {
                crate::state::GateGroupStatus::Passing => "passing",
                crate::state::GateGroupStatus::Pending => "pending",
                crate::state::GateGroupStatus::Manual => "manual",
                crate::state::GateGroupStatus::Blocked => "failing",
            };
            StateJsonGateGroup {
                id: group.id.clone(),
                label: group.label.clone(),
                status: group_status,
                gates: group
                    .gates
                    .iter()
                    .map(|gate| StateJsonGate {
                        id: gate.id.clone(),
                        label: gate.label.clone(),
                        status: gate_status_str(&gate.status),
                    })
                    .collect(),
            }
        })
        .collect();

    let pr_number = if feature.pull_request.number == 0 {
        None
    } else {
        Some(feature.pull_request.number)
    };

    StateStatusJsonReport {
        feature_id: feature.feature_id.clone(),
        branch: feature.branch.clone(),
        pr_number,
        workflow_state: feature.workflow_state.as_str().to_string(),
        gate_groups,
        blocking_gate_ids: feature.blocking_gate_ids(),
        active_session_count: feature.active_sessions.len(),
    }
}

/// Load state from `.calypso/state.json` and return the JSON report.
/// Returns `Ok(json)` on success, `Err(message)` when the file cannot be loaded.
pub fn run_state_status_json(cwd: &Path) -> Result<String, String> {
    let state_path = cwd.join(".calypso").join("state.json");
    let state =
        crate::state::RepositoryState::load_from_path(&state_path).map_err(|e| e.to_string())?;
    let json_report = state_status_json_report(&state.current_feature);
    serde_json::to_string_pretty(&json_report).map_err(|e| format!("serialization error: {e}"))
}

/// Render a human-readable summary of the feature state.
pub fn render_state_status(feature: &FeatureState) -> String {
    let mut lines = Vec::new();

    lines.push(format!("feature: {}", feature.feature_id));

    let pr_part = if feature.pull_request.number != 0 {
        format!("  PR: #{}", feature.pull_request.number)
    } else {
        String::new()
    };
    lines.push(format!("branch:  {}{}", feature.branch, pr_part));
    lines.push(format!("state:   {}", feature.workflow_state.as_str()));

    if !feature.gate_groups.is_empty() {
        lines.push(String::new());
        lines.push("  Gates".to_string());
        lines.push(format!("  {}", "─".repeat(33)));
        for group in &feature.gate_groups {
            let rollup = group.rollup();
            let blocking_count = rollup.blocking_gate_ids.len();
            let group_marker = if blocking_count == 0 { "✓" } else { "✗" };
            let blocking_str = if blocking_count > 0 {
                format!("  {blocking_count} blocking")
            } else {
                String::new()
            };
            lines.push(format!(
                "  {} {}{}",
                group_marker, group.label, blocking_str
            ));
            for gate in &group.gates {
                let gate_marker = if gate.status == GateStatus::Passing {
                    "✓"
                } else {
                    "✗"
                };
                lines.push(format!("    {} {}", gate_marker, gate.label));
            }
        }
    }

    lines.join("\n")
}

/// Load state from `.calypso/state.json` and return a plain-text summary.
pub fn run_state_status_plain(cwd: &Path) -> Result<String, String> {
    let state_path = cwd.join(".calypso").join("state.json");
    let state =
        crate::state::RepositoryState::load_from_path(&state_path).map_err(|e| e.to_string())?;
    Ok(render_state_status(&state.current_feature))
}

// ---------------------------------------------------------------------------
// Feature 3 — agents (plain text and --json)
// ---------------------------------------------------------------------------

fn agent_status_str(status: &AgentSessionStatus) -> &'static str {
    match status {
        AgentSessionStatus::Running => "running",
        AgentSessionStatus::WaitingForHuman => "waiting-for-human",
        AgentSessionStatus::Completed => "completed",
        AgentSessionStatus::Failed => "failed",
        AgentSessionStatus::Aborted => "aborted",
    }
}

/// Build an `AgentsJsonReport` from a `FeatureState`.
pub fn agents_json_report(feature: &FeatureState) -> AgentsJsonReport {
    let sessions = feature
        .active_sessions
        .iter()
        .map(|session| AgentJsonSession {
            session_id: session.session_id.clone(),
            role: session.role.clone(),
            status: agent_status_str(&session.status).to_string(),
            output: session.output.iter().map(|o| o.text.clone()).collect(),
            pending_follow_ups: session.pending_follow_ups.clone(),
        })
        .collect();

    AgentsJsonReport {
        feature_id: feature.feature_id.clone(),
        sessions,
    }
}

/// Load state from `.calypso/state.json` and return the agents JSON report.
pub fn run_agents_json(cwd: &Path) -> Result<String, String> {
    let state_path = cwd.join(".calypso").join("state.json");
    let state =
        crate::state::RepositoryState::load_from_path(&state_path).map_err(|e| e.to_string())?;
    let json_report = agents_json_report(&state.current_feature);
    serde_json::to_string_pretty(&json_report).map_err(|e| format!("serialization error: {e}"))
}

/// Render a human-readable agents status from a session list.
pub fn render_agents(feature: &FeatureState) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Active sessions for {}", feature.feature_id));
    lines.push("─".repeat(42));

    if feature.active_sessions.is_empty() {
        lines.push("  (no active sessions)".to_string());
    } else {
        for session in &feature.active_sessions {
            let (marker, status_str) = session_display_parts(session);
            lines.push(format!(
                "{} {}  [{}]  {}",
                marker, session.role, session.session_id, status_str
            ));
            for line in &session.output {
                for text_line in line.text.lines() {
                    lines.push(format!("  {text_line}"));
                }
            }
        }
    }

    lines.join("\n")
}

fn session_display_parts(session: &AgentSession) -> (&'static str, &'static str) {
    match session.status {
        AgentSessionStatus::Running => ("▶", "running"),
        AgentSessionStatus::WaitingForHuman => ("⏸", "waiting-for-human"),
        AgentSessionStatus::Completed => ("✓", "completed"),
        AgentSessionStatus::Failed => ("✗", "failed"),
        AgentSessionStatus::Aborted => ("⊗", "aborted"),
    }
}

/// Load state from `.calypso/state.json` and return a plain-text agents summary.
pub fn run_agents_plain(cwd: &Path) -> Result<String, String> {
    let state_path = cwd.join(".calypso").join("state.json");
    let state =
        crate::state::RepositoryState::load_from_path(&state_path).map_err(|e| e.to_string())?;
    Ok(render_agents(&state.current_feature))
}

// ---------------------------------------------------------------------------
// Feature 4 — workflows (list / show / validate)
// ---------------------------------------------------------------------------

/// Return a newline-separated list of all embedded blueprint workflow name stems.
pub fn run_workflows_list() -> String {
    crate::blueprint_workflows::BlueprintWorkflowLibrary::list()
        .iter()
        .map(|(stem, _)| *stem)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Return the raw YAML content for a named workflow, or an error message.
pub fn run_workflows_show(name: &str) -> Result<String, String> {
    crate::blueprint_workflows::BlueprintWorkflowLibrary::get(name)
        .map(|yaml| yaml.to_string())
        .ok_or_else(|| format!("workflow not found: {name}"))
}

/// Parse the named workflow and return `Ok("OK")` or `Err(parse_error_string)`.
pub fn run_workflows_validate(name: &str) -> Result<String, String> {
    let yaml = crate::blueprint_workflows::BlueprintWorkflowLibrary::get(name)
        .ok_or_else(|| format!("workflow not found: {name}"))?;
    crate::blueprint_workflows::BlueprintWorkflowLibrary::parse(yaml)
        .map(|_| "OK".to_string())
        .map_err(|e| e.to_string())
}

// ---------------------------------------------------------------------------
// Feature — development state (dev-status)
// ---------------------------------------------------------------------------

/// Render a human-readable summary of the development phase state machine.
pub fn render_dev_status(dev_state: &DevelopmentState) -> String {
    let mut lines = Vec::new();

    lines.push(format!("Development phase: {}", dev_state.phase));

    if let Some(init_step) = &dev_state.init_step {
        lines.push(format!("Init sub-state:    {init_step}"));
    }

    if let Some(ts) = &dev_state.last_transition_at {
        lines.push(format!("Last transition:   {ts}"));
    }

    if !dev_state.transition_log.is_empty() {
        lines.push(String::new());
        lines.push("Transition history".to_string());
        lines.push(format!("  {}", "\u{2500}".repeat(40)));
        for entry in &dev_state.transition_log {
            lines.push(format!(
                "  {} -> {} ({})",
                entry.from, entry.to, entry.timestamp
            ));
        }
    }

    lines.join("\n")
}

/// Load development state from `.calypso/dev-state.json` and render a plain-text summary.
pub fn run_dev_status(cwd: &Path) -> Result<String, String> {
    let dev_state_path = cwd.join(".calypso").join("dev-state.json");
    let dev_state = DevelopmentState::load_from_path(&dev_state_path).map_err(|e| e.to_string())?;
    Ok(render_dev_status(&dev_state))
}

/// Load development state from `.calypso/dev-state.json` and return as JSON.
pub fn run_dev_status_json(cwd: &Path) -> Result<String, String> {
    let dev_state_path = cwd.join(".calypso").join("dev-state.json");
    let dev_state = DevelopmentState::load_from_path(&dev_state_path).map_err(|e| e.to_string())?;
    serde_json::to_string_pretty(&dev_state).map_err(|e| format!("serialization error: {e}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_fix_results_empty_shows_all_passing() {
        let output = render_fix_results(&[]);
        assert!(
            output.contains("nothing to fix"),
            "expected 'nothing to fix' in output: {output}"
        );
    }

    #[test]
    fn render_fix_results_shows_fixed_status() {
        let results = vec![FixAttemptResult {
            check_label: "git-initialized".to_string(),
            applied: true,
            output: "Initialized empty Git repository".to_string(),
            validated: Some(true),
        }];
        let output = render_fix_results(&results);
        assert!(output.contains("[FIXED]"), "expected FIXED tag: {output}");
        assert!(
            output.contains("git-initialized"),
            "expected check label: {output}"
        );
    }

    #[test]
    fn render_fix_results_shows_failed_status() {
        let results = vec![FixAttemptResult {
            check_label: "gh-authenticated".to_string(),
            applied: true,
            output: "attempted auth".to_string(),
            validated: Some(false),
        }];
        let output = render_fix_results(&results);
        assert!(output.contains("[FAILED]"), "expected FAILED tag: {output}");
    }

    #[test]
    fn render_fix_results_shows_skip_for_manual() {
        let results = vec![FixAttemptResult {
            check_label: "gh-installed".to_string(),
            applied: false,
            output: "manual fix: Install gh from https://cli.github.com".to_string(),
            validated: None,
        }];
        let output = render_fix_results(&results);
        assert!(output.contains("[SKIP]"), "expected SKIP tag: {output}");
    }

    #[test]
    fn render_fix_results_shows_pass_for_already_passing() {
        let results = vec![FixAttemptResult {
            check_label: "git-initialized".to_string(),
            applied: false,
            output: "already passing".to_string(),
            validated: Some(true),
        }];
        let output = render_fix_results(&results);
        assert!(output.contains("[PASS]"), "expected PASS tag: {output}");
    }

    #[test]
    fn render_fix_results_shows_applied_when_no_validation() {
        let results = vec![FixAttemptResult {
            check_label: "some-check".to_string(),
            applied: true,
            output: "did something".to_string(),
            validated: None,
        }];
        let output = render_fix_results(&results);
        assert!(
            output.contains("[APPLIED]"),
            "expected APPLIED tag: {output}"
        );
    }

}
