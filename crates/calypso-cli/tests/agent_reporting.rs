//! Tests for agent status reporting — session builder helpers, text rendering,
//! and state file round-trips.
//!
//! These tests exercise the `OperatorSurface` rendering functions to assert
//! that session status icons, output lines, and pending clarifications appear
//! correctly in the rendered output.

use std::time::{SystemTime, UNIX_EPOCH};

use calypso_runtime::operator_surface::OperatorSurface;
use calypso_runtime::state::{
    AgentSession, AgentSessionStatus, AgentTerminalOutcome, ClarificationEntry, FeatureState,
    FeatureType, Gate, GateGroup, GateStatus, PullRequestRef, RepositoryState, SchedulingMeta,
    SessionOutput, SessionOutputStream, WorkflowState,
};

// ── Session builder helpers ───────────────────────────────────────────────────

fn running_session(role: &str, output_lines: &[&str]) -> AgentSession {
    AgentSession {
        role: role.to_string(),
        session_id: format!("{role}-session"),
        provider_session_id: None,
        status: AgentSessionStatus::Running,
        output: output_lines
            .iter()
            .map(|line| SessionOutput {
                stream: SessionOutputStream::Stdout,
                text: line.to_string(),
            })
            .collect(),
        pending_follow_ups: vec![],
        terminal_outcome: None,
    }
}

fn completed_session(role: &str) -> AgentSession {
    AgentSession {
        role: role.to_string(),
        session_id: format!("{role}-session"),
        provider_session_id: None,
        status: AgentSessionStatus::Completed,
        output: vec![],
        pending_follow_ups: vec![],
        terminal_outcome: Some(AgentTerminalOutcome::Ok),
    }
}

fn failed_session(role: &str) -> AgentSession {
    AgentSession {
        role: role.to_string(),
        session_id: format!("{role}-session"),
        provider_session_id: None,
        status: AgentSessionStatus::Failed,
        output: vec![],
        pending_follow_ups: vec![],
        terminal_outcome: Some(AgentTerminalOutcome::Nok),
    }
}

fn aborted_session(role: &str) -> AgentSession {
    AgentSession {
        role: role.to_string(),
        session_id: format!("{role}-session"),
        provider_session_id: None,
        status: AgentSessionStatus::Aborted,
        output: vec![],
        pending_follow_ups: vec![],
        terminal_outcome: Some(AgentTerminalOutcome::Aborted),
    }
}

fn waiting_session(role: &str, question: &str) -> AgentSession {
    AgentSession {
        role: role.to_string(),
        session_id: format!("{role}-session"),
        provider_session_id: None,
        status: AgentSessionStatus::WaitingForHuman,
        output: vec![],
        pending_follow_ups: vec![question.to_string()],
        terminal_outcome: None,
    }
}

// ── Feature builder ───────────────────────────────────────────────────────────

fn feature_with_sessions(sessions: Vec<AgentSession>) -> FeatureState {
    FeatureState {
        feature_id: "feat-reporting".to_string(),
        branch: "feat/reporting".to_string(),
        worktree_path: "/tmp/reporting".to_string(),
        pull_request: PullRequestRef {
            number: 42,
            url: "https://github.com/example/repo/pull/42".to_string(),
        },
        github_snapshot: None,
        github_error: None,
        workflow_state: WorkflowState::Implementation,
        gate_groups: vec![GateGroup {
            id: "ci".to_string(),
            label: "CI".to_string(),
            gates: vec![Gate {
                id: "tests".to_string(),
                label: "Tests green".to_string(),
                task: "tests".to_string(),
                status: GateStatus::Pending,
            }],
        }],
        active_sessions: sessions,
        feature_type: FeatureType::Feat,
        roles: vec![],
        scheduling: SchedulingMeta::default(),
        artifact_refs: vec![],
        transcript_refs: vec![],
        clarification_history: vec![],
    }
}

fn feature_empty() -> FeatureState {
    let mut f = feature_with_sessions(vec![]);
    f.gate_groups.clear();
    f
}

/// Create a unique temp directory.
fn temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("calypso-reporting-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

// ── Unit tests — text rendering ───────────────────────────────────────────────

#[test]
fn running_session_shows_running_status_in_render() {
    let feature = feature_with_sessions(vec![running_session("engineer", &[])]);
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(
        rendered.contains("[running]"),
        "running session should show [running] status"
    );
    assert!(
        rendered.contains("engineer"),
        "running session should show role name"
    );
}

#[test]
fn completed_session_shows_completed_status_in_render() {
    let feature = feature_with_sessions(vec![completed_session("reviewer")]);
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(
        rendered.contains("[completed]"),
        "completed session should show [completed] status"
    );
    assert!(rendered.contains("reviewer"));
}

#[test]
fn failed_session_shows_failed_status_in_render() {
    let feature = feature_with_sessions(vec![failed_session("validator")]);
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(
        rendered.contains("[failed]"),
        "failed session should show [failed] status"
    );
    assert!(rendered.contains("validator"));
}

#[test]
fn aborted_session_shows_aborted_status_in_render() {
    let feature = feature_with_sessions(vec![aborted_session("orchestrator")]);
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(
        rendered.contains("[aborted]"),
        "aborted session should show [aborted] status"
    );
    assert!(rendered.contains("orchestrator"));
}

#[test]
fn session_output_lines_appear_in_render() {
    let session = running_session(
        "engineer",
        &[
            "Cloning repository",
            "Running cargo build",
            "Build succeeded",
        ],
    );
    let feature = feature_with_sessions(vec![session]);
    let rendered = OperatorSurface::from_feature_state(&feature).render();

    assert!(
        rendered.contains("Cloning repository"),
        "first output line should appear in render"
    );
    assert!(
        rendered.contains("Running cargo build"),
        "second output line should appear in render"
    );
    assert!(
        rendered.contains("Build succeeded"),
        "third output line should appear in render"
    );
}

#[test]
fn pending_follow_ups_appear_in_render() {
    let session = waiting_session("architect", "What branch should I target?");
    let feature = feature_with_sessions(vec![session]);
    let rendered = OperatorSurface::from_feature_state(&feature).render();

    // The waiting session has a pending follow-up question in pending_follow_ups
    assert!(
        rendered.contains("architect"),
        "waiting session role should appear"
    );
    assert!(
        rendered.contains("[waiting-for-human]"),
        "waiting session should show waiting status"
    );
}

#[test]
fn feature_with_no_sessions_shows_no_active_sessions() {
    let feature = feature_empty();
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(
        rendered.contains("No active sessions"),
        "feature with no sessions should render 'No active sessions'"
    );
}

#[test]
fn pending_clarification_appears_in_render() {
    let mut feature = feature_with_sessions(vec![running_session("engineer", &[])]);
    feature.clarification_history = vec![ClarificationEntry {
        session_id: "engineer-session".to_string(),
        question: "Which module should I modify?".to_string(),
        answer: None,
        timestamp: "2026-03-15T10:00:00Z".to_string(),
    }];

    let rendered = OperatorSurface::from_feature_state(&feature).render();

    assert!(
        rendered.contains("Which module should I modify?"),
        "unanswered clarification question should appear in render"
    );
    assert!(
        rendered.contains("Pending Clarifications"),
        "pending clarifications section should appear"
    );
}

// ── Integration tests — state file round-trip ─────────────────────────────────

fn repo_state_with_sessions(sessions: Vec<AgentSession>) -> RepositoryState {
    RepositoryState {
        version: 1,
        repo_id: "reporting-repo".to_string(),
        schema_version: 2,
        current_feature: feature_with_sessions(sessions),
        identity: Default::default(),
        providers: vec![],
        releases: vec![],
        deployments: vec![],
        github_auth_ref: None,
        secure_key_refs: vec![],
    }
}

#[test]
fn state_file_round_trip_three_sessions_all_roles_visible() {
    let dir = temp_dir("round-trip-three");
    let calypso_dir = dir.join(".calypso");
    std::fs::create_dir_all(&calypso_dir).expect("create .calypso dir");
    let state_path = calypso_dir.join("repository-state.json");

    // Build a RepositoryState with three sessions.
    let sessions = vec![
        running_session("engineer", &["Compiling crate"]),
        completed_session("reviewer"),
        failed_session("validator"),
    ];
    let original = repo_state_with_sessions(sessions);

    // Serialize to disk.
    original
        .save_to_path(&state_path)
        .expect("save state to disk");

    // Load back.
    let loaded = RepositoryState::load_from_path(&state_path).expect("load state from disk");

    // Render via OperatorSurface.
    let rendered = OperatorSurface::from_feature_state(&loaded.current_feature).render();

    assert!(
        rendered.contains("engineer"),
        "loaded state should render engineer session"
    );
    assert!(
        rendered.contains("reviewer"),
        "loaded state should render reviewer session"
    );
    assert!(
        rendered.contains("validator"),
        "loaded state should render validator session"
    );
    assert!(
        rendered.contains("[running]"),
        "running session status should survive round-trip"
    );
    assert!(
        rendered.contains("[completed]"),
        "completed session status should survive round-trip"
    );
    assert!(
        rendered.contains("[failed]"),
        "failed session status should survive round-trip"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

// ── Edge cases ────────────────────────────────────────────────────────────────

#[test]
fn feature_with_zero_sessions_renders_no_active_sessions() {
    let feature = feature_empty();
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(
        rendered.contains("No active sessions"),
        "zero sessions should show 'No active sessions'"
    );
}

#[test]
fn session_with_ten_output_lines_all_appear_in_render() {
    let lines: Vec<&str> = vec![
        "Line 1: init",
        "Line 2: fetch",
        "Line 3: compile",
        "Line 4: link",
        "Line 5: test",
        "Line 6: coverage",
        "Line 7: lint",
        "Line 8: format",
        "Line 9: upload",
        "Line 10: done",
    ];
    let session = running_session("engineer", &lines);
    let feature = feature_with_sessions(vec![session]);
    let rendered = OperatorSurface::from_feature_state(&feature).render();

    for line in &lines {
        assert!(
            rendered.contains(line),
            "output line '{line}' should appear in render"
        );
    }
}

#[test]
fn session_with_pending_clarification_unanswered_question_appears() {
    let mut feature = feature_with_sessions(vec![running_session("orchestrator", &[])]);
    feature.clarification_history = vec![ClarificationEntry {
        session_id: "orchestrator-session".to_string(),
        question: "Should I use async or sync IO?".to_string(),
        answer: None,
        timestamp: "2026-03-15T10:00:00Z".to_string(),
    }];

    let rendered = OperatorSurface::from_feature_state(&feature).render();

    assert!(
        rendered.contains("Should I use async or sync IO?"),
        "pending clarification question should appear in render"
    );
}

#[test]
fn answered_clarification_does_not_appear_in_pending_section() {
    let mut feature = feature_with_sessions(vec![running_session("orchestrator", &[])]);
    feature.clarification_history = vec![
        ClarificationEntry {
            session_id: "orchestrator-session".to_string(),
            question: "Pending question".to_string(),
            answer: None,
            timestamp: "2026-03-15T10:00:00Z".to_string(),
        },
        ClarificationEntry {
            session_id: "orchestrator-session".to_string(),
            question: "Already answered question".to_string(),
            answer: Some("Use async IO".to_string()),
            timestamp: "2026-03-15T10:01:00Z".to_string(),
        },
    ];

    let rendered = OperatorSurface::from_feature_state(&feature).render();

    assert!(
        rendered.contains("Pending question"),
        "unanswered question should appear in render"
    );
    assert!(
        !rendered.contains("Already answered question"),
        "answered question should NOT appear in pending section"
    );
}

#[test]
fn state_file_round_trip_preserves_session_output_lines() {
    let dir = temp_dir("round-trip-output");
    let calypso_dir = dir.join(".calypso");
    std::fs::create_dir_all(&calypso_dir).expect("create .calypso dir");
    let state_path = calypso_dir.join("repository-state.json");

    let session = running_session("engineer", &["Build output line A", "Build output line B"]);
    let original = repo_state_with_sessions(vec![session]);

    original
        .save_to_path(&state_path)
        .expect("save state to disk");

    let loaded = RepositoryState::load_from_path(&state_path).expect("load state from disk");
    let rendered = OperatorSurface::from_feature_state(&loaded.current_feature).render();

    assert!(
        rendered.contains("Build output line A"),
        "output line A should survive round-trip"
    );
    assert!(
        rendered.contains("Build output line B"),
        "output line B should survive round-trip"
    );

    let _ = std::fs::remove_dir_all(&dir);
}
