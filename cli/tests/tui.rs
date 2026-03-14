use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use calypso_cli::state::{
    AgentSession, AgentSessionStatus, ClarificationEntry, EvidenceStatus, FeatureState,
    FeatureType, Gate, GateGroup, GateStatus, GithubMergeability, GithubPullRequestSnapshot,
    GithubReviewStatus, PullRequestRef, SchedulingMeta, SessionOutput, SessionOutputStream,
    WorkflowState,
};
use calypso_cli::tui::{
    InputBuffer, OperatorSurface, SurfaceEvent, answer_clarification, interrupt_active_sessions,
    queue_follow_up,
};

fn sample_feature() -> FeatureState {
    FeatureState {
        feature_id: "feat-tui-surface".to_string(),
        branch: "feat/cli-tui-operator-surface".to_string(),
        worktree_path: "/worktrees/feat-cli-tui-operator-surface".to_string(),
        pull_request: PullRequestRef {
            number: 22,
            url: "https://github.com/org/repo/pull/22".to_string(),
        },
        github_snapshot: None,
        github_error: None,
        workflow_state: WorkflowState::Implementation,
        gate_groups: vec![
            GateGroup {
                id: "specification".to_string(),
                label: "Specification".to_string(),
                gates: vec![Gate {
                    id: "pr-canonicalized".to_string(),
                    label: "PR canonicalized".to_string(),
                    task: "pr-editor".to_string(),
                    status: GateStatus::Passing,
                }],
            },
            GateGroup {
                id: "validation".to_string(),
                label: "Validation".to_string(),
                gates: vec![Gate {
                    id: "rust-quality-green".to_string(),
                    label: "Rust quality green".to_string(),
                    task: "rust-quality".to_string(),
                    status: GateStatus::Failing,
                }],
            },
        ],
        active_sessions: vec![AgentSession {
            role: "engineer".to_string(),
            session_id: "session_01".to_string(),
            provider_session_id: Some("codex_01".to_string()),
            status: AgentSessionStatus::Running,
            output: vec![
                SessionOutput {
                    stream: SessionOutputStream::Stdout,
                    text: "Inspecting branch state".to_string(),
                },
                SessionOutput {
                    stream: SessionOutputStream::Stderr,
                    text: "Waiting on operator guidance".to_string(),
                },
            ],
            pending_follow_ups: Vec::new(),
            terminal_outcome: None,
        }],
        feature_type: FeatureType::Feat,
        roles: Vec::new(),
        scheduling: SchedulingMeta::default(),
        artifact_refs: Vec::new(),
        transcript_refs: Vec::new(),
        clarification_history: Vec::new(),
    }
}

#[test]
fn operator_surface_render_includes_feature_context_gates_and_sessions() {
    let feature = sample_feature();
    let surface = OperatorSurface::from_feature_state(&feature);

    let rendered = surface.render();

    assert!(rendered.contains("Calypso Operator Surface"));
    assert!(rendered.contains("Feature: feat-tui-surface"));
    assert!(rendered.contains("Branch: feat/cli-tui-operator-surface"));
    assert!(rendered.contains("Workflow: implementation"));
    assert!(rendered.contains("Blocking: rust-quality-green"));
    assert!(rendered.contains("Specification"));
    assert!(rendered.contains("[passing] PR canonicalized"));
    assert!(rendered.contains("Validation"));
    assert!(rendered.contains("[failing] Rust quality green"));
    assert!(rendered.contains("engineer (session_01) [running]"));
    assert!(rendered.contains("Inspecting branch state"));
    assert!(rendered.contains("Waiting on operator guidance"));
    assert!(rendered.contains("Follow-up input:"));
}

#[test]
fn operator_surface_renders_normalized_github_evidence() {
    let mut feature = sample_feature();
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: false,
        review_status: GithubReviewStatus::Approved,
        checks: EvidenceStatus::Passing,
        mergeability: GithubMergeability::Mergeable,
    });

    let rendered = OperatorSurface::from_feature_state(&feature).render();

    assert!(rendered.contains("GitHub"));
    assert!(rendered.contains("PR state: ready-for-review"));
    assert!(rendered.contains("Review: approved"));
    assert!(rendered.contains("Checks: passing"));
    assert!(rendered.contains("Mergeability: mergeable"));
}

#[test]
fn operator_surface_renders_github_error_when_snapshot_is_missing() {
    let mut feature = sample_feature();
    feature.github_error = Some("Run `gh auth login`.".to_string());

    let rendered = OperatorSurface::from_feature_state(&feature).render();

    assert!(rendered.contains("GitHub"));
    assert!(rendered.contains("Error: Run `gh auth login`."));
}

#[test]
fn input_buffer_supports_editing_and_submit() {
    let mut input = InputBuffer::default();

    input.push('h');
    input.push('i');
    input.backspace();
    input.push('!');

    assert_eq!(input.as_str(), "h!");
    assert_eq!(input.submit(), Some("h!".to_string()));
    assert_eq!(input.as_str(), "");
    assert_eq!(input.submit(), None);

    input.push(' ');
    input.push('\t');
    assert_eq!(input.submit(), None);
}

#[test]
fn operator_surface_handles_follow_up_submission_and_quit() {
    let feature = sample_feature();
    let mut surface = OperatorSurface::from_feature_state(&feature);

    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Char('o'))),
        SurfaceEvent::Continue
    );
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Char('k'))),
        SurfaceEvent::Continue
    );
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Enter)),
        SurfaceEvent::Submitted("ok".to_string())
    );
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Esc)),
        SurfaceEvent::Quit
    );

    let feature = sample_feature();
    let mut surface = OperatorSurface::from_feature_state(&feature);
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Backspace)),
        SurfaceEvent::Continue
    );
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Enter)),
        SurfaceEvent::Continue
    );
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Tab)),
        SurfaceEvent::Continue
    );
    assert!(
        surface
            .render()
            .contains("Last event: ignored empty follow-up")
    );
}

#[test]
fn queue_follow_up_routes_message_to_active_session() {
    let mut feature = sample_feature();

    assert!(queue_follow_up(
        &mut feature,
        "Please include the CI logs".to_string()
    ));
    assert_eq!(
        feature.active_sessions[0].pending_follow_ups,
        vec!["Please include the CI logs".to_string()]
    );

    feature.active_sessions[0].status = AgentSessionStatus::Completed;
    assert!(!queue_follow_up(
        &mut feature,
        "This should not be queued".to_string()
    ));
}

#[test]
fn operator_surface_renders_draft_pr_state_label() {
    let mut feature = sample_feature();
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: true,
        review_status: GithubReviewStatus::ReviewRequired,
        checks: EvidenceStatus::Failing,
        mergeability: GithubMergeability::Conflicting,
    });

    let rendered = OperatorSurface::from_feature_state(&feature).render();

    assert!(rendered.contains("PR state: draft"));
    assert!(rendered.contains("Review: review-required"));
    assert!(rendered.contains("Checks: failing"));
    assert!(rendered.contains("Mergeability: conflicting"));
}

#[test]
fn operator_surface_renders_all_github_label_variants() {
    let mut feature = sample_feature();

    // ChangesRequested + Blocked + Pending
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: false,
        review_status: GithubReviewStatus::ChangesRequested,
        checks: EvidenceStatus::Pending,
        mergeability: GithubMergeability::Blocked,
    });
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("Review: changes-requested"));
    assert!(rendered.contains("Checks: pending"));
    assert!(rendered.contains("Mergeability: blocked"));

    // Approved + Unknown + Manual checks
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: false,
        review_status: GithubReviewStatus::Approved,
        checks: EvidenceStatus::Manual,
        mergeability: GithubMergeability::Unknown,
    });
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("Checks: manual"));
    assert!(rendered.contains("Mergeability: unknown"));
}

#[test]
fn operator_surface_renders_empty_and_alternate_status_states() {
    let mut feature = sample_feature();
    feature.workflow_state = WorkflowState::New;
    feature.gate_groups[0].gates[0].status = GateStatus::Manual;
    feature.gate_groups[1].gates[0].status = GateStatus::Pending;
    feature.active_sessions[0].status = AgentSessionStatus::Completed;
    feature.active_sessions[0].output.clear();

    let rendered = OperatorSurface::from_feature_state(&feature).render();

    assert!(rendered.contains("Workflow: new"));
    assert!(rendered.contains("[manual] PR canonicalized"));
    assert!(rendered.contains("[pending] Rust quality green"));
    assert!(rendered.contains("engineer (session_01) [completed]"));
    assert!(rendered.contains("No streamed output yet."));

    feature.workflow_state = WorkflowState::ReleaseReady;
    feature.active_sessions.clear();
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("Workflow: release-ready"));
    assert!(rendered.contains("No active sessions"));

    feature.workflow_state = WorkflowState::Blocked;
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("Workflow: blocked"));

    feature.active_sessions = vec![AgentSession {
        role: "reviewer".to_string(),
        session_id: "session_02".to_string(),
        provider_session_id: None,
        status: AgentSessionStatus::Failed,
        output: Vec::new(),
        pending_follow_ups: Vec::new(),
        terminal_outcome: None,
    }];
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("reviewer (session_02) [failed]"));

    feature.active_sessions[0].status = AgentSessionStatus::Aborted;
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("reviewer (session_02) [aborted]"));
}

#[test]
fn operator_surface_renders_gate_group_rollup_status() {
    let feature = sample_feature();
    let rendered = OperatorSurface::from_feature_state(&feature).render();

    // Specification group: all passing
    assert!(rendered.contains("Specification [passing]"));
    // Validation group: has a failing gate — shows blocked
    assert!(rendered.contains("Validation [blocked]"));
}

#[test]
fn operator_surface_highlights_blocking_gates() {
    let feature = sample_feature();
    let rendered = OperatorSurface::from_feature_state(&feature).render();

    // Passing gate has no blocking marker
    assert!(rendered.contains("[passing] PR canonicalized"));
    assert!(!rendered.contains("[passing] PR canonicalized !"));

    // Failing gate is marked as blocking with " !"
    assert!(rendered.contains("[failing] Rust quality green !"));
}

#[test]
fn operator_surface_renders_pending_clarifications() {
    let mut feature = sample_feature();
    feature.clarification_history = vec![
        ClarificationEntry {
            session_id: "session_01".to_string(),
            question: "Which directory should I write tests to?".to_string(),
            answer: None,
            timestamp: "2026-03-14T10:00:00Z".to_string(),
        },
        ClarificationEntry {
            session_id: "session_01".to_string(),
            question: "Already answered question".to_string(),
            answer: Some("tests/".to_string()),
            timestamp: "2026-03-14T10:01:00Z".to_string(),
        },
    ];

    let surface = OperatorSurface::from_feature_state(&feature);
    let rendered = surface.render();

    assert!(rendered.contains("Pending Clarifications"));
    assert!(rendered.contains("Which directory should I write tests to?"));
    // Answered clarification should not appear in pending section
    assert!(!rendered.contains("Already answered question"));
    assert_eq!(surface.pending_clarification_count(), 1);

    // When there are pending clarifications the input prompt changes
    assert!(rendered.contains("Clarification answer"));
}

#[test]
fn operator_surface_emits_clarification_answered_when_pending_clarification_present() {
    let mut feature = sample_feature();
    feature.clarification_history = vec![ClarificationEntry {
        session_id: "session_01".to_string(),
        question: "What branch should I target?".to_string(),
        answer: None,
        timestamp: "2026-03-14T10:00:00Z".to_string(),
    }];

    let mut surface = OperatorSurface::from_feature_state(&feature);

    // Type an answer and submit
    surface.handle_key_event(KeyEvent::from(KeyCode::Char('m')));
    surface.handle_key_event(KeyEvent::from(KeyCode::Char('a')));
    surface.handle_key_event(KeyEvent::from(KeyCode::Char('i')));
    surface.handle_key_event(KeyEvent::from(KeyCode::Char('n')));

    let event = surface.handle_key_event(KeyEvent::from(KeyCode::Enter));

    assert_eq!(
        event,
        SurfaceEvent::ClarificationAnswered {
            session_id: "session_01".to_string(),
            answer: "main".to_string(),
        }
    );
}

#[test]
fn operator_surface_emits_interrupt_on_ctrl_c() {
    let feature = sample_feature();
    let mut surface = OperatorSurface::from_feature_state(&feature);

    let event = surface.handle_key_event(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));

    assert_eq!(event, SurfaceEvent::Interrupt);
    assert!(surface.render().contains("Last event: interrupt requested"));
}

#[test]
fn interrupt_active_sessions_sets_aborted_status_and_outcome() {
    use calypso_cli::state::AgentTerminalOutcome;

    let mut feature = sample_feature();
    // Add a waiting-for-human session too
    feature.active_sessions.push(AgentSession {
        role: "reviewer".to_string(),
        session_id: "session_02".to_string(),
        provider_session_id: None,
        status: AgentSessionStatus::WaitingForHuman,
        output: Vec::new(),
        pending_follow_ups: Vec::new(),
        terminal_outcome: None,
    });

    interrupt_active_sessions(&mut feature);

    assert_eq!(
        feature.active_sessions[0].status,
        AgentSessionStatus::Aborted
    );
    assert_eq!(
        feature.active_sessions[0].terminal_outcome,
        Some(AgentTerminalOutcome::Aborted)
    );
    assert_eq!(
        feature.active_sessions[1].status,
        AgentSessionStatus::Aborted
    );
    assert_eq!(
        feature.active_sessions[1].terminal_outcome,
        Some(AgentTerminalOutcome::Aborted)
    );
}

#[test]
fn interrupt_active_sessions_does_not_affect_completed_sessions() {
    use calypso_cli::state::AgentTerminalOutcome;

    let mut feature = sample_feature();
    feature.active_sessions[0].status = AgentSessionStatus::Completed;
    feature.active_sessions[0].terminal_outcome = Some(AgentTerminalOutcome::Ok);

    interrupt_active_sessions(&mut feature);

    // Completed session should be unchanged
    assert_eq!(
        feature.active_sessions[0].status,
        AgentSessionStatus::Completed
    );
    assert_eq!(
        feature.active_sessions[0].terminal_outcome,
        Some(AgentTerminalOutcome::Ok)
    );
}

#[test]
fn answer_clarification_fills_first_unanswered_entry() {
    let mut feature = sample_feature();
    feature.clarification_history = vec![
        ClarificationEntry {
            session_id: "session_01".to_string(),
            question: "First question".to_string(),
            answer: None,
            timestamp: "2026-03-14T10:00:00Z".to_string(),
        },
        ClarificationEntry {
            session_id: "session_01".to_string(),
            question: "Second question".to_string(),
            answer: None,
            timestamp: "2026-03-14T10:01:00Z".to_string(),
        },
    ];

    let answered = answer_clarification(&mut feature, "session_01", "my answer".to_string());

    assert!(answered);
    assert_eq!(
        feature.clarification_history[0].answer,
        Some("my answer".to_string())
    );
    // Second question still unanswered
    assert!(feature.clarification_history[1].answer.is_none());
}

#[test]
fn answer_clarification_returns_false_when_no_unanswered_entry() {
    let mut feature = sample_feature();

    let answered = answer_clarification(&mut feature, "session_01", "should not store".to_string());

    assert!(!answered);
}

#[test]
fn operator_surface_renders_without_crashing_on_empty_session() {
    let mut feature = sample_feature();
    feature.active_sessions.clear();
    feature.gate_groups.clear();

    let surface = OperatorSurface::from_feature_state(&feature);
    let rendered = surface.render();

    assert!(rendered.contains("Calypso Operator Surface"));
    assert!(rendered.contains("No active sessions"));
    assert!(rendered.contains("Blocking: none"));
}

#[test]
fn operator_surface_renders_gate_group_status_all_variants() {
    let mut feature = sample_feature();

    // Pending gates
    feature.gate_groups[0].gates[0].status = GateStatus::Pending;
    feature.gate_groups[1].gates[0].status = GateStatus::Pending;
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("Specification [pending]"));

    // Manual gate
    feature.gate_groups[0].gates[0].status = GateStatus::Manual;
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("Specification [manual]"));

    // All passing
    feature.gate_groups[0].gates[0].status = GateStatus::Passing;
    feature.gate_groups[1].gates[0].status = GateStatus::Passing;
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("Specification [passing]"));
    assert!(rendered.contains("Validation [passing]"));
}
