//! Integration tests for the JSON report shapes produced by the three new CLI features:
//!   1. `calypso doctor --json`  (Feature 1)
//!   2. `calypso state status --json`  (Feature 2)
//!   3. `calypso agents --json`  (Feature 3)

use calypso_cli::app::{agents_json_report, doctor_json_report, state_status_json_report};
use calypso_cli::doctor::{
    DoctorCheck, DoctorCheckId, DoctorCheckScope, DoctorReport, DoctorStatus,
};
use calypso_cli::state::{
    AgentSession, AgentSessionStatus, FeatureState, Gate, GateGroup, GateStatus, PullRequestRef,
    SessionOutput, SessionOutputStream, WorkflowState,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn passing_check(id: DoctorCheckId) -> DoctorCheck {
    DoctorCheck {
        id,
        scope: DoctorCheckScope::LocalConfiguration,
        status: DoctorStatus::Passing,
        detail: None,
        remediation: None,
        fix: None,
    }
}

fn failing_check(
    id: DoctorCheckId,
    detail: Option<String>,
    remediation: Option<String>,
) -> DoctorCheck {
    DoctorCheck {
        id,
        scope: DoctorCheckScope::LocalConfiguration,
        status: DoctorStatus::Failing,
        detail,
        remediation,
        fix: None,
    }
}

fn minimal_feature_state() -> FeatureState {
    FeatureState {
        feature_id: "feat-login-oauth".to_string(),
        branch: "feat/login-oauth".to_string(),
        worktree_path: "/tmp/worktree".to_string(),
        pull_request: PullRequestRef {
            number: 42,
            url: "https://github.com/example/repo/pull/42".to_string(),
        },
        github_snapshot: None,
        github_error: None,
        workflow_state: WorkflowState::Implementation,
        gate_groups: vec![
            GateGroup {
                id: "spec".to_string(),
                label: "Specification".to_string(),
                gates: vec![Gate {
                    id: "spec.canonicalized".to_string(),
                    label: "PR canonicalized".to_string(),
                    task: "builtin.github.pr_canonicalized".to_string(),
                    status: GateStatus::Passing,
                }],
            },
            GateGroup {
                id: "validation".to_string(),
                label: "Validation".to_string(),
                gates: vec![Gate {
                    id: "validation.rust_quality".to_string(),
                    label: "Rust quality green".to_string(),
                    task: "builtin.doctor.rust_quality".to_string(),
                    status: GateStatus::Failing,
                }],
            },
        ],
        active_sessions: vec![AgentSession {
            role: "engineer".to_string(),
            session_id: "session_01".to_string(),
            provider_session_id: None,
            status: AgentSessionStatus::Running,
            output: vec![SessionOutput {
                stream: SessionOutputStream::Stdout,
                text: "Inspecting branch state…".to_string(),
            }],
            pending_follow_ups: vec![],
            terminal_outcome: None,
        }],
        feature_type: calypso_cli::state::FeatureType::Feat,
        roles: vec![],
        scheduling: calypso_cli::state::SchedulingMeta::default(),
        artifact_refs: vec![],
        transcript_refs: vec![],
        clarification_history: vec![],
    }
}

// ---------------------------------------------------------------------------
// Feature 1 — doctor --json
// ---------------------------------------------------------------------------

#[test]
fn doctor_json_all_passing_has_correct_summary() {
    let report = DoctorReport {
        checks: vec![
            passing_check(DoctorCheckId::GitInitialized),
            passing_check(DoctorCheckId::GhInstalled),
            passing_check(DoctorCheckId::ClaudeInstalled),
        ],
    };

    let json_report = doctor_json_report(&report);

    assert_eq!(json_report.summary.total, 3);
    assert_eq!(json_report.summary.passing, 3);
    assert_eq!(json_report.summary.failing, 0);
}

#[test]
fn doctor_json_failing_check_counted_in_summary() {
    let report = DoctorReport {
        checks: vec![
            passing_check(DoctorCheckId::GitInitialized),
            failing_check(
                DoctorCheckId::GhInstalled,
                Some("gh not found".to_string()),
                Some("Install gh CLI".to_string()),
            ),
        ],
    };

    let json_report = doctor_json_report(&report);

    assert_eq!(json_report.summary.total, 2);
    assert_eq!(json_report.summary.passing, 1);
    assert_eq!(json_report.summary.failing, 1);
}

#[test]
fn doctor_json_check_fields_match_source() {
    let report = DoctorReport {
        checks: vec![failing_check(
            DoctorCheckId::GhInstalled,
            Some("not on PATH".to_string()),
            Some("Install from https://cli.github.com".to_string()),
        )],
    };

    let json_report = doctor_json_report(&report);

    assert_eq!(json_report.checks.len(), 1);
    let check = &json_report.checks[0];
    assert_eq!(check.id, "gh-installed");
    assert_eq!(check.status, "failing");
    assert_eq!(check.detail.as_deref(), Some("not on PATH"));
    assert_eq!(
        check.remediation.as_deref(),
        Some("Install from https://cli.github.com")
    );
    assert!(
        !check.has_auto_fix,
        "manual remediation should not be auto-fix"
    );
}

#[test]
fn doctor_json_serializes_to_valid_json() {
    let report = DoctorReport {
        checks: vec![passing_check(DoctorCheckId::GitInitialized)],
    };

    let json_report = doctor_json_report(&report);
    let json = serde_json::to_string_pretty(&json_report).expect("must serialize");

    // Round-trip: verify expected top-level keys are present.
    let value: serde_json::Value = serde_json::from_str(&json).expect("must be valid JSON");
    assert!(value.get("checks").is_some(), "must have 'checks' key");
    assert!(value.get("summary").is_some(), "must have 'summary' key");
    let summary = &value["summary"];
    assert!(summary.get("total").is_some());
    assert!(summary.get("passing").is_some());
    assert!(summary.get("failing").is_some());
}

#[test]
fn doctor_json_passing_check_has_null_detail_and_remediation() {
    let report = DoctorReport {
        checks: vec![passing_check(DoctorCheckId::GitInitialized)],
    };

    let json_report = doctor_json_report(&report);
    let check = &json_report.checks[0];

    assert!(check.detail.is_none());
    assert!(check.remediation.is_none());
    assert_eq!(check.status, "passing");
}

#[test]
fn doctor_json_warning_check_counted_separately_from_failures() {
    let report = DoctorReport {
        checks: vec![
            passing_check(DoctorCheckId::GitInitialized),
            failing_check(
                DoctorCheckId::GhInstalled,
                Some("gh not found".to_string()),
                Some("Install gh CLI".to_string()),
            ),
            DoctorCheck {
                id: DoctorCheckId::CodexInstalled,
                scope: DoctorCheckScope::LocalConfiguration,
                status: DoctorStatus::Warning,
                detail: None,
                remediation: Some("Install codex CLI".to_string()),
                fix: None,
            },
        ],
    };

    let json_report = doctor_json_report(&report);

    assert_eq!(json_report.summary.total, 3);
    assert_eq!(json_report.summary.passing, 1);
    assert_eq!(json_report.summary.warnings, 1);
    assert_eq!(json_report.summary.failing, 1);
    assert_eq!(json_report.checks[2].status, "warning");
}

// ---------------------------------------------------------------------------
// Feature 2 — state status --json
// ---------------------------------------------------------------------------

#[test]
fn state_status_json_basic_fields() {
    let feature = minimal_feature_state();
    let report = state_status_json_report(&feature);

    assert_eq!(report.feature_id, "feat-login-oauth");
    assert_eq!(report.branch, "feat/login-oauth");
    assert_eq!(report.pr_number, Some(42));
    assert_eq!(report.workflow_state, "implementation");
    assert_eq!(report.active_session_count, 1);
}

#[test]
fn state_status_json_blocking_gate_ids_populated() {
    let feature = minimal_feature_state();
    let report = state_status_json_report(&feature);

    assert!(
        report
            .blocking_gate_ids
            .contains(&"validation.rust_quality".to_string()),
        "failing gate should appear in blocking_gate_ids"
    );
}

#[test]
fn state_status_json_gate_groups_shape() {
    let feature = minimal_feature_state();
    let report = state_status_json_report(&feature);

    assert_eq!(report.gate_groups.len(), 2);

    let spec_group = &report.gate_groups[0];
    assert_eq!(spec_group.id, "spec");
    assert_eq!(spec_group.status, "passing");
    assert_eq!(spec_group.gates.len(), 1);
    assert_eq!(spec_group.gates[0].status, "passing");

    let validation_group = &report.gate_groups[1];
    assert_eq!(validation_group.id, "validation");
    assert_eq!(validation_group.status, "failing");
    assert_eq!(validation_group.gates[0].status, "failing");
}

#[test]
fn state_status_json_no_pr_yields_null_pr_number() {
    let mut feature = minimal_feature_state();
    feature.pull_request = PullRequestRef {
        number: 0,
        url: String::new(),
    };

    let report = state_status_json_report(&feature);
    assert!(report.pr_number.is_none());
}

#[test]
fn state_status_json_serializes_to_valid_json() {
    let feature = minimal_feature_state();
    let report = state_status_json_report(&feature);
    let json = serde_json::to_string_pretty(&report).expect("must serialize");

    let value: serde_json::Value = serde_json::from_str(&json).expect("must be valid JSON");
    assert!(value.get("feature_id").is_some());
    assert!(value.get("branch").is_some());
    assert!(value.get("workflow_state").is_some());
    assert!(value.get("gate_groups").is_some());
    assert!(value.get("blocking_gate_ids").is_some());
    assert!(value.get("active_session_count").is_some());
}

#[test]
fn state_status_json_missing_state_file_returns_error() {
    let tmp = std::env::temp_dir().join("calypso-test-no-state-file");
    // Ensure the directory does NOT have a state file.
    let _ = std::fs::remove_dir_all(&tmp);

    let result = calypso_cli::app::run_state_status_json(&tmp);
    assert!(
        result.is_err(),
        "missing state file should produce an error"
    );
    let msg = result.unwrap_err();
    assert!(
        msg.contains("state I/O error") || msg.contains("os error") || !msg.is_empty(),
        "error message should be non-empty: {msg}"
    );
}

// ---------------------------------------------------------------------------
// Feature 3 — agents --json
// ---------------------------------------------------------------------------

#[test]
fn agents_json_basic_fields() {
    let feature = minimal_feature_state();
    let report = agents_json_report(&feature);

    assert_eq!(report.feature_id, "feat-login-oauth");
    assert_eq!(report.sessions.len(), 1);

    let session = &report.sessions[0];
    assert_eq!(session.session_id, "session_01");
    assert_eq!(session.role, "engineer");
    assert_eq!(session.status, "running");
    assert_eq!(session.output, vec!["Inspecting branch state…"]);
    assert!(session.pending_follow_ups.is_empty());
}

#[test]
fn agents_json_empty_sessions() {
    let mut feature = minimal_feature_state();
    feature.active_sessions = vec![];

    let report = agents_json_report(&feature);
    assert!(report.sessions.is_empty());
}

#[test]
fn agents_json_serializes_to_valid_json() {
    let feature = minimal_feature_state();
    let report = agents_json_report(&feature);
    let json = serde_json::to_string_pretty(&report).expect("must serialize");

    let value: serde_json::Value = serde_json::from_str(&json).expect("must be valid JSON");
    assert!(value.get("feature_id").is_some());
    assert!(value.get("sessions").is_some());

    let sessions = value["sessions"]
        .as_array()
        .expect("sessions must be array");
    assert_eq!(sessions.len(), 1);
    let s = &sessions[0];
    assert!(s.get("session_id").is_some());
    assert!(s.get("role").is_some());
    assert!(s.get("status").is_some());
    assert!(s.get("output").is_some());
    assert!(s.get("pending_follow_ups").is_some());
}

#[test]
fn agents_json_missing_state_file_returns_error() {
    let tmp = std::env::temp_dir().join("calypso-test-no-agents-state");
    let _ = std::fs::remove_dir_all(&tmp);

    let result = calypso_cli::app::run_agents_json(&tmp);
    assert!(
        result.is_err(),
        "missing state file should produce an error"
    );
}

#[test]
fn agents_json_completed_session_status_string() {
    let mut feature = minimal_feature_state();
    feature.active_sessions[0].status = AgentSessionStatus::Completed;

    let report = agents_json_report(&feature);
    assert_eq!(report.sessions[0].status, "completed");
}

#[test]
fn agents_json_multiple_sessions_all_present() {
    let mut feature = minimal_feature_state();
    feature.active_sessions.push(AgentSession {
        role: "reviewer".to_string(),
        session_id: "session_02".to_string(),
        provider_session_id: None,
        status: AgentSessionStatus::Completed,
        output: vec![],
        pending_follow_ups: vec![],
        terminal_outcome: None,
    });

    let report = agents_json_report(&feature);
    assert_eq!(report.sessions.len(), 2);
    assert_eq!(report.sessions[1].role, "reviewer");
    assert_eq!(report.sessions[1].status, "completed");
}
