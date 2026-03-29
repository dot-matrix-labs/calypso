use std::path::Path;

use crate::doctor::{HostDoctorEnvironment, collect_doctor_report};
use crate::github::{HostGithubEnvironment, collect_github_report};
use crate::policy::{HostPolicyEnvironment, collect_policy_evidence};
use crate::report::{StateJsonGate, StateJsonGateGroup, StateStatusJsonReport};
use crate::state::{
    DevelopmentState, EvidenceStatus, FeatureState, GateStatus, GithubMergeability,
    GithubReviewStatus, PullRequestChecklistItem, PullRequestRef,
};
use crate::template::load_embedded_template_set;

use super::helpers::{
    missing_pull_request_evidence, missing_pull_request_ref, resolve_current_branch,
    resolve_current_pull_request, resolve_repo_root,
};

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
        .map(|pr| collect_github_report(&HostGithubEnvironment::default(), pr));
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

fn gate_status_str(status: &GateStatus) -> &'static str {
    match status {
        GateStatus::Passing => "passing",
        GateStatus::Failing => "failing",
        GateStatus::Pending => "pending",
        GateStatus::Manual => "manual",
    }
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

/// Load state from `.calypso/repository-state.json` and return the JSON report.
/// Returns `Ok(json)` on success, `Err(message)` when the file cannot be loaded.
pub fn run_state_status_json(cwd: &Path) -> Result<String, String> {
    let state_path = cwd.join(".calypso").join("repository-state.json");
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

/// Load state from `.calypso/repository-state.json` and return a plain-text summary.
pub fn run_state_status_plain(cwd: &Path) -> Result<String, String> {
    let state_path = cwd.join(".calypso").join("repository-state.json");
    let state =
        crate::state::RepositoryState::load_from_path(&state_path).map_err(|e| e.to_string())?;
    Ok(render_state_status(&state.current_feature))
}

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

    // ── render_dev_status tests ───────────────────────────────────────────

    #[test]
    fn render_dev_status_shows_phase() {
        let state = DevelopmentState::new();
        let output = render_dev_status(&state);
        assert!(
            output.contains("Development phase: init"),
            "expected phase line: {output}"
        );
    }

    #[test]
    fn render_dev_status_shows_init_step_when_present() {
        let mut state = DevelopmentState::new();
        state.update_init_step("verify-setup");
        let output = render_dev_status(&state);
        assert!(
            output.contains("Init sub-state:    verify-setup"),
            "expected init step: {output}"
        );
    }

    #[test]
    fn render_dev_status_hides_init_step_when_absent() {
        let state = DevelopmentState::new();
        let output = render_dev_status(&state);
        assert!(
            !output.contains("Init sub-state"),
            "should not contain init sub-state: {output}"
        );
    }

    #[test]
    fn render_dev_status_shows_last_transition_timestamp() {
        let mut state = DevelopmentState::new();
        state
            .transition_to(
                crate::state::DevelopmentPhase::Development,
                "2026-03-15T00:00:00Z",
            )
            .unwrap();
        let output = render_dev_status(&state);
        assert!(
            output.contains("Last transition:   2026-03-15T00:00:00Z"),
            "expected timestamp: {output}"
        );
    }

    #[test]
    fn render_dev_status_shows_transition_history() {
        let mut state = DevelopmentState::new();
        state
            .transition_to(
                crate::state::DevelopmentPhase::Development,
                "2026-03-15T00:00:00Z",
            )
            .unwrap();
        let output = render_dev_status(&state);
        assert!(
            output.contains("Transition history"),
            "expected history header: {output}"
        );
        assert!(
            output.contains("init -> development"),
            "expected transition entry: {output}"
        );
    }

    #[test]
    fn render_dev_status_no_history_when_empty() {
        let state = DevelopmentState::new();
        let output = render_dev_status(&state);
        assert!(
            !output.contains("Transition history"),
            "should not contain history section: {output}"
        );
    }

    // ── run_dev_status / run_dev_status_json tests ────────────────────────

    #[test]
    fn run_dev_status_returns_error_for_missing_dir() {
        let result = run_dev_status(std::path::Path::new("/nonexistent/project"));
        assert!(result.is_err());
    }

    #[test]
    fn run_dev_status_json_returns_error_for_missing_dir() {
        let result = run_dev_status_json(std::path::Path::new("/nonexistent/project"));
        assert!(result.is_err());
    }

    #[test]
    fn run_dev_status_loads_from_file() {
        let tmp = std::env::temp_dir().join("calypso-app-dev-status-test");
        let calypso_dir = tmp.join(".calypso");
        std::fs::create_dir_all(&calypso_dir).unwrap();

        let mut state = DevelopmentState::new();
        state
            .transition_to(
                crate::state::DevelopmentPhase::Development,
                "2026-03-15T00:00:00Z",
            )
            .unwrap();
        state
            .save_to_path(&calypso_dir.join("dev-state.json"))
            .unwrap();

        let output = run_dev_status(&tmp).unwrap();
        assert!(
            output.contains("Development phase: development"),
            "expected development phase: {output}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn run_dev_status_json_returns_valid_json() {
        let tmp = std::env::temp_dir().join("calypso-app-dev-status-json-test");
        let calypso_dir = tmp.join(".calypso");
        std::fs::create_dir_all(&calypso_dir).unwrap();

        let state = DevelopmentState::new();
        state
            .save_to_path(&calypso_dir.join("dev-state.json"))
            .unwrap();

        let json_str = run_dev_status_json(&tmp).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json_str).unwrap();
        assert_eq!(parsed["phase"], "init");

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── render_feature_status dev phase integration ───────────────────────

    fn make_minimal_feature() -> FeatureState {
        use crate::state::{PullRequestRef, WorkflowState};
        FeatureState {
            feature_id: "test-feat".to_string(),
            branch: "main".to_string(),
            worktree_path: "/tmp/test".to_string(),
            pull_request: PullRequestRef {
                number: 1,
                url: "https://github.com/test/test/pull/1".to_string(),
            },
            github_snapshot: None,
            github_error: None,
            workflow_state: WorkflowState::New,
            gate_groups: vec![],
            active_sessions: vec![],
            feature_type: crate::state::FeatureType::Feat,
            roles: vec![],
            scheduling: crate::state::SchedulingMeta::default(),
            artifact_refs: vec![],
            transcript_refs: vec![],
            clarification_history: vec![],
        }
    }

    #[test]
    fn render_feature_status_shows_unknown_dev_phase_when_no_state_file() {
        let tmp = std::env::temp_dir().join("calypso-feat-status-no-dev-state");
        std::fs::create_dir_all(&tmp).unwrap();

        let feature = make_minimal_feature();
        let output = render_feature_status(&tmp, "main", None, &feature);
        assert!(
            output.contains("Development phase: unknown"),
            "expected unknown phase: {output}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn render_feature_status_shows_dev_phase_from_state_file() {
        let tmp = std::env::temp_dir().join("calypso-feat-status-with-dev-state");
        let calypso_dir = tmp.join(".calypso");
        std::fs::create_dir_all(&calypso_dir).unwrap();

        let mut state = DevelopmentState::new();
        state
            .transition_to(
                crate::state::DevelopmentPhase::Development,
                "2026-03-15T00:00:00Z",
            )
            .unwrap();
        state
            .save_to_path(&calypso_dir.join("dev-state.json"))
            .unwrap();

        let feature = make_minimal_feature();
        let output = render_feature_status(&tmp, "feat-branch", None, &feature);
        assert!(
            output.contains("Development phase: development"),
            "expected development phase: {output}"
        );

        let _ = std::fs::remove_dir_all(&tmp);
    }
}
