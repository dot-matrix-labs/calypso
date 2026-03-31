//! Daemon-backed API handlers for `calypso-web`.
//!
//! This module provides the server-side logic for the daemon-owned API boundary
//! described in PRD §7.6.  `calypso-web` reads run state, workflow graph
//! metadata, transition history, blocking conditions, and steering actions
//! through the functions here rather than directly accessing repo-local files.
//!
//! # Data source
//!
//! All run state is read from the `WorkflowRun` persisted at
//! `.calypso/workflow-run.json`.  Steering writes are appended to that same
//! file.  Workflow graph metadata is loaded from the `WorkflowCatalog`.
//!
//! # Canonical docs
//!
//! - `docs/prd.md` §7.6
//! - `crates/calypso-runtime/src/workflow_run.rs` — `WorkflowRun` schema

use std::path::Path;

use calypso_runtime::workflow_run::{
    SteeringAction, SteeringOutcome, WorkflowRun, WorkflowRunError,
};
use serde_json::{Value, json};

// ── Run-state endpoint ────────────────────────────────────────────────────────

/// Return the current daemon run state as a JSON object.
///
/// Reads `WorkflowRun` from `.calypso/workflow-run.json`.  If no run is active
/// the returned object has `"active": false` and all other fields are `null`.
///
/// The JSON shape is:
/// ```json
/// {
///   "active": true,
///   "run_id": "scan-loop-1",
///   "workflow_id": "calypso-default-feature-workflow",
///   "current_state": "write-failing-tests",
///   "locality": "local",
///   "iteration": 3,
///   "created_at": "2026-03-31T00:00:00Z",
///   "updated_at": "2026-03-31T00:01:00Z",
///   "is_stopped": false,
///   "terminal_reason": null
/// }
/// ```
pub fn run_state_json(cwd: &Path) -> String {
    let path = WorkflowRun::default_path(cwd);
    match WorkflowRun::load(&path) {
        Ok(Some(run)) => {
            let obj = json!({
                "active": true,
                "run_id": run.run_id.as_str(),
                "workflow_id": run.workflow_id,
                "current_state": run.current_state,
                "locality": run.locality.to_string(),
                "iteration": run.iteration,
                "created_at": run.created_at,
                "updated_at": run.updated_at,
                "is_stopped": run.is_stopped(),
                "terminal_reason": run.terminal_reason.as_ref().map(|r| r.to_string()),
            });
            serde_json::to_string(&obj).unwrap_or_else(|_| "{}".to_string())
        }
        Ok(None) => {
            let obj = json!({
                "active": false,
                "run_id": null,
                "workflow_id": null,
                "current_state": null,
                "locality": null,
                "iteration": null,
                "created_at": null,
                "updated_at": null,
                "is_stopped": false,
                "terminal_reason": null,
            });
            serde_json::to_string(&obj).unwrap_or_else(|_| "{}".to_string())
        }
        Err(_) => {
            let obj = json!({ "active": false, "error": "failed to load run state" });
            serde_json::to_string(&obj).unwrap_or_else(|_| "{}".to_string())
        }
    }
}

// ── Transitions endpoint ──────────────────────────────────────────────────────

/// Return the transition history for the current daemon run as a JSON array.
///
/// Each element has the shape:
/// ```json
/// { "from_state": "a", "to_state": "b", "trigger": "on_success", "timestamp": "..." }
/// ```
///
/// Returns an empty array when no run is active or the run has no history.
pub fn transitions_json(cwd: &Path) -> String {
    let path = WorkflowRun::default_path(cwd);
    let transitions = match WorkflowRun::load(&path) {
        Ok(Some(run)) => run
            .transition_history
            .into_iter()
            .map(|t| {
                json!({
                    "from_state": t.from_state,
                    "to_state": t.to_state,
                    "trigger": t.trigger,
                    "timestamp": t.timestamp,
                })
            })
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    serde_json::to_string(&Value::Array(transitions)).unwrap_or_else(|_| "[]".to_string())
}

// ── Checks / blocking-conditions endpoint ────────────────────────────────────

/// Return the pending deterministic checks for the current daemon run.
///
/// Each element has the shape:
/// ```json
/// {
///   "check_id": "ci.tests",
///   "description": "CI test suite must pass",
///   "status": "pending",
///   "last_evaluated_at": null
/// }
/// ```
///
/// Returns an empty array when no run is active or there are no checks.
pub fn checks_json(cwd: &Path) -> String {
    let path = WorkflowRun::default_path(cwd);
    let checks = match WorkflowRun::load(&path) {
        Ok(Some(run)) => run
            .pending_checks
            .into_iter()
            .map(|c| {
                json!({
                    "check_id": c.check_id,
                    "description": c.description,
                    "status": format!("{:?}", c.status).to_lowercase(),
                    "last_evaluated_at": c.last_evaluated_at,
                })
            })
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    serde_json::to_string(&Value::Array(checks)).unwrap_or_else(|_| "[]".to_string())
}

// ── Steering endpoint ─────────────────────────────────────────────────────────

/// Accepted steering request body shapes.
///
/// The web surface sends one of:
/// - `{ "action": "retry" }`
/// - `{ "action": "abort" }`
/// - `{ "action": "clarify", "message": "..." }`
/// - `{ "action": "skip", "target_state": "..." }`
/// - `{ "action": "force", "target_state": "...", "reason": "..." }`
#[derive(Debug, PartialEq, Eq)]
pub enum SteeringRequest {
    Retry,
    Abort,
    Clarify { message: String },
    Skip { target_state: String },
    Force { target_state: String, reason: String },
}

/// Errors produced when parsing or applying a steering request.
#[derive(Debug, PartialEq, Eq)]
pub enum SteeringError {
    /// The request JSON was malformed or missing required fields.
    BadRequest,
    /// No active run was found; cannot apply steering.
    NoActiveRun,
    /// Persisting the updated run failed.
    PersistFailed,
}

/// Parse a steering request from raw JSON bytes.
pub fn parse_steering(body: &[u8]) -> Result<SteeringRequest, SteeringError> {
    let parsed: Value =
        serde_json::from_slice(body).map_err(|_| SteeringError::BadRequest)?;

    let action = parsed
        .get("action")
        .and_then(|v| v.as_str())
        .ok_or(SteeringError::BadRequest)?;

    match action {
        "retry" => Ok(SteeringRequest::Retry),
        "abort" => Ok(SteeringRequest::Abort),
        "clarify" => {
            let message = parsed
                .get("message")
                .and_then(|v| v.as_str())
                .ok_or(SteeringError::BadRequest)?
                .to_string();
            Ok(SteeringRequest::Clarify { message })
        }
        "skip" => {
            let target_state = parsed
                .get("target_state")
                .and_then(|v| v.as_str())
                .ok_or(SteeringError::BadRequest)?
                .to_string();
            Ok(SteeringRequest::Skip { target_state })
        }
        "force" => {
            let target_state = parsed
                .get("target_state")
                .and_then(|v| v.as_str())
                .ok_or(SteeringError::BadRequest)?
                .to_string();
            let reason = parsed
                .get("reason")
                .and_then(|v| v.as_str())
                .ok_or(SteeringError::BadRequest)?
                .to_string();
            Ok(SteeringRequest::Force {
                target_state,
                reason,
            })
        }
        _ => Err(SteeringError::BadRequest),
    }
}

/// Apply a steering request to the active daemon run.
///
/// Loads the current `WorkflowRun` from disk, appends the steering entry, and
/// persists the updated run.  This is a best-effort interface — the daemon is
/// expected to notice the new steering entry on its next tick.
pub fn apply_steering(cwd: &Path, request: SteeringRequest) -> Result<(), SteeringError> {
    let path = WorkflowRun::default_path(cwd);
    let mut run = match WorkflowRun::load(&path) {
        Ok(Some(r)) => r,
        Ok(None) => return Err(SteeringError::NoActiveRun),
        Err(_) => return Err(SteeringError::PersistFailed),
    };

    let action = match request {
        SteeringRequest::Retry => SteeringAction::Retry,
        SteeringRequest::Abort => SteeringAction::Abort,
        SteeringRequest::Clarify { message } => SteeringAction::Clarify { message },
        SteeringRequest::Skip { target_state } => SteeringAction::Skip { target_state },
        SteeringRequest::Force {
            target_state,
            reason,
        } => SteeringAction::ForceTransition {
            target_state,
            reason,
        },
    };

    run.add_steering(action);
    // Mark the newest entry as pending (already the default from add_steering).

    run.save(&path).map_err(|e| match e {
        WorkflowRunError::Io(_) | WorkflowRunError::Json(_) => SteeringError::PersistFailed,
    })
}

/// Return the full steering log for the active run as a JSON array.
///
/// Each element has the shape:
/// ```json
/// {
///   "action": "retry",
///   "requested_at": "...",
///   "outcome": "pending"
/// }
/// ```
pub fn steering_log_json(cwd: &Path) -> String {
    let path = WorkflowRun::default_path(cwd);
    let entries = match WorkflowRun::load(&path) {
        Ok(Some(run)) => run
            .steering
            .into_iter()
            .map(|s| {
                let action_label = match &s.action {
                    SteeringAction::Retry => "retry".to_string(),
                    SteeringAction::Abort => "abort".to_string(),
                    SteeringAction::Clarify { message } => format!("clarify: {message}"),
                    SteeringAction::Skip { target_state } => format!("skip to {target_state}"),
                    SteeringAction::ForceTransition {
                        target_state,
                        reason,
                    } => format!("force → {target_state} ({reason})"),
                };
                let outcome_label = match &s.outcome {
                    SteeringOutcome::Pending => "pending",
                    SteeringOutcome::Applied => "applied",
                    SteeringOutcome::Rejected { .. } => "rejected",
                };
                json!({
                    "action": action_label,
                    "requested_at": s.requested_at,
                    "outcome": outcome_label,
                })
            })
            .collect::<Vec<_>>(),
        _ => vec![],
    };
    serde_json::to_string(&Value::Array(entries)).unwrap_or_else(|_| "[]".to_string())
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use calypso_runtime::workflow_run::{CheckStatus, WorkflowRun};

    fn tmp_dir(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(name);
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join(".calypso")).unwrap();
        dir
    }

    fn save_run(dir: &std::path::Path, run: &WorkflowRun) {
        let path = WorkflowRun::default_path(dir);
        run.save(&path).unwrap();
    }

    // ── run_state_json ────────────────────────────────────────────────────────

    #[test]
    fn run_state_json_no_run_returns_inactive() {
        let dir = tmp_dir("web-daemon-state-no-run");
        let json = run_state_json(&dir);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["active"], false);
        assert!(parsed["run_id"].is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_state_json_active_run_returns_correct_fields() {
        let dir = tmp_dir("web-daemon-state-active");
        let run = WorkflowRun::new("test-wf", "scan", 1);
        save_run(&dir, &run);

        let json = run_state_json(&dir);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["active"], true);
        assert_eq!(parsed["workflow_id"], "test-wf");
        assert_eq!(parsed["current_state"], "scan");
        assert_eq!(parsed["is_stopped"], false);
        assert!(parsed["terminal_reason"].is_null());
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── transitions_json ──────────────────────────────────────────────────────

    #[test]
    fn transitions_json_no_run_returns_empty_array() {
        let dir = tmp_dir("web-daemon-transitions-no-run");
        let json = transitions_json(&dir);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn transitions_json_returns_history_entries() {
        let dir = tmp_dir("web-daemon-transitions-active");
        let mut run = WorkflowRun::new("test-wf", "scan", 1);
        run.record_transition("check", "on_success");
        run.record_transition("done", "on_pass");
        save_run(&dir, &run);

        let json = transitions_json(&dir);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["from_state"], "scan");
        assert_eq!(arr[0]["to_state"], "check");
        assert_eq!(arr[1]["trigger"], "on_pass");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── checks_json ───────────────────────────────────────────────────────────

    #[test]
    fn checks_json_no_run_returns_empty_array() {
        let dir = tmp_dir("web-daemon-checks-no-run");
        let json = checks_json(&dir);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn checks_json_returns_pending_checks() {
        let dir = tmp_dir("web-daemon-checks-active");
        let mut run = WorkflowRun::new("test-wf", "check", 1);
        run.set_check("ci.tests", "CI test suite", CheckStatus::Failing);
        run.set_check("branch.up-to-date", "Branch must be rebased", CheckStatus::Passing);
        save_run(&dir, &run);

        let json = checks_json(&dir);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 2);
        assert_eq!(arr[0]["check_id"], "ci.tests");
        assert_eq!(arr[0]["status"], "failing");
        assert_eq!(arr[1]["check_id"], "branch.up-to-date");
        assert_eq!(arr[1]["status"], "passing");
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── parse_steering ────────────────────────────────────────────────────────

    #[test]
    fn parse_steering_retry() {
        let body = br#"{"action":"retry"}"#;
        assert_eq!(parse_steering(body).unwrap(), SteeringRequest::Retry);
    }

    #[test]
    fn parse_steering_abort() {
        let body = br#"{"action":"abort"}"#;
        assert_eq!(parse_steering(body).unwrap(), SteeringRequest::Abort);
    }

    #[test]
    fn parse_steering_clarify() {
        let body = br#"{"action":"clarify","message":"please clarify the scope"}"#;
        let req = parse_steering(body).unwrap();
        assert_eq!(
            req,
            SteeringRequest::Clarify {
                message: "please clarify the scope".to_string()
            }
        );
    }

    #[test]
    fn parse_steering_skip() {
        let body = br#"{"action":"skip","target_state":"verify"}"#;
        let req = parse_steering(body).unwrap();
        assert_eq!(
            req,
            SteeringRequest::Skip {
                target_state: "verify".to_string()
            }
        );
    }

    #[test]
    fn parse_steering_force() {
        let body = br#"{"action":"force","target_state":"deploy","reason":"manual override"}"#;
        let req = parse_steering(body).unwrap();
        assert_eq!(
            req,
            SteeringRequest::Force {
                target_state: "deploy".to_string(),
                reason: "manual override".to_string(),
            }
        );
    }

    #[test]
    fn parse_steering_unknown_action_returns_bad_request() {
        let body = br#"{"action":"teleport"}"#;
        assert_eq!(parse_steering(body).unwrap_err(), SteeringError::BadRequest);
    }

    #[test]
    fn parse_steering_invalid_json_returns_bad_request() {
        let body = b"not json";
        assert_eq!(parse_steering(body).unwrap_err(), SteeringError::BadRequest);
    }

    #[test]
    fn parse_steering_missing_action_returns_bad_request() {
        let body = br#"{"foo":"bar"}"#;
        assert_eq!(parse_steering(body).unwrap_err(), SteeringError::BadRequest);
    }

    // ── apply_steering ────────────────────────────────────────────────────────

    #[test]
    fn apply_steering_no_active_run_returns_error() {
        let dir = tmp_dir("web-daemon-steer-no-run");
        let result = apply_steering(&dir, SteeringRequest::Retry);
        assert_eq!(result.unwrap_err(), SteeringError::NoActiveRun);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn apply_steering_appends_entry_to_run() {
        let dir = tmp_dir("web-daemon-steer-active");
        let run = WorkflowRun::new("test-wf", "stuck", 1);
        save_run(&dir, &run);

        apply_steering(&dir, SteeringRequest::Retry).unwrap();

        let path = WorkflowRun::default_path(&dir);
        let loaded = WorkflowRun::load(&path).unwrap().unwrap();
        assert_eq!(loaded.steering.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ── steering_log_json ─────────────────────────────────────────────────────

    #[test]
    fn steering_log_json_no_run_returns_empty_array() {
        let dir = tmp_dir("web-daemon-steer-log-no-run");
        let json = steering_log_json(&dir);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        assert!(parsed.as_array().unwrap().is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn steering_log_json_returns_entries() {
        let dir = tmp_dir("web-daemon-steer-log-active");
        let run = WorkflowRun::new("test-wf", "stuck", 1);
        save_run(&dir, &run);
        apply_steering(&dir, SteeringRequest::Abort).unwrap();

        let json = steering_log_json(&dir);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        let arr = parsed.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0]["action"], "abort");
        assert_eq!(arr[0]["outcome"], "pending");
        let _ = std::fs::remove_dir_all(&dir);
    }
}
