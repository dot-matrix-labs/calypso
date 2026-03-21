//! Plain-text operator surface — renders feature state for headless / CLI output.
//!
//! This module was extracted from the former TUI module. It contains only the
//! data model and text rendering logic; no terminal UI (crossterm) dependencies.

use crate::state::{
    AgentSessionStatus, EvidenceStatus, FeatureState, GateGroupStatus, GateStatus,
    GithubMergeability, GithubReviewStatus, WorkflowState,
};

// ── Public types ────────────────────────────────────────────────────────────

/// A pending clarification question visible in the operator surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingClarification {
    pub session_id: String,
    pub question: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorSurface {
    feature_id: String,
    branch: String,
    workflow: String,
    pull_request_number: u64,
    github: Option<GithubView>,
    github_error: Option<String>,
    blocking_gate_ids: Vec<String>,
    gate_groups: Vec<GateGroupView>,
    sessions: Vec<SessionView>,
    pending_clarifications: Vec<PendingClarification>,
    queued_follow_ups: Vec<String>,
    pub last_event: String,
}

impl OperatorSurface {
    pub fn from_feature_state(feature: &FeatureState) -> Self {
        let pending_clarifications = pending_clarifications_from_feature(feature);

        Self {
            feature_id: feature.feature_id.clone(),
            branch: feature.branch.clone(),
            workflow: workflow_label(feature.workflow_state.clone()),
            pull_request_number: feature.pull_request.number,
            github: feature.github_snapshot.as_ref().map(|snapshot| GithubView {
                pr_state: if snapshot.is_draft {
                    "draft".to_string()
                } else {
                    "ready-for-review".to_string()
                },
                review: github_review_label(&snapshot.review_status).to_string(),
                checks: evidence_status_label(&snapshot.checks).to_string(),
                mergeability: github_mergeability_label(&snapshot.mergeability).to_string(),
            }),
            github_error: feature.github_error.clone(),
            blocking_gate_ids: feature.blocking_gate_ids(),
            gate_groups: feature
                .gate_groups
                .iter()
                .map(|group| {
                    let rollup_status = group.rollup_status();
                    GateGroupView {
                        label: group.label.clone(),
                        group_status: gate_group_status_label(rollup_status).to_string(),
                        gates: group
                            .gates
                            .iter()
                            .map(|gate| {
                                let is_blocking = gate.status != GateStatus::Passing;
                                GateView {
                                    label: gate.label.clone(),
                                    status: gate_status_label(gate.status.clone()).to_string(),
                                    is_blocking,
                                }
                            })
                            .collect(),
                    }
                })
                .collect(),
            sessions: feature
                .active_sessions
                .iter()
                .map(|session| SessionView {
                    role: session.role.clone(),
                    session_id: session.session_id.clone(),
                    status: session_status_label(session.status.clone()).to_string(),
                    output: if session.output.is_empty() {
                        vec!["No streamed output yet.".to_string()]
                    } else {
                        session
                            .output
                            .iter()
                            .map(|event| event.text.clone())
                            .collect()
                    },
                })
                .collect(),
            pending_clarifications,
            queued_follow_ups: feature
                .active_sessions
                .iter()
                .flat_map(|session| session.pending_follow_ups.iter().cloned())
                .collect(),
            last_event: "idle".to_string(),
        }
    }

    pub fn render(&self) -> String {
        let mut lines = vec![
            "┌─ Calypso ──────────────────────────────────────────────────────────────────┐"
                .to_string(),
            format!("│ Feature: {:<66} │", self.feature_id),
            format!(
                "│ Branch:  {:<30}  PR: #{:<29} │",
                self.branch, self.pull_request_number
            ),
            "└────────────────────────────────────────────────────────────────────────────┘"
                .to_string(),
            String::new(),
        ];

        // State machine pipeline
        lines.push(render_workflow_pipeline(&self.workflow));
        lines.push(String::new());

        // Status row
        let blocking_str = if self.blocking_gate_ids.is_empty() {
            "none".to_string()
        } else {
            self.blocking_gate_ids.join(", ")
        };
        lines.push(format!(
            "  Follow-ups queued: {}   Blocking: {}   Last event: {}",
            self.queued_follow_ups.len(),
            blocking_str,
            self.last_event,
        ));

        // GitHub
        if let Some(github) = &self.github {
            lines.push(String::new());
            lines.push(format!(
                "  GitHub  PR: {}  Review: {}  Checks: {}  Merge: {}",
                github.pr_state, github.review, github.checks, github.mergeability
            ));
        } else if let Some(error) = &self.github_error {
            lines.push(String::new());
            lines.push(format!("  GitHub  error: {error}"));
        }

        // Gate groups
        if !self.gate_groups.is_empty() {
            lines.push(String::new());
            lines.push("  Gates".to_string());
            lines.push("  ─────────────────────────────────────────────────".to_string());
            for group in &self.gate_groups {
                let group_icon = match group.group_status.as_str() {
                    "passing" => "✓",
                    "blocked" => "✗",
                    "manual" => "◆",
                    _ => "○",
                };
                lines.push(format!("  {} {}:", group_icon, group.label));
                for gate in &group.gates {
                    let gate_icon = match gate.status.as_str() {
                        "passing" => "  ✓",
                        "failing" => "  ✗",
                        "manual" => "  ◆",
                        _ => "  ○",
                    };
                    let blocking_marker = if gate.is_blocking { " ⚠" } else { "" };
                    lines.push(format!(
                        "  {}  {}{}",
                        gate_icon, gate.label, blocking_marker
                    ));
                }
            }
        }

        // Pending clarifications
        if !self.pending_clarifications.is_empty() {
            lines.push(String::new());
            lines.push("  Pending Clarifications".to_string());
            lines.push("  ─────────────────────────────────────────────────".to_string());
            for clarification in &self.pending_clarifications {
                lines.push(format!(
                    "  [{}] {}",
                    clarification.session_id, clarification.question
                ));
            }
        }

        // Active sessions
        lines.push(String::new());
        lines.push("  Active Sessions".to_string());
        lines.push("  ─────────────────────────────────────────────────".to_string());
        if self.sessions.is_empty() {
            lines.push("  No active sessions".to_string());
        } else {
            for session in &self.sessions {
                let status_icon = match session.status.as_str() {
                    "running" => "▶",
                    "completed" => "✓",
                    "failed" => "✗",
                    "aborted" => "⊗",
                    _ => "○",
                };
                lines.push(format!(
                    "  {} {} ({}) [{}]",
                    status_icon, session.role, session.session_id, session.status
                ));
                for output in &session.output {
                    lines.push(format!("    {output}"));
                }
            }
        }

        lines.push(String::new());
        lines.join("\n")
    }
}

// ── Private view types ──────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubView {
    pr_state: String,
    review: String,
    checks: String,
    mergeability: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GateGroupView {
    label: String,
    group_status: String,
    gates: Vec<GateView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GateView {
    label: String,
    status: String,
    is_blocking: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionView {
    role: String,
    session_id: String,
    status: String,
    output: Vec<String>,
}

// ── Label helpers ───────────────────────────────────────────────────────────

fn workflow_label(state: WorkflowState) -> String {
    state.as_str().to_string()
}

/// Render a one-line visual pipeline showing the current position in the workflow.
fn render_workflow_pipeline(current: &str) -> String {
    const PIPELINE: &[(&str, &str)] = &[
        ("new", "new"),
        ("prd-review", "prd"),
        ("architecture-plan", "arch"),
        ("scaffold-tdd", "tdd"),
        ("architecture-review", "rev"),
        ("implementation", "impl"),
        ("qa-validation", "qa"),
        ("release-ready", "rel"),
        ("done", "done"),
    ];

    let current_pos = PIPELINE.iter().position(|(s, _)| *s == current);
    let nodes: Vec<String> = PIPELINE
        .iter()
        .enumerate()
        .map(|(i, (_, label))| match current_pos {
            Some(pos) if i < pos => format!("✓{label}"),
            Some(pos) if i == pos => format!("●{label}"),
            _ => format!("○{label}"),
        })
        .collect();

    let flow = nodes.join(" → ");

    match current {
        "blocked" => format!("  {flow}\n  ⚠  state: blocked"),
        "aborted" => format!("  {flow}\n  ✗  state: aborted"),
        _ => format!("  {flow}"),
    }
}

fn gate_status_label(status: GateStatus) -> &'static str {
    match status {
        GateStatus::Pending => "pending",
        GateStatus::Passing => "passing",
        GateStatus::Failing => "failing",
        GateStatus::Manual => "manual",
    }
}

fn gate_group_status_label(status: GateGroupStatus) -> &'static str {
    match status {
        GateGroupStatus::Passing => "passing",
        GateGroupStatus::Pending => "pending",
        GateGroupStatus::Manual => "manual",
        GateGroupStatus::Blocked => "blocked",
    }
}

fn session_status_label(status: AgentSessionStatus) -> &'static str {
    match status {
        AgentSessionStatus::Running => "running",
        AgentSessionStatus::WaitingForHuman => "waiting-for-human",
        AgentSessionStatus::Completed => "completed",
        AgentSessionStatus::Failed => "failed",
        AgentSessionStatus::Aborted => "aborted",
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

fn pending_clarifications_from_feature(feature: &FeatureState) -> Vec<PendingClarification> {
    feature
        .clarification_history
        .iter()
        .filter(|e| e.answer.is_none())
        .map(|e| PendingClarification {
            session_id: e.session_id.clone(),
            question: e.question.clone(),
        })
        .collect()
}
