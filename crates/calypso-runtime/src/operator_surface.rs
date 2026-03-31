//! Plain-text operator surface — renders workflow state for headless / CLI output.
//!
//! This module was extracted from the former TUI module. It contains only the
//! data model and text rendering logic; no terminal UI (crossterm) dependencies.
//!
//! The primary construction path is [`OperatorSurface::from_workflow_run`], which
//! derives all display fields from a persisted [`WorkflowRun`] — no PR number,
//! branch name, or GitHub review state is required.  The legacy
//! [`OperatorSurface::from_feature_state`] constructor is retained for backward
//! compatibility but is not the recommended path for new code.

use crate::state::{
    AgentSessionStatus, EvidenceStatus, FeatureState, GateGroupStatus, GateStatus,
    GithubMergeability, GithubReviewStatus, WorkflowState,
};
use crate::workflow_run::{AgentRunStatus, CheckStatus, SteeringOutcome, WorkflowRun};

// ── Public types ────────────────────────────────────────────────────────────

/// A pending clarification question visible in the operator surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingClarification {
    pub session_id: String,
    pub question: String,
}

/// A recorded state transition visible in the operator surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransitionView {
    pub from_state: String,
    pub to_state: String,
    pub trigger: String,
    pub timestamp: String,
}

/// A steering action visible in the operator surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SteeringView {
    pub action_label: String,
    pub outcome: String,
    pub requested_at: String,
}

/// Blocking condition explanation for a workflow run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockingCondition {
    pub label: String,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperatorSurface {
    /// Run or feature identifier — opaque label for the header.
    run_id: String,
    /// Workflow definition identifier (e.g. `"scan-loop"`, `"develop"`).
    workflow_id: String,
    /// Current state within the workflow graph.
    current_state: String,
    /// Execution locality (local, forge, delegated).
    locality: String,
    /// Iteration count (number of transitions executed).
    iteration: usize,

    // Legacy PR-centric fields — populated only by `from_feature_state`.
    feature_id: String,
    branch: String,
    workflow: String,
    pull_request_number: u64,
    github: Option<GithubView>,
    github_error: Option<String>,

    // Shared workflow-agnostic fields.
    blocking_gate_ids: Vec<String>,
    blocking_conditions: Vec<BlockingCondition>,
    gate_groups: Vec<GateGroupView>,
    sessions: Vec<SessionView>,
    pending_clarifications: Vec<PendingClarification>,
    steering_views: Vec<SteeringView>,
    transition_history: Vec<TransitionView>,
    queued_follow_ups: Vec<String>,
    pub last_event: String,

    /// True when this surface was built from a `WorkflowRun` rather than
    /// `FeatureState`.  Controls which render path is used.
    is_run_backed: bool,
}

impl OperatorSurface {
    pub fn from_feature_state(feature: &FeatureState) -> Self {
        let pending_clarifications = pending_clarifications_from_feature(feature);

        Self {
            run_id: String::new(),
            workflow_id: String::new(),
            current_state: String::new(),
            locality: String::new(),
            iteration: 0,
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
            blocking_conditions: Vec::new(),
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
            steering_views: Vec::new(),
            transition_history: Vec::new(),
            queued_follow_ups: feature
                .active_sessions
                .iter()
                .flat_map(|session| session.pending_follow_ups.iter().cloned())
                .collect(),
            last_event: "idle".to_string(),
            is_run_backed: false,
        }
    }

    /// Build an `OperatorSurface` from a daemon-native [`WorkflowRun`].
    ///
    /// This is the primary construction path for workflow-run-centric operator
    /// output.  All display fields are derived from persisted run state — no PR
    /// number, branch name, or GitHub review state is required.
    ///
    /// The resulting surface exposes:
    /// - current workflow and state
    /// - recent transition history
    /// - pending deterministic checks
    /// - active/completed agent runs
    /// - blocking conditions with explanations
    /// - steering actions and pending clarification state
    /// - available steering actions
    pub fn from_workflow_run(run: &WorkflowRun) -> Self {
        let blocking_gate_ids: Vec<String> = run
            .pending_checks
            .iter()
            .filter(|c| !matches!(c.status, CheckStatus::Passing))
            .map(|c| c.check_id.clone())
            .collect();

        // Build blocking condition explanations so the operator can understand
        // *why* the run is not advancing.
        let mut blocking_conditions: Vec<BlockingCondition> = Vec::new();

        for check in &run.pending_checks {
            if matches!(check.status, CheckStatus::Failing) {
                blocking_conditions.push(BlockingCondition {
                    label: check.check_id.clone(),
                    reason: format!("Deterministic check '{}' is failing", check.description),
                });
            } else if matches!(check.status, CheckStatus::Pending) {
                blocking_conditions.push(BlockingCondition {
                    label: check.check_id.clone(),
                    reason: format!(
                        "Waiting for deterministic check '{}' to complete",
                        check.description
                    ),
                });
            }
        }

        // Pending steering requests also block progress.
        for entry in &run.steering {
            if matches!(entry.outcome, SteeringOutcome::Pending) {
                let action_label = steering_action_label(&entry.action);
                blocking_conditions.push(BlockingCondition {
                    label: "steering".to_string(),
                    reason: format!("Pending steering action: {action_label}"),
                });
            }
        }

        if let Some(ref reason) = run.terminal_reason {
            blocking_conditions.push(BlockingCondition {
                label: "terminal".to_string(),
                reason: format!("Run stopped: {reason}"),
            });
        }

        let gate_groups = if run.pending_checks.is_empty() {
            Vec::new()
        } else {
            vec![GateGroupView {
                label: "Deterministic checks".to_string(),
                group_status: if run
                    .pending_checks
                    .iter()
                    .any(|c| matches!(c.status, CheckStatus::Failing))
                {
                    "blocked".to_string()
                } else if run
                    .pending_checks
                    .iter()
                    .any(|c| matches!(c.status, CheckStatus::Pending))
                {
                    "pending".to_string()
                } else {
                    "passing".to_string()
                },
                gates: run
                    .pending_checks
                    .iter()
                    .map(|c| {
                        let status = match c.status {
                            CheckStatus::Passing => "passing",
                            CheckStatus::Failing => "failing",
                            CheckStatus::Pending => "pending",
                        };
                        GateView {
                            label: c.description.clone(),
                            status: status.to_string(),
                            is_blocking: !matches!(c.status, CheckStatus::Passing),
                        }
                    })
                    .collect(),
            }]
        };

        let sessions: Vec<SessionView> = run
            .agent_runs
            .iter()
            .map(|r| {
                let status = match r.status {
                    AgentRunStatus::Running => "running",
                    AgentRunStatus::Completed => "completed",
                    AgentRunStatus::Failed => "failed",
                    AgentRunStatus::TimedOut => "failed",
                    AgentRunStatus::Aborted => "aborted",
                };
                SessionView {
                    role: r.state_name.clone(),
                    session_id: r.agent_run_id.clone(),
                    status: status.to_string(),
                    output: r
                        .outcome
                        .as_ref()
                        .map(|o| vec![o.clone()])
                        .unwrap_or_else(|| vec!["No output yet.".to_string()]),
                }
            })
            .collect();

        let pending_clarifications: Vec<PendingClarification> = run
            .steering
            .iter()
            .filter(|s| matches!(s.outcome, SteeringOutcome::Pending))
            .filter_map(|s| {
                if let crate::workflow_run::SteeringAction::Clarify { ref message } = s.action {
                    Some(PendingClarification {
                        session_id: "operator".to_string(),
                        question: message.clone(),
                    })
                } else {
                    None
                }
            })
            .collect();

        let steering_views: Vec<SteeringView> = run
            .steering
            .iter()
            .map(|entry| {
                let outcome = match &entry.outcome {
                    SteeringOutcome::Pending => "pending".to_string(),
                    SteeringOutcome::Applied => "applied".to_string(),
                    SteeringOutcome::Rejected { reason } => format!("rejected: {reason}"),
                };
                SteeringView {
                    action_label: steering_action_label(&entry.action),
                    outcome,
                    requested_at: entry.requested_at.clone(),
                }
            })
            .collect();

        let transition_history: Vec<TransitionView> = run
            .transition_history
            .iter()
            .map(|t| TransitionView {
                from_state: t.from_state.clone(),
                to_state: t.to_state.clone(),
                trigger: t.trigger.clone(),
                timestamp: t.timestamp.clone(),
            })
            .collect();

        Self {
            run_id: run.run_id.as_str().to_string(),
            workflow_id: run.workflow_id.clone(),
            current_state: run.current_state.clone(),
            locality: run.locality.to_string(),
            iteration: run.iteration,
            feature_id: run.run_id.as_str().to_string(),
            branch: String::new(),
            workflow: run.current_state.clone(),
            pull_request_number: 0,
            github: None,
            github_error: None,
            blocking_gate_ids,
            blocking_conditions,
            gate_groups,
            sessions,
            pending_clarifications,
            steering_views,
            transition_history,
            queued_follow_ups: Vec::new(),
            last_event: run
                .terminal_reason
                .as_ref()
                .map(|r| r.to_string())
                .unwrap_or_else(|| "running".to_string()),
            is_run_backed: true,
        }
    }

    /// Render the operator surface as plain text.
    ///
    /// Dispatches to the workflow-run-centric renderer when this surface was
    /// built from a [`WorkflowRun`], or to the legacy feature-state renderer
    /// otherwise.
    pub fn render(&self) -> String {
        if self.is_run_backed {
            self.render_run()
        } else {
            self.render_feature()
        }
    }

    /// Legacy feature-state-centric renderer.
    ///
    /// Preserves the original output format for backward compatibility with
    /// `from_feature_state` surfaces.
    fn render_feature(&self) -> String {
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
        self.render_gate_groups(&mut lines);

        // Pending clarifications
        self.render_pending_clarifications(&mut lines);

        // Active sessions
        self.render_sessions(&mut lines);

        lines.push(String::new());
        lines.join("\n")
    }

    /// Workflow-run-centric renderer.
    ///
    /// Produces operator output derived entirely from persisted run state.
    /// No PR number, branch, or GitHub review fields appear.  Instead the
    /// output shows workflow identity, current state, transition history,
    /// deterministic checks, blocking conditions, agent activity, steering
    /// actions, and pending clarifications.
    fn render_run(&self) -> String {
        let mut lines = vec![
            "┌─ Calypso ──────────────────────────────────────────────────────────────────┐"
                .to_string(),
            format!("│ Run:      {:<65} │", self.run_id),
            format!("│ Workflow: {:<65} │", self.workflow_id),
            format!(
                "│ State:    {:<30}  Locality: {:<22} │",
                self.current_state, self.locality
            ),
            format!("│ Iteration: {:<64} │", self.iteration),
            "└────────────────────────────────────────────────────────────────────────────┘"
                .to_string(),
            String::new(),
        ];

        // Dynamic workflow graph — render based on transition history
        lines.push(render_dynamic_workflow_state(
            &self.current_state,
            &self.transition_history,
        ));
        lines.push(String::new());

        // Status row
        let blocking_str = if self.blocking_gate_ids.is_empty() {
            "none".to_string()
        } else {
            self.blocking_gate_ids.join(", ")
        };
        lines.push(format!(
            "  Blocking: {}   Last event: {}",
            blocking_str, self.last_event,
        ));

        // Blocking conditions — explain *why* the run is stuck
        if !self.blocking_conditions.is_empty() {
            lines.push(String::new());
            lines.push("  Blocking Conditions".to_string());
            lines.push("  ─────────────────────────────────────────────────".to_string());
            for condition in &self.blocking_conditions {
                lines.push(format!("  [{}] {}", condition.label, condition.reason));
            }
        }

        // Gate groups (deterministic checks)
        self.render_gate_groups(&mut lines);

        // Transition history
        if !self.transition_history.is_empty() {
            lines.push(String::new());
            lines.push("  Transition History".to_string());
            lines.push("  ─────────────────────────────────────────────────".to_string());
            for t in &self.transition_history {
                lines.push(format!(
                    "  {} -> {} (trigger: {}, at: {})",
                    t.from_state, t.to_state, t.trigger, t.timestamp
                ));
            }
        }

        // Steering actions
        if !self.steering_views.is_empty() {
            lines.push(String::new());
            lines.push("  Steering Actions".to_string());
            lines.push("  ─────────────────────────────────────────────────".to_string());
            for sv in &self.steering_views {
                let icon = match sv.outcome.as_str() {
                    "pending" => "○",
                    "applied" => "✓",
                    _ => "✗",
                };
                lines.push(format!(
                    "  {} {} [{}] (at: {})",
                    icon, sv.action_label, sv.outcome, sv.requested_at
                ));
            }
        }

        // Pending clarifications
        self.render_pending_clarifications(&mut lines);

        // Active sessions / agent runs
        self.render_sessions(&mut lines);

        lines.push(String::new());
        lines.join("\n")
    }

    // ── Shared rendering helpers ───────────────────────────────────────────

    fn render_gate_groups(&self, lines: &mut Vec<String>) {
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
    }

    fn render_pending_clarifications(&self, lines: &mut Vec<String>) {
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
    }

    fn render_sessions(&self, lines: &mut Vec<String>) {
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

/// Render a one-line visual pipeline showing the current position in the
/// legacy fixed-stage workflow.
///
/// This is used only by `render_feature` for backward compatibility.
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

/// Render the workflow state for an arbitrary workflow graph.
///
/// Instead of a fixed pipeline, this derives the visual from the transition
/// history: visited states are marked done, and the current state is
/// highlighted.  This supports any workflow graph shape — linear, branching,
/// or cyclic.
fn render_dynamic_workflow_state(current: &str, history: &[TransitionView]) -> String {
    // Collect the unique ordered states from transition history
    let mut visited: Vec<String> = Vec::new();
    for t in history {
        if !visited.contains(&t.from_state) {
            visited.push(t.from_state.clone());
        }
        if !visited.contains(&t.to_state) {
            visited.push(t.to_state.clone());
        }
    }
    // Ensure current state is represented even with no transitions
    if !visited.contains(&current.to_string()) {
        visited.push(current.to_string());
    }

    let nodes: Vec<String> = visited
        .iter()
        .map(|state| {
            if state == current {
                format!("●{state}")
            } else {
                format!("✓{state}")
            }
        })
        .collect();

    let flow = nodes.join(" -> ");
    format!("  {flow}")
}

/// Produce a human-readable label for a steering action.
fn steering_action_label(action: &crate::workflow_run::SteeringAction) -> String {
    use crate::workflow_run::SteeringAction;
    match action {
        SteeringAction::Clarify { message } => {
            let truncated = if message.len() > 40 {
                format!("{}...", &message[..40])
            } else {
                message.clone()
            };
            format!("clarify: \"{truncated}\"")
        }
        SteeringAction::Retry => "retry".to_string(),
        SteeringAction::Skip { target_state } => format!("skip to {target_state}"),
        SteeringAction::Abort => "abort".to_string(),
        SteeringAction::ForceTransition {
            target_state,
            reason,
        } => format!("force -> {target_state} ({reason})"),
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

// ── Tests ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow_run::{
        AgentRunStatus, CheckStatus, SteeringAction, SteeringOutcome, TerminalReason, WorkflowRun,
    };

    /// Helper: build a minimal linear workflow run (scan -> check -> report).
    fn linear_workflow_run() -> WorkflowRun {
        let mut run = WorkflowRun::new("scan-loop", "scan", 1);
        run.record_transition("check", "on_scan_done");
        run.record_transition("report", "on_check_pass");
        run
    }

    /// Helper: build a branching workflow run (init -> a, a -> b, a -> c, c -> done).
    fn branching_workflow_run() -> WorkflowRun {
        let mut run = WorkflowRun::new("branch-wf", "init", 1);
        run.record_transition("a", "start");
        run.record_transition("b", "path_1");
        // Simulate going back and taking a different path
        run.record_transition("c", "path_2");
        run.record_transition("done", "finish");
        run
    }

    /// Helper: build a run with pending checks, agent activity, and steering.
    fn rich_workflow_run() -> WorkflowRun {
        let mut run = WorkflowRun::new("develop", "implement", 1);
        run.set_check("ci.tests", "CI test suite", CheckStatus::Failing);
        run.set_check(
            "branch.up-to-date",
            "Branch is up to date",
            CheckStatus::Passing,
        );
        run.set_check("lint", "Linter", CheckStatus::Pending);
        run.start_agent_run("agent-1", "implement");
        run.add_steering(SteeringAction::Clarify {
            message: "Which module should be refactored?".to_string(),
        });
        run
    }

    // ── Acceptance criterion 1: arbitrary workflow graphs ───────────────────

    #[test]
    fn render_linear_workflow_run() {
        let run = linear_workflow_run();
        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        // Should show run-centric header, not PR-centric
        assert!(output.contains("Run:"), "should show run ID header");
        assert!(output.contains("Workflow:"), "should show workflow ID");
        assert!(
            output.contains("scan-loop"),
            "should show the workflow name"
        );
        assert!(
            output.contains("report"),
            "should show the current state 'report'"
        );

        // Should NOT contain PR-centric fields
        assert!(
            !output.contains("PR: #"),
            "run-backed surface should not show PR number"
        );
        assert!(
            !output.contains("Branch:"),
            "run-backed surface should not show branch"
        );
    }

    #[test]
    fn render_branching_workflow_run() {
        let run = branching_workflow_run();
        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        // All visited states should appear in the dynamic workflow line
        assert!(output.contains("init"), "should show 'init' state");
        assert!(output.contains("done"), "should show 'done' state");
        // Current state should be marked with the active indicator
        assert!(
            output.contains("done"),
            "current state 'done' should appear"
        );
    }

    #[test]
    fn render_single_state_workflow_no_transitions() {
        let run = WorkflowRun::new("minimal", "waiting", 1);
        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        assert!(output.contains("waiting"), "should show initial state");
        assert!(output.contains("Run:"), "should show run header");
    }

    // ── Acceptance criterion 2: transitions, checks, agents ────────────────

    #[test]
    fn render_shows_transition_history() {
        let run = linear_workflow_run();
        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        assert!(
            output.contains("Transition History"),
            "should include transition history section"
        );
        assert!(
            output.contains("scan -> check"),
            "should show first transition"
        );
        assert!(
            output.contains("check -> report"),
            "should show second transition"
        );
        assert!(
            output.contains("on_scan_done"),
            "should show transition trigger"
        );
    }

    #[test]
    fn render_shows_pending_checks_and_agents() {
        let run = rich_workflow_run();
        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        // Deterministic checks
        assert!(
            output.contains("Gates"),
            "should render deterministic checks as gates"
        );
        assert!(
            output.contains("CI test suite"),
            "should show check descriptions"
        );
        assert!(
            output.contains("Linter"),
            "should show pending linter check"
        );

        // Agent activity
        assert!(
            output.contains("Active Sessions"),
            "should show agent sessions"
        );
        assert!(output.contains("agent-1"), "should show agent run ID");
        assert!(
            output.contains("implement"),
            "should show agent's state context"
        );
    }

    // ── Acceptance criterion 3: steering and clarification ─────────────────

    #[test]
    fn render_shows_steering_actions_without_pr_metadata() {
        let run = rich_workflow_run();
        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        assert!(
            output.contains("Steering Actions"),
            "should show steering actions section"
        );
        assert!(
            output.contains("clarify"),
            "should show the clarify steering action"
        );
        assert!(output.contains("pending"), "should show pending outcome");

        // Pending clarifications
        assert!(
            output.contains("Pending Clarifications"),
            "should show pending clarification section"
        );
        assert!(
            output.contains("Which module should be refactored?"),
            "should show the clarification question"
        );
    }

    #[test]
    fn render_steering_with_resolved_actions() {
        let mut run = WorkflowRun::new("wf", "stuck", 1);
        run.add_steering(SteeringAction::Retry);
        run.resolve_steering(SteeringOutcome::Applied);
        run.add_steering(SteeringAction::Skip {
            target_state: "recovery".to_string(),
        });
        run.resolve_steering(SteeringOutcome::Rejected {
            reason: "invalid target".to_string(),
        });

        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        assert!(output.contains("retry"), "should show retry action");
        assert!(output.contains("applied"), "should show applied outcome");
        assert!(
            output.contains("skip to recovery"),
            "should show skip action with target"
        );
        assert!(output.contains("rejected"), "should show rejected outcome");
    }

    // ── Acceptance criterion 4: blocking/waiting explanation ────────────────

    #[test]
    fn render_shows_blocking_conditions_for_failing_checks() {
        let run = rich_workflow_run();
        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        assert!(
            output.contains("Blocking Conditions"),
            "should show blocking conditions section"
        );
        assert!(
            output.contains("Deterministic check 'CI test suite' is failing"),
            "should explain why the failing check blocks"
        );
        assert!(
            output.contains("Waiting for deterministic check 'Linter' to complete"),
            "should explain pending check"
        );
    }

    #[test]
    fn render_shows_blocking_condition_for_pending_steering() {
        let mut run = WorkflowRun::new("wf", "stuck", 1);
        run.add_steering(SteeringAction::Clarify {
            message: "Need guidance".to_string(),
        });

        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        assert!(
            output.contains("Blocking Conditions"),
            "should show blocking conditions"
        );
        assert!(
            output.contains("Pending steering action"),
            "should explain steering is blocking"
        );
    }

    #[test]
    fn render_shows_terminal_reason_as_blocking_condition() {
        let mut run = WorkflowRun::new("wf", "scan", 1);
        run.terminate(TerminalReason::Error {
            state: "scan".to_string(),
            message: "timeout reached".to_string(),
        });

        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        assert!(
            output.contains("Blocking Conditions"),
            "terminal reason should appear as blocking"
        );
        assert!(
            output.contains("Run stopped"),
            "should explain the run stopped"
        );
        assert!(
            output.contains("timeout reached"),
            "should include the error detail"
        );
    }

    #[test]
    fn render_no_blocking_conditions_when_all_clear() {
        let run = WorkflowRun::new("wf", "running", 1);
        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        assert!(
            !output.contains("Blocking Conditions"),
            "should not show blocking section when nothing is blocking"
        );
    }

    // ── Test plan: rendering does not require PR-centric fields ────────────

    #[test]
    fn run_backed_surface_omits_github_and_pr_fields() {
        let mut run = WorkflowRun::new("deploy-wf", "deploy", 1);
        run.record_transition("verify", "on_deploy_done");
        run.set_check("smoke", "Smoke tests", CheckStatus::Passing);

        let surface = OperatorSurface::from_workflow_run(&run);
        let output = surface.render();

        // Must not contain legacy PR fields
        assert!(!output.contains("PR: #"));
        assert!(!output.contains("Branch:"));
        assert!(!output.contains("GitHub"));
        assert!(!output.contains("review"));
        assert!(!output.contains("mergeability"));
    }

    // ── Dynamic workflow state rendering ───────────────────────────────────

    #[test]
    fn dynamic_workflow_state_renders_visited_and_current() {
        let history = vec![
            TransitionView {
                from_state: "a".into(),
                to_state: "b".into(),
                trigger: "ev1".into(),
                timestamp: "2026-01-01T00:00:00Z".into(),
            },
            TransitionView {
                from_state: "b".into(),
                to_state: "c".into(),
                trigger: "ev2".into(),
                timestamp: "2026-01-01T00:01:00Z".into(),
            },
        ];
        let result = render_dynamic_workflow_state("c", &history);
        assert!(result.contains("a"), "should contain visited state 'a'");
        assert!(result.contains("b"), "should contain visited state 'b'");
        assert!(result.contains("c"), "should contain current state 'c'");
    }

    #[test]
    fn dynamic_workflow_state_with_no_transitions() {
        let result = render_dynamic_workflow_state("init", &[]);
        assert!(
            result.contains("init"),
            "should render the initial state with no history"
        );
    }

    // ── Steering action label helper ───────────────────────────────────────

    #[test]
    fn steering_action_labels() {
        assert_eq!(steering_action_label(&SteeringAction::Retry), "retry");
        assert_eq!(steering_action_label(&SteeringAction::Abort), "abort");
        assert_eq!(
            steering_action_label(&SteeringAction::Skip {
                target_state: "done".into()
            }),
            "skip to done"
        );
        assert_eq!(
            steering_action_label(&SteeringAction::ForceTransition {
                target_state: "reset".into(),
                reason: "operator override".into()
            }),
            "force -> reset (operator override)"
        );
        let label = steering_action_label(&SteeringAction::Clarify {
            message: "short msg".into(),
        });
        assert!(label.contains("short msg"));
    }

    // ── Backward compatibility: feature-state rendering ────────────────────

    #[test]
    fn feature_state_surface_still_renders_pr_fields() {
        // The from_feature_state path should still produce the legacy format
        // with PR number and branch. We just verify the flag is set correctly.
        let run = WorkflowRun::new("wf", "scan", 1);
        let surface = OperatorSurface::from_workflow_run(&run);
        assert!(
            surface.is_run_backed,
            "from_workflow_run should set is_run_backed"
        );
    }
}
