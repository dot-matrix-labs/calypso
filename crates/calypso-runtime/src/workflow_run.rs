//! Daemon-native workflow run schema and persistence model.
//!
//! A `WorkflowRun` is the first-class runtime object that Calypso persists and
//! resumes.  It replaces the fixed feature/PR lifecycle as the source of truth
//! for daemon execution state.
//!
//! # Canonical fields (PRD §5.5)
//!
//! - **run_id** — unique identifier for this execution attempt
//! - **workflow_id** — which workflow definition is being executed
//! - **current_state** — the state the machine is in right now
//! - **execution_locality** — where this run is executing (local, forge, etc.)
//! - **transition_history** — ordered list of state transitions with timestamps
//! - **pending_checks** — deterministic checks that must pass before advancing
//! - **steering** — operator steering requests and their outcomes
//! - **agent_runs** — metadata about agent sessions spawned during this run
//! - **terminal_reason** — why the run stopped (success, abort, error, etc.)
//!
//! # Persistence
//!
//! `WorkflowRun` serializes to JSON and is stored at
//! `<repo_root>/.calypso/workflow-run.json`.  On daemon restart the run is loaded
//! and execution resumes from `current_state`.

use std::fmt;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Run identity ────────────────────────────────────────────────────────────

/// A unique, opaque identifier for a single workflow run.
///
/// Generated as `<workflow_id>-<monotonic_counter>` or a short random suffix.
/// The format is intentionally simple — no UUID dependency required.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct RunId(pub String);

impl RunId {
    /// Create a new run ID from a workflow name and a sequence number.
    pub fn new(workflow_id: &str, seq: u64) -> Self {
        Self(format!("{workflow_id}-{seq}"))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for RunId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Execution locality ──────────────────────────────────────────────────────

/// Where a workflow run is executing.
///
/// Execution locality is runtime metadata per PRD §5.4 — it does not change the
/// workflow definition, only where steps physically run.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionLocality {
    /// Running under the local Calypso daemon.
    #[default]
    Local,
    /// Running on a git-forge action runner (e.g. GitHub Actions).
    Forge { runner: String },
    /// Delegated execution — the daemon tracks state but another system runs steps.
    Delegated { target: String },
}

impl fmt::Display for ExecutionLocality {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Local => f.write_str("local"),
            Self::Forge { runner } => write!(f, "forge:{runner}"),
            Self::Delegated { target } => write!(f, "delegated:{target}"),
        }
    }
}

// ── Transition history ──────────────────────────────────────────────────────

/// A single recorded state transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionRecord {
    /// The state transitioned from.
    pub from_state: String,
    /// The state transitioned to.
    pub to_state: String,
    /// The event or trigger that caused the transition.
    pub trigger: String,
    /// RFC 3339 timestamp of when the transition occurred.
    pub timestamp: String,
}

// ── Pending deterministic checks ────────────────────────────────────────────

/// A deterministic check that must pass before the run can advance.
///
/// These are the programmatic gates described in PRD §5.3 — CI checks, artifact
/// presence, branch state validation, etc.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PendingCheck {
    /// Identifier for the check (e.g. `"ci.tests"`, `"branch.up-to-date"`).
    pub check_id: String,
    /// Human-readable description.
    pub description: String,
    /// Current status of this check.
    pub status: CheckStatus,
    /// When the check was last evaluated.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_evaluated_at: Option<String>,
}

/// Status of a deterministic check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Pending,
    Passing,
    Failing,
}

// ── Steering ────────────────────────────────────────────────────────────────

/// An operator steering request and its outcome.
///
/// Per PRD §5.6, steering covers: clarification, retry, skip, abort, and
/// forced transitions with recorded operator intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SteeringEntry {
    /// What the operator asked to do.
    pub action: SteeringAction,
    /// RFC 3339 timestamp of the request.
    pub requested_at: String,
    /// Whether the steering action was applied.
    pub outcome: SteeringOutcome,
    /// RFC 3339 timestamp of when the outcome was recorded.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub resolved_at: Option<String>,
}

/// The type of steering action requested.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteeringAction {
    /// Provide clarification to a stuck agent step.
    Clarify { message: String },
    /// Retry the current step.
    Retry,
    /// Skip to an allowed recovery path.
    Skip { target_state: String },
    /// Abort the entire workflow run.
    Abort,
    /// Force a transition with recorded operator intent.
    ForceTransition {
        target_state: String,
        reason: String,
    },
}

/// Outcome of a steering action.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SteeringOutcome {
    /// The steering action is pending — not yet applied.
    Pending,
    /// The steering action was applied successfully.
    Applied,
    /// The steering action was rejected (e.g. invalid target state).
    Rejected { reason: String },
}

// ── Agent run metadata ──────────────────────────────────────────────────────

/// Metadata about a single agent invocation within a workflow run.
///
/// Per PRD §7.3, Calypso supervises agent work through headless sessions,
/// capturing outcomes and enforcing timeouts.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentRunRecord {
    /// Identifier for this agent run (typically a session ID).
    pub agent_run_id: String,
    /// The workflow state that spawned this agent run.
    pub state_name: String,
    /// Status of the agent run.
    pub status: AgentRunStatus,
    /// RFC 3339 timestamp of when the agent run started.
    pub started_at: String,
    /// RFC 3339 timestamp of when the agent run completed (if finished).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub completed_at: Option<String>,
    /// Structured outcome from the agent, if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outcome: Option<String>,
}

/// Status of an agent run within a workflow.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRunStatus {
    Running,
    Completed,
    Failed,
    TimedOut,
    Aborted,
}

// ── Terminal / interruption reasons ─────────────────────────────────────────

/// Why a workflow run stopped.
///
/// Per PRD §7.8, these are first-class states: success, abort, error, timeout,
/// interruption, and retry are all distinguished.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TerminalReason {
    /// The workflow reached a terminal success state.
    Success { terminal_state: String },
    /// The workflow was aborted by an operator or policy.
    Aborted { reason: String },
    /// The workflow encountered a fatal error.
    Error { state: String, message: String },
    /// The workflow was interrupted by a signal (SIGINT, SIGTERM).
    Interrupted { signal: String },
    /// The workflow hit its step/time limit.
    Timeout { detail: String },
    /// The workflow is being retried from an earlier state.
    Retry {
        from_state: String,
        to_state: String,
    },
}

impl fmt::Display for TerminalReason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Success { terminal_state } => {
                write!(f, "success (terminal state: {terminal_state})")
            }
            Self::Aborted { reason } => write!(f, "aborted: {reason}"),
            Self::Error { state, message } => write!(f, "error at '{state}': {message}"),
            Self::Interrupted { signal } => write!(f, "interrupted by {signal}"),
            Self::Timeout { detail } => write!(f, "timeout: {detail}"),
            Self::Retry {
                from_state,
                to_state,
            } => write!(f, "retry: {from_state} -> {to_state}"),
        }
    }
}

// ── WorkflowRun ─────────────────────────────────────────────────────────────

/// A daemon-native workflow run — the first-class persisted runtime object.
///
/// This replaces the fixed feature lifecycle (`FeatureState` + `WorkflowState`)
/// as the source of truth for what the daemon is doing, what state it is in,
/// and what happened in the past.
///
/// Operator-facing inspection (PRD §6.4) is derived from this structure rather
/// than from log scraping or legacy feature metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowRun {
    /// Unique identifier for this run.
    pub run_id: RunId,

    /// Which workflow definition is being executed.
    pub workflow_id: String,

    /// The state the machine is currently in (or was in when it stopped).
    pub current_state: String,

    /// Where this run is executing.
    #[serde(default)]
    pub locality: ExecutionLocality,

    /// Ordered list of all state transitions that have occurred.
    #[serde(default)]
    pub transition_history: Vec<TransitionRecord>,

    /// Deterministic checks that must pass before advancing.
    #[serde(default)]
    pub pending_checks: Vec<PendingCheck>,

    /// Operator steering requests and their outcomes.
    #[serde(default)]
    pub steering: Vec<SteeringEntry>,

    /// Metadata about agent invocations during this run.
    #[serde(default)]
    pub agent_runs: Vec<AgentRunRecord>,

    /// Number of execution iterations completed.
    #[serde(default)]
    pub iteration: usize,

    /// RFC 3339 timestamp of when this run was created.
    pub created_at: String,

    /// RFC 3339 timestamp of the last state change or persistence event.
    pub updated_at: String,

    /// If the run has stopped, the reason why.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub terminal_reason: Option<TerminalReason>,
}

impl WorkflowRun {
    /// Create a new workflow run starting at the given initial state.
    pub fn new(workflow_id: &str, initial_state: &str, seq: u64) -> Self {
        let now = now_rfc3339();
        Self {
            run_id: RunId::new(workflow_id, seq),
            workflow_id: workflow_id.to_string(),
            current_state: initial_state.to_string(),
            locality: ExecutionLocality::default(),
            transition_history: Vec::new(),
            pending_checks: Vec::new(),
            steering: Vec::new(),
            agent_runs: Vec::new(),
            iteration: 0,
            created_at: now.clone(),
            updated_at: now,
            terminal_reason: None,
        }
    }

    /// Record a state transition and update current_state.
    pub fn record_transition(&mut self, to_state: &str, trigger: &str) {
        let now = now_rfc3339();
        self.transition_history.push(TransitionRecord {
            from_state: self.current_state.clone(),
            to_state: to_state.to_string(),
            trigger: trigger.to_string(),
            timestamp: now.clone(),
        });
        self.current_state = to_state.to_string();
        self.iteration += 1;
        self.updated_at = now;
    }

    /// Mark the run as terminally stopped.
    pub fn terminate(&mut self, reason: TerminalReason) {
        self.terminal_reason = Some(reason);
        self.updated_at = now_rfc3339();
    }

    /// Returns true if the run has stopped (has a terminal reason).
    pub fn is_stopped(&self) -> bool {
        self.terminal_reason.is_some()
    }

    /// Add or update a pending deterministic check.
    pub fn set_check(&mut self, check_id: &str, description: &str, status: CheckStatus) {
        let now = now_rfc3339();
        if let Some(existing) = self
            .pending_checks
            .iter_mut()
            .find(|c| c.check_id == check_id)
        {
            existing.status = status;
            existing.last_evaluated_at = Some(now);
        } else {
            self.pending_checks.push(PendingCheck {
                check_id: check_id.to_string(),
                description: description.to_string(),
                status,
                last_evaluated_at: Some(now),
            });
        }
        self.updated_at = now_rfc3339();
    }

    /// Record an agent run starting.
    pub fn start_agent_run(&mut self, agent_run_id: &str, state_name: &str) {
        let now = now_rfc3339();
        self.agent_runs.push(AgentRunRecord {
            agent_run_id: agent_run_id.to_string(),
            state_name: state_name.to_string(),
            status: AgentRunStatus::Running,
            started_at: now.clone(),
            completed_at: None,
            outcome: None,
        });
        self.updated_at = now;
    }

    /// Complete an agent run with a status and optional outcome.
    pub fn complete_agent_run(
        &mut self,
        agent_run_id: &str,
        status: AgentRunStatus,
        outcome: Option<String>,
    ) {
        let now = now_rfc3339();
        if let Some(run) = self
            .agent_runs
            .iter_mut()
            .find(|r| r.agent_run_id == agent_run_id)
        {
            run.status = status;
            run.completed_at = Some(now.clone());
            run.outcome = outcome;
        }
        self.updated_at = now;
    }

    /// Record a steering request.
    pub fn add_steering(&mut self, action: SteeringAction) {
        let now = now_rfc3339();
        self.steering.push(SteeringEntry {
            action,
            requested_at: now.clone(),
            outcome: SteeringOutcome::Pending,
            resolved_at: None,
        });
        self.updated_at = now;
    }

    /// Resolve the most recent pending steering entry.
    pub fn resolve_steering(&mut self, outcome: SteeringOutcome) {
        let now = now_rfc3339();
        if let Some(entry) = self
            .steering
            .iter_mut()
            .rev()
            .find(|e| matches!(e.outcome, SteeringOutcome::Pending))
        {
            entry.outcome = outcome;
            entry.resolved_at = Some(now.clone());
        }
        self.updated_at = now;
    }
}

// ── Persistence ─────────────────────────────────────────────────────────────

/// Default file name for the workflow run state.
const WORKFLOW_RUN_FILE: &str = "workflow-run.json";
/// Default directory for Calypso state.
const STATE_DIR: &str = ".calypso";

impl WorkflowRun {
    /// Return the default persistence path inside a repository root.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root.join(STATE_DIR).join(WORKFLOW_RUN_FILE)
    }

    /// Persist this run state to the given path.
    pub fn save(&self, path: &Path) -> Result<(), WorkflowRunError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(WorkflowRunError::Io)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(WorkflowRunError::Json)?;
        let tmp_path = path.with_extension("tmp");
        std::fs::write(&tmp_path, &json).map_err(WorkflowRunError::Io)?;
        std::fs::rename(&tmp_path, path).map_err(WorkflowRunError::Io)?;
        Ok(())
    }

    /// Load a workflow run from the given path.
    ///
    /// Returns `Ok(None)` if the file does not exist (fresh run).
    pub fn load(path: &Path) -> Result<Option<Self>, WorkflowRunError> {
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(path).map_err(WorkflowRunError::Io)?;
        let run: Self = serde_json::from_slice(&bytes).map_err(WorkflowRunError::Json)?;
        Ok(Some(run))
    }

    /// Delete the persistence file (after a clean terminal exit).
    pub fn clear(path: &Path) -> Result<(), WorkflowRunError> {
        if path.exists() {
            std::fs::remove_file(path).map_err(WorkflowRunError::Io)?;
        }
        Ok(())
    }
}

// ── Errors ──────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum WorkflowRunError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for WorkflowRunError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "workflow run I/O error: {e}"),
            Self::Json(e) => write!(f, "workflow run JSON error: {e}"),
        }
    }
}

impl std::error::Error for WorkflowRunError {}

// ── Timestamp helper ────────────────────────────────────────────────────────

/// Produce an RFC 3339 timestamp for the current UTC moment.
///
/// Uses a minimal implementation to avoid pulling in a datetime crate at the
/// calypso-runtime level.  The format is `YYYY-MM-DDTHH:MM:SSZ`.
fn now_rfc3339() -> String {
    use std::time::SystemTime;
    let d = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = d.as_secs();
    // Simple UTC breakdown
    let days = secs / 86400;
    let time_of_day = secs % 86400;
    let hours = time_of_day / 3600;
    let minutes = (time_of_day % 3600) / 60;
    let seconds = time_of_day % 60;

    // Civil date from days since epoch (simplified Gregorian)
    let (year, month, day) = civil_from_days(days as i64);

    format!("{year:04}-{month:02}-{day:02}T{hours:02}:{minutes:02}:{seconds:02}Z")
}

/// Convert days since Unix epoch to (year, month, day).
/// Algorithm from Howard Hinnant's `civil_from_days`.
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

// ── Operator inspection helpers ─────────────────────────────────────────────

/// A summary of a workflow run suitable for operator display.
///
/// Derived entirely from persisted `WorkflowRun` state — no log scraping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunInspection {
    pub run_id: String,
    pub workflow_id: String,
    pub current_state: String,
    pub locality: String,
    pub iteration: usize,
    pub transition_count: usize,
    pub pending_check_count: usize,
    pub failing_check_count: usize,
    pub active_agent_count: usize,
    pub steering_pending_count: usize,
    pub terminal_reason: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

impl WorkflowRun {
    /// Build an operator-facing inspection summary from this run's state.
    ///
    /// This is the primary mechanism for PRD §6.4 (workflow observability) —
    /// operator-facing history and inspection data derived from persisted run
    /// state rather than best-effort log inference.
    pub fn inspect(&self) -> RunInspection {
        RunInspection {
            run_id: self.run_id.as_str().to_string(),
            workflow_id: self.workflow_id.clone(),
            current_state: self.current_state.clone(),
            locality: self.locality.to_string(),
            iteration: self.iteration,
            transition_count: self.transition_history.len(),
            pending_check_count: self
                .pending_checks
                .iter()
                .filter(|c| matches!(c.status, CheckStatus::Pending))
                .count(),
            failing_check_count: self
                .pending_checks
                .iter()
                .filter(|c| matches!(c.status, CheckStatus::Failing))
                .count(),
            active_agent_count: self
                .agent_runs
                .iter()
                .filter(|r| matches!(r.status, AgentRunStatus::Running))
                .count(),
            steering_pending_count: self
                .steering
                .iter()
                .filter(|s| matches!(s.outcome, SteeringOutcome::Pending))
                .count(),
            terminal_reason: self.terminal_reason.as_ref().map(|r| r.to_string()),
            created_at: self.created_at.clone(),
            updated_at: self.updated_at.clone(),
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_run_has_correct_defaults() {
        let run = WorkflowRun::new("scan-loop", "scan", 1);
        assert_eq!(run.run_id.as_str(), "scan-loop-1");
        assert_eq!(run.workflow_id, "scan-loop");
        assert_eq!(run.current_state, "scan");
        assert_eq!(run.locality, ExecutionLocality::Local);
        assert!(run.transition_history.is_empty());
        assert!(run.pending_checks.is_empty());
        assert!(run.steering.is_empty());
        assert!(run.agent_runs.is_empty());
        assert_eq!(run.iteration, 0);
        assert!(!run.is_stopped());
    }

    #[test]
    fn record_transition_updates_state_and_history() {
        let mut run = WorkflowRun::new("wf", "a", 1);
        run.record_transition("b", "on_success");
        assert_eq!(run.current_state, "b");
        assert_eq!(run.iteration, 1);
        assert_eq!(run.transition_history.len(), 1);
        assert_eq!(run.transition_history[0].from_state, "a");
        assert_eq!(run.transition_history[0].to_state, "b");
        assert_eq!(run.transition_history[0].trigger, "on_success");
    }

    #[test]
    fn terminate_marks_run_stopped() {
        let mut run = WorkflowRun::new("wf", "done", 1);
        assert!(!run.is_stopped());
        run.terminate(TerminalReason::Success {
            terminal_state: "done".to_string(),
        });
        assert!(run.is_stopped());
    }

    #[test]
    fn set_check_inserts_and_updates() {
        let mut run = WorkflowRun::new("wf", "a", 1);
        run.set_check("ci.tests", "CI test suite", CheckStatus::Pending);
        assert_eq!(run.pending_checks.len(), 1);
        assert!(matches!(run.pending_checks[0].status, CheckStatus::Pending));

        run.set_check("ci.tests", "CI test suite", CheckStatus::Passing);
        assert_eq!(run.pending_checks.len(), 1);
        assert!(matches!(run.pending_checks[0].status, CheckStatus::Passing));
    }

    #[test]
    fn agent_run_lifecycle() {
        let mut run = WorkflowRun::new("wf", "implement", 1);
        run.start_agent_run("session-1", "implement");
        assert_eq!(run.agent_runs.len(), 1);
        assert!(matches!(run.agent_runs[0].status, AgentRunStatus::Running));

        run.complete_agent_run("session-1", AgentRunStatus::Completed, Some("ok".into()));
        assert!(matches!(
            run.agent_runs[0].status,
            AgentRunStatus::Completed
        ));
        assert_eq!(run.agent_runs[0].outcome.as_deref(), Some("ok"));
    }

    #[test]
    fn steering_lifecycle() {
        let mut run = WorkflowRun::new("wf", "stuck", 1);
        run.add_steering(SteeringAction::Retry);
        assert_eq!(run.steering.len(), 1);
        assert!(matches!(run.steering[0].outcome, SteeringOutcome::Pending));

        run.resolve_steering(SteeringOutcome::Applied);
        assert!(matches!(run.steering[0].outcome, SteeringOutcome::Applied));
    }

    #[test]
    fn inspect_returns_correct_counts() {
        let mut run = WorkflowRun::new("wf", "a", 1);
        run.record_transition("b", "ev");
        run.record_transition("c", "ev");
        run.set_check("c1", "check 1", CheckStatus::Pending);
        run.set_check("c2", "check 2", CheckStatus::Failing);
        run.start_agent_run("s1", "b");
        run.add_steering(SteeringAction::Retry);

        let inspection = run.inspect();
        assert_eq!(inspection.transition_count, 2);
        assert_eq!(inspection.pending_check_count, 1);
        assert_eq!(inspection.failing_check_count, 1);
        assert_eq!(inspection.active_agent_count, 1);
        assert_eq!(inspection.steering_pending_count, 1);
        assert!(inspection.terminal_reason.is_none());
    }

    #[test]
    fn serialization_round_trip() {
        let mut run = WorkflowRun::new("scan-loop", "scan", 42);
        run.record_transition("check", "on_success");
        run.set_check("ci.tests", "Test suite", CheckStatus::Passing);
        run.start_agent_run("agent-1", "scan");
        run.complete_agent_run("agent-1", AgentRunStatus::Completed, None);
        run.add_steering(SteeringAction::Abort);
        run.resolve_steering(SteeringOutcome::Applied);
        run.terminate(TerminalReason::Success {
            terminal_state: "done".to_string(),
        });

        let json = serde_json::to_string_pretty(&run).unwrap();
        let restored: WorkflowRun = serde_json::from_str(&json).unwrap();
        assert_eq!(run, restored);
    }

    #[test]
    fn persistence_save_load_clear() {
        let dir = std::env::temp_dir().join("calypso-wfrun-test");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("workflow-run.json");

        // Fresh load returns None
        let loaded = WorkflowRun::load(&path).unwrap();
        assert!(loaded.is_none());

        // Save and reload
        let run = WorkflowRun::new("test-wf", "init", 1);
        run.save(&path).unwrap();
        let loaded = WorkflowRun::load(&path).unwrap().unwrap();
        assert_eq!(loaded.run_id, run.run_id);
        assert_eq!(loaded.current_state, "init");

        // Clear
        WorkflowRun::clear(&path).unwrap();
        assert!(WorkflowRun::load(&path).unwrap().is_none());

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn multiple_transitions_preserve_full_history() {
        let mut run = WorkflowRun::new("wf", "s0", 1);
        run.record_transition("s1", "ev1");
        run.record_transition("s2", "ev2");
        run.record_transition("s3", "ev3");

        assert_eq!(run.current_state, "s3");
        assert_eq!(run.iteration, 3);
        assert_eq!(run.transition_history.len(), 3);

        // Verify the full chain
        let states: Vec<&str> = run
            .transition_history
            .iter()
            .map(|t| t.to_state.as_str())
            .collect();
        assert_eq!(states, vec!["s1", "s2", "s3"]);
    }

    #[test]
    fn terminal_reasons_display() {
        let cases = vec![
            (
                TerminalReason::Success {
                    terminal_state: "done".into(),
                },
                "success (terminal state: done)",
            ),
            (
                TerminalReason::Aborted {
                    reason: "user request".into(),
                },
                "aborted: user request",
            ),
            (
                TerminalReason::Error {
                    state: "scan".into(),
                    message: "timeout".into(),
                },
                "error at 'scan': timeout",
            ),
            (
                TerminalReason::Interrupted {
                    signal: "SIGINT".into(),
                },
                "interrupted by SIGINT",
            ),
        ];
        for (reason, expected) in cases {
            assert_eq!(reason.to_string(), expected);
        }
    }

    #[test]
    fn execution_locality_display() {
        assert_eq!(ExecutionLocality::Local.to_string(), "local");
        assert_eq!(
            ExecutionLocality::Forge {
                runner: "ubuntu-latest".into()
            }
            .to_string(),
            "forge:ubuntu-latest"
        );
        assert_eq!(
            ExecutionLocality::Delegated {
                target: "remote-1".into()
            }
            .to_string(),
            "delegated:remote-1"
        );
    }
}
