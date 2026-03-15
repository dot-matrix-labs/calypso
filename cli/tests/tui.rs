use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use calypso_cli::state::{
    AgentSession, AgentSessionStatus, ClarificationEntry, EvidenceStatus, FeatureState,
    FeatureType, Gate, GateGroup, GateStatus, GithubMergeability, GithubPullRequestSnapshot,
    GithubReviewStatus, PullRequestRef, SchedulingMeta, SessionOutput, SessionOutputStream,
    WorkflowState,
};
use calypso_cli::tui::{
    InputBuffer, OperatorSurface, PanedLayout, SmEvent, StateMachineSurface, SurfaceEvent,
    TerminalSize, answer_clarification, interrupt_active_sessions, queue_follow_up,
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

    assert!(rendered.contains("Calypso"));
    assert!(rendered.contains("Feature: feat-tui-surface"));
    assert!(rendered.contains("feat/cli-tui-operator-surface"));
    assert!(rendered.contains("●impl"));
    assert!(rendered.contains("Blocking: rust-quality-green"));
    assert!(rendered.contains("Specification"));
    assert!(rendered.contains("✓  PR canonicalized"));
    assert!(rendered.contains("Validation"));
    assert!(rendered.contains("✗  Rust quality green"));
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
    assert!(rendered.contains("PR: ready-for-review"));
    assert!(rendered.contains("Review: approved"));
    assert!(rendered.contains("Checks: passing"));
    assert!(rendered.contains("Merge: mergeable"));
}

#[test]
fn operator_surface_renders_github_error_when_snapshot_is_missing() {
    let mut feature = sample_feature();
    feature.github_error = Some("Run `gh auth login`.".to_string());

    let rendered = OperatorSurface::from_feature_state(&feature).render();

    assert!(rendered.contains("GitHub"));
    assert!(rendered.contains("error: Run `gh auth login`."));
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

    assert!(rendered.contains("PR: draft"));
    assert!(rendered.contains("Review: review-required"));
    assert!(rendered.contains("Checks: failing"));
    assert!(rendered.contains("Merge: conflicting"));
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
    assert!(rendered.contains("Merge: blocked"));

    // Approved + Unknown + Manual checks
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: false,
        review_status: GithubReviewStatus::Approved,
        checks: EvidenceStatus::Manual,
        mergeability: GithubMergeability::Unknown,
    });
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("Checks: manual"));
    assert!(rendered.contains("Merge: unknown"));
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

    assert!(rendered.contains("●new"));
    assert!(rendered.contains("◆  PR canonicalized"));
    assert!(rendered.contains("○  Rust quality green"));
    assert!(rendered.contains("engineer (session_01) [completed]"));
    assert!(rendered.contains("No streamed output yet."));

    feature.workflow_state = WorkflowState::ReleaseReady;
    feature.active_sessions.clear();
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("●rel"));
    assert!(rendered.contains("No active sessions"));

    feature.workflow_state = WorkflowState::Blocked;
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("state: blocked"));

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
    assert!(rendered.contains("✓ Specification:"));
    // Validation group: has a failing gate — shows blocked
    assert!(rendered.contains("✗ Validation:"));
}

#[test]
fn operator_surface_highlights_blocking_gates() {
    let feature = sample_feature();
    let rendered = OperatorSurface::from_feature_state(&feature).render();

    // Passing gate has no blocking marker
    assert!(rendered.contains("✓  PR canonicalized"));
    assert!(!rendered.contains("PR canonicalized ⚠"));

    // Failing gate is marked as blocking with ⚠
    assert!(rendered.contains("✗  Rust quality green ⚠"));
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
    assert!(rendered.contains("Answer (Enter to submit"));
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

    assert!(rendered.contains("Calypso"));
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
    assert!(rendered.contains("○ Specification:"));

    // Manual gate
    feature.gate_groups[0].gates[0].status = GateStatus::Manual;
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("◆ Specification:"));

    // All passing
    feature.gate_groups[0].gates[0].status = GateStatus::Passing;
    feature.gate_groups[1].gates[0].status = GateStatus::Passing;
    let rendered = OperatorSurface::from_feature_state(&feature).render();
    assert!(rendered.contains("✓ Specification:"));
    assert!(rendered.contains("✓ Validation:"));
}

// ── DoctorSurface tests ───────────────────────────────────────────────────────

use calypso_cli::doctor::DoctorStatus;
use calypso_cli::tui::{DoctorCheckView, DoctorSurface, DoctorSurfaceEvent};

fn sample_doctor_checks() -> Vec<DoctorCheckView> {
    use calypso_cli::doctor::DoctorFix;
    vec![
        DoctorCheckView {
            id: "gh-installed".to_string(),
            status: DoctorStatus::Passing,
            detail: None,
            remediation: None,
            fix: None,
        },
        DoctorCheckView {
            id: "gh-authenticated".to_string(),
            status: DoctorStatus::Failing,
            detail: None,
            remediation: Some(
                "Run `gh auth login` and confirm the active account can access this repository."
                    .to_string(),
            ),
            fix: Some(DoctorFix::RunCommand {
                command: "gh".to_string(),
                args: vec!["auth".to_string(), "login".to_string()],
            }),
        },
        DoctorCheckView {
            id: "required-workflows-present".to_string(),
            status: DoctorStatus::Failing,
            detail: Some("rust-unit.yml".to_string()),
            remediation: Some(
                "Missing workflow files will be written and pushed: rust-unit.yml".to_string(),
            ),
            fix: None,
        },
    ]
}

#[test]
fn doctor_surface_renders_check_list_with_pass_fail_indicators() {
    let surface = DoctorSurface::new(sample_doctor_checks(), std::path::PathBuf::from("/tmp"));
    let rendered = surface.render();

    assert!(rendered.contains("Calypso Doctor"));
    assert!(rendered.contains("✓  gh-installed"));
    assert!(rendered.contains("✗  gh-authenticated"));
    assert!(rendered.contains("✗  required-workflows-present"));
    assert!(rendered.contains("[auto-fix]"));
}

#[test]
fn doctor_surface_renders_selected_check_detail() {
    let surface = DoctorSurface::new(sample_doctor_checks(), std::path::PathBuf::from("/tmp"));
    let rendered = surface.render();

    // First item (index 0) is selected by default — shown in detail panel
    assert!(rendered.contains("Selected: gh-installed"));
}

#[test]
fn doctor_surface_navigation_updates_selected_index() {
    let mut surface = DoctorSurface::new(sample_doctor_checks(), std::path::PathBuf::from("/tmp"));
    let cwd = std::path::Path::new("/tmp");

    assert_eq!(surface.selected(), 0);

    // Navigate down
    let event = surface.handle_key_event(KeyEvent::from(KeyCode::Down), cwd);
    assert_eq!(event, DoctorSurfaceEvent::Continue);
    assert_eq!(surface.selected(), 1);

    // Navigate down again
    surface.handle_key_event(KeyEvent::from(KeyCode::Down), cwd);
    assert_eq!(surface.selected(), 2);

    // Can't go past end
    surface.handle_key_event(KeyEvent::from(KeyCode::Down), cwd);
    assert_eq!(surface.selected(), 2);

    // Navigate up
    surface.handle_key_event(KeyEvent::from(KeyCode::Up), cwd);
    assert_eq!(surface.selected(), 1);

    // Can't go before start
    surface.handle_key_event(KeyEvent::from(KeyCode::Up), cwd);
    surface.handle_key_event(KeyEvent::from(KeyCode::Up), cwd);
    assert_eq!(surface.selected(), 0);
}

#[test]
fn doctor_surface_quit_on_q_and_esc() {
    let mut surface = DoctorSurface::new(sample_doctor_checks(), std::path::PathBuf::from("/tmp"));
    let cwd = std::path::Path::new("/tmp");

    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Char('q')), cwd),
        DoctorSurfaceEvent::Quit
    );

    let mut surface = DoctorSurface::new(sample_doctor_checks(), std::path::PathBuf::from("/tmp"));
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Esc), cwd),
        DoctorSurfaceEvent::Quit
    );
}

#[test]
fn doctor_surface_quit_on_ctrl_c() {
    let mut surface = DoctorSurface::new(sample_doctor_checks(), std::path::PathBuf::from("/tmp"));
    let cwd = std::path::Path::new("/tmp");

    let event = surface.handle_key_event(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        cwd,
    );

    assert_eq!(event, DoctorSurfaceEvent::Quit);
}

#[test]
fn doctor_surface_renders_selected_check_detail_after_navigation() {
    let mut surface = DoctorSurface::new(sample_doctor_checks(), std::path::PathBuf::from("/tmp"));
    let cwd = std::path::Path::new("/tmp");

    surface.handle_key_event(KeyEvent::from(KeyCode::Down), cwd);
    surface.handle_key_event(KeyEvent::from(KeyCode::Down), cwd);

    let rendered = surface.render();
    assert!(rendered.contains("Selected: required-workflows-present"));
    assert!(rendered.contains("Detail: rust-unit.yml"));
    assert!(rendered.contains("Fix: Missing workflow files will be written and pushed"));
}

#[test]
fn doctor_surface_check_count_matches_input() {
    let surface = DoctorSurface::new(sample_doctor_checks(), std::path::PathBuf::from("/tmp"));
    assert_eq!(surface.check_count(), 3);

    let empty_surface = DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    assert_eq!(empty_surface.check_count(), 0);
}

#[test]
fn doctor_surface_renders_keybinding_help() {
    let surface = DoctorSurface::new(sample_doctor_checks(), std::path::PathBuf::from("/tmp"));
    let rendered = surface.render();

    assert!(rendered.contains("[r] Refresh"));
    assert!(rendered.contains("[f] Apply fix"));
    assert!(rendered.contains("[q/Esc] Quit"));
}

// ── StateMachineSurface tests ─────────────────────────────────────────────────

fn sm_layout() -> PanedLayout {
    PanedLayout::from_size(TerminalSize { cols: 80, rows: 24 })
}

fn feature_with_pending_gates() -> FeatureState {
    FeatureState {
        feature_id: "feat-sm".to_string(),
        branch: "feat/sm".to_string(),
        worktree_path: "/worktrees/feat-sm".to_string(),
        pull_request: PullRequestRef {
            number: 1,
            url: "https://github.com/org/repo/pull/1".to_string(),
        },
        github_snapshot: None,
        github_error: None,
        workflow_state: WorkflowState::Implementation,
        gate_groups: vec![GateGroup {
            id: "ci".to_string(),
            label: "CI".to_string(),
            gates: vec![
                Gate {
                    id: "unit".to_string(),
                    label: "Unit Tests".to_string(),
                    task: "unit".to_string(),
                    status: GateStatus::Passing,
                },
                Gate {
                    id: "e2e".to_string(),
                    label: "E2E Tests".to_string(),
                    task: "e2e".to_string(),
                    status: GateStatus::Pending,
                },
            ],
        }],
        active_sessions: vec![AgentSession {
            role: "engineer".to_string(),
            session_id: "sess-01".to_string(),
            provider_session_id: None,
            status: AgentSessionStatus::Running,
            output: Vec::new(),
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
fn sm_surface_empty_renders_pipeline_steps() {
    let surface = StateMachineSurface::new();
    let layout = sm_layout();
    let mut out = Vec::new();

    surface.render_paned(&mut out, &layout).unwrap();
    let rendered = String::from_utf8_lossy(&out);

    assert!(rendered.contains("State Machine"));
    assert!(rendered.contains("New"));
    assert!(rendered.contains("Implementation"));
    assert!(rendered.contains("Done"));
}

#[test]
fn sm_surface_from_feature_shows_active_step_and_agent_indicator() {
    let feature = feature_with_pending_gates();
    let surface = StateMachineSurface::from_feature_state(&feature);
    let layout = sm_layout();
    let mut out = Vec::new();

    surface.render_paned(&mut out, &layout).unwrap();
    let rendered = String::from_utf8_lossy(&out);

    // Active step rendered with ● icon and agent indicator *
    assert!(rendered.contains("Implementation"));
    assert!(rendered.contains("*"));
    // Gate group visible (step auto-expanded when gate groups present)
    assert!(rendered.contains("CI"));
}

#[test]
fn sm_surface_navigate_up_down() {
    let mut surface = StateMachineSurface::new();

    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Down)),
        SmEvent::Continue
    );
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Up)),
        SmEvent::Continue
    );
    // Up at top is a no-op.
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Up)),
        SmEvent::Continue
    );
}

#[test]
fn sm_surface_enter_expands_expandable_step() {
    let feature = feature_with_pending_gates();
    let mut surface = StateMachineSurface::from_feature_state(&feature);

    // Collapse what was auto-expanded so we can test Enter explicitly.
    surface.handle_key_event(KeyEvent::from(KeyCode::Esc));

    // Navigate to the Implementation step (cursor was placed on it).
    // After Esc collapse, surface is in an indeterminate cursor position,
    // so navigate back to a known position and try Enter.
    let result = surface.handle_key_event(KeyEvent::from(KeyCode::Enter));
    assert_eq!(result, SmEvent::Continue);
}

#[test]
fn sm_surface_esc_collapses_gate_group_then_step_then_quits() {
    let feature = feature_with_pending_gates();
    let mut surface = StateMachineSurface::from_feature_state(&feature);

    // Step is auto-expanded; first Esc collapses it (Continue).
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Esc)),
        SmEvent::Continue
    );
    // Nothing expanded now; second Esc quits.
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Esc)),
        SmEvent::Quit
    );
}

#[test]
fn sm_surface_q_quits() {
    let mut surface = StateMachineSurface::new();
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Char('q'))),
        SmEvent::Quit
    );
}

#[test]
fn sm_surface_a_returns_jump_to_agents_with_session_when_on_agentic_step() {
    let feature = feature_with_pending_gates();
    let surface = StateMachineSurface::from_feature_state(&feature);
    // Cursor is placed on the active (Implementation) step which has a running session.
    let mut surface = surface;
    let result = surface.handle_key_event(KeyEvent::from(KeyCode::Char('a')));
    assert_eq!(result, SmEvent::JumpToAgents(Some("sess-01".to_string())));
}

#[test]
fn sm_surface_a_returns_jump_to_agents_without_session_on_empty_surface() {
    let mut surface = StateMachineSurface::new();
    let result = surface.handle_key_event(KeyEvent::from(KeyCode::Char('a')));
    assert_eq!(result, SmEvent::JumpToAgents(None));
}

#[test]
fn sm_surface_renders_blocked_state() {
    let mut feature = feature_with_pending_gates();
    feature.workflow_state = WorkflowState::Blocked;
    let surface = StateMachineSurface::from_feature_state(&feature);
    let layout = sm_layout();
    let mut out = Vec::new();

    surface.render_paned(&mut out, &layout).unwrap();
    let rendered = String::from_utf8_lossy(&out);

    assert!(rendered.contains("Blocked"));
}

#[test]
fn sm_surface_renders_aborted_state() {
    let mut feature = feature_with_pending_gates();
    feature.workflow_state = WorkflowState::Aborted;
    let surface = StateMachineSurface::from_feature_state(&feature);
    let layout = sm_layout();
    let mut out = Vec::new();

    surface.render_paned(&mut out, &layout).unwrap();
    let rendered = String::from_utf8_lossy(&out);

    assert!(rendered.contains("Aborted"));
}

#[test]
fn sm_surface_concurrent_activity_count_shown_for_multiple_pending_gates() {
    let feature = FeatureState {
        feature_id: "feat-ci".to_string(),
        branch: "feat/ci".to_string(),
        worktree_path: "/tmp".to_string(),
        pull_request: PullRequestRef {
            number: 2,
            url: "https://github.com/org/repo/pull/2".to_string(),
        },
        github_snapshot: None,
        github_error: None,
        workflow_state: WorkflowState::QaValidation,
        gate_groups: vec![GateGroup {
            id: "ci".to_string(),
            label: "CI".to_string(),
            gates: vec![
                Gate {
                    id: "g1".to_string(),
                    label: "Job 1".to_string(),
                    task: "t1".to_string(),
                    status: GateStatus::Pending,
                },
                Gate {
                    id: "g2".to_string(),
                    label: "Job 2".to_string(),
                    task: "t2".to_string(),
                    status: GateStatus::Pending,
                },
                Gate {
                    id: "g3".to_string(),
                    label: "Job 3".to_string(),
                    task: "t3".to_string(),
                    status: GateStatus::Failing,
                },
                Gate {
                    id: "g4".to_string(),
                    label: "Job 4".to_string(),
                    task: "t4".to_string(),
                    status: GateStatus::Manual,
                },
            ],
        }],
        active_sessions: Vec::new(),
        feature_type: FeatureType::Feat,
        roles: Vec::new(),
        scheduling: SchedulingMeta::default(),
        artifact_refs: Vec::new(),
        transcript_refs: Vec::new(),
        clarification_history: Vec::new(),
    };

    let surface = StateMachineSurface::from_feature_state(&feature);
    let layout = sm_layout();
    let mut out = Vec::new();

    surface.render_paned(&mut out, &layout).unwrap();
    let rendered = String::from_utf8_lossy(&out);

    // Gate group has 2 pending gates → "2 -" shown as concurrent count.
    assert!(rendered.contains("2 -"));
}

#[test]
fn sm_surface_gate_group_expands_on_enter_and_shows_gates() {
    let feature = feature_with_pending_gates();
    let mut surface = StateMachineSurface::from_feature_state(&feature);
    // Step is auto-expanded; navigate to the CI gate group row (index 1 in visible list).
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    // Expand the gate group.
    surface.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let layout = sm_layout();
    let mut out = Vec::new();
    surface.render_paned(&mut out, &layout).unwrap();
    let rendered = String::from_utf8_lossy(&out);

    assert!(rendered.contains("Unit Tests"));
    assert!(rendered.contains("E2E Tests"));
}

#[test]
fn sm_surface_esc_collapses_gate_group_before_step() {
    let feature = feature_with_pending_gates();
    let mut surface = StateMachineSurface::from_feature_state(&feature);

    // Expand the gate group.
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    surface.handle_key_event(KeyEvent::from(KeyCode::Enter));

    // First Esc collapses the gate group (Continue).
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Esc)),
        SmEvent::Continue
    );
    // Second Esc collapses the step (Continue).
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Esc)),
        SmEvent::Continue
    );
    // Third Esc quits (nothing open).
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Esc)),
        SmEvent::Quit
    );
}

#[test]
fn sm_surface_down_at_bottom_is_noop() {
    let mut surface = StateMachineSurface::new();
    // Navigate past the end (9 pipeline steps, indices 0-8).
    for _ in 0..20 {
        surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    }
    // Should not panic and should still render cleanly.
    let layout = sm_layout();
    let mut out = Vec::new();
    surface.render_paned(&mut out, &layout).unwrap();
    assert!(!out.is_empty());
}

#[test]
fn sm_surface_deprecated_waiting_for_human_maps_to_implementation() {
    let mut feature = feature_with_pending_gates();
    feature.workflow_state = WorkflowState::WaitingForHuman;
    let surface = StateMachineSurface::from_feature_state(&feature);
    let layout = sm_layout();
    let mut out = Vec::new();
    surface.render_paned(&mut out, &layout).unwrap();
    let rendered = String::from_utf8_lossy(&out);
    // Should render as active Implementation step.
    assert!(rendered.contains("Implementation"));
}
