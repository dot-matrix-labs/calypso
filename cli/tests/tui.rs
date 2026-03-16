use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use calypso_cli::state::{
    AgentSession, AgentSessionStatus, ClarificationEntry, EvidenceStatus, FeatureState,
    FeatureType, Gate, GateGroup, GateStatus, GithubMergeability, GithubPullRequestSnapshot,
    GithubReviewStatus, PullRequestRef, SchedulingMeta, SessionOutput, SessionOutputStream,
    WorkflowState,
};
use calypso_cli::tui::{
    AppEvent, AppShell, InputBuffer, OperatorSurface, PanedLayout, SmEvent, StateMachineSurface,
    SurfaceEvent, TerminalSize, WorkflowNavigator, answer_clarification, interrupt_active_sessions,
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
        DoctorCheckView {
            id: "codex-installed".to_string(),
            status: DoctorStatus::Warning,
            detail: None,
            remediation: Some(
                "Install Codex CLI and ensure `codex` is available on PATH.".to_string(),
            ),
            fix: Some(DoctorFix::Manual {
                instructions: "Install codex from https://openai.com/codex".to_string(),
            }),
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
    assert!(rendered.contains("⚠  codex-installed"));
    assert!(rendered.contains("[auto-fix]"));
    assert!(rendered.contains("1 warning(s)"));
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

    // Navigate down to last item (warning check)
    surface.handle_key_event(KeyEvent::from(KeyCode::Down), cwd);
    assert_eq!(surface.selected(), 3);

    // Can't go past end
    surface.handle_key_event(KeyEvent::from(KeyCode::Down), cwd);
    assert_eq!(surface.selected(), 3);

    // Navigate up
    surface.handle_key_event(KeyEvent::from(KeyCode::Up), cwd);
    assert_eq!(surface.selected(), 2);

    // Can't go before start
    surface.handle_key_event(KeyEvent::from(KeyCode::Up), cwd);
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
    assert_eq!(surface.check_count(), 4);

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

// ── Agent screen (OperatorSurface session selection) tests ────────────────────

fn op_layout() -> PanedLayout {
    PanedLayout::from_size(TerminalSize { cols: 80, rows: 24 })
}

fn two_session_feature() -> FeatureState {
    let mut feature = sample_feature();
    feature.active_sessions.push(AgentSession {
        role: "reviewer".to_string(),
        session_id: "session_02".to_string(),
        provider_session_id: None,
        status: AgentSessionStatus::Completed,
        output: vec![SessionOutput {
            stream: SessionOutputStream::Stdout,
            text: "Review complete".to_string(),
        }],
        pending_follow_ups: Vec::new(),
        terminal_outcome: None,
    });
    feature
}

#[test]
fn operator_surface_focus_session_sets_selected_index() {
    let feature = sample_feature();
    let mut surface = OperatorSurface::from_feature_state(&feature);

    assert_eq!(surface.selected_session(), None);

    surface.focus_session("session_01");
    assert_eq!(surface.selected_session(), Some(0));

    // Unknown id leaves selection unchanged
    surface.focus_session("no-such-session");
    assert_eq!(surface.selected_session(), None);
}

#[test]
fn operator_surface_focused_session_shown_in_main_pane() {
    let feature = two_session_feature();
    let mut surface = OperatorSurface::from_feature_state(&feature);
    surface.focus_session("session_02");

    let layout = op_layout();
    let mut out = Vec::new();
    surface.render_paned(&mut out, &layout).unwrap();
    let rendered = String::from_utf8_lossy(&out);

    // Focused session detail rendered in main pane
    assert!(rendered.contains("reviewer"));
    assert!(rendered.contains("session_02"));
    assert!(rendered.contains("Review complete"));
}

#[test]
fn operator_surface_sidebar_shows_sessions_header_and_list() {
    let feature = two_session_feature();
    let surface = OperatorSurface::from_feature_state(&feature);

    let layout = op_layout();
    let mut out = Vec::new();
    surface.render_paned(&mut out, &layout).unwrap();
    let rendered = String::from_utf8_lossy(&out);

    // Sidebar header
    assert!(rendered.contains("Sessions"));
    // Both session roles visible in sidebar
    assert!(rendered.contains("engineer"));
    assert!(rendered.contains("reviewer"));
}

#[test]
fn operator_surface_sidebar_shows_cursor_on_selected_session() {
    let feature = two_session_feature();
    let mut surface = OperatorSurface::from_feature_state(&feature);
    surface.focus_session("session_02");

    let layout = op_layout();
    let mut out = Vec::new();
    surface.render_paned(&mut out, &layout).unwrap();
    let rendered = String::from_utf8_lossy(&out);

    // Cursor marker (●) present for the selected session
    assert!(rendered.contains("●"));
}

#[test]
fn operator_surface_down_in_sidebar_selects_session() {
    let feature = two_session_feature();
    let mut surface = OperatorSurface::from_feature_state(&feature);

    // Focus sidebar via Tab
    surface.handle_key_event(KeyEvent::from(KeyCode::Tab));

    // Down selects first session
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    assert_eq!(surface.selected_session(), Some(0));

    // Down again selects second session
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    assert_eq!(surface.selected_session(), Some(1));

    // Down at last clamps
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    assert_eq!(surface.selected_session(), Some(1));

    // Up moves back
    surface.handle_key_event(KeyEvent::from(KeyCode::Up));
    assert_eq!(surface.selected_session(), Some(0));

    // Up at first clamps
    surface.handle_key_event(KeyEvent::from(KeyCode::Up));
    assert_eq!(surface.selected_session(), Some(0));
}

#[test]
fn operator_surface_up_down_scrolls_when_sidebar_not_focused() {
    let feature = sample_feature();
    let mut surface = OperatorSurface::from_feature_state(&feature);

    // Sidebar not focused by default — Up/Down should scroll
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    // selected_session stays None (scrolling, not session navigation)
    assert_eq!(surface.selected_session(), None);
}

#[test]
fn operator_surface_down_no_op_when_sidebar_focused_and_no_sessions() {
    let mut feature = sample_feature();
    feature.active_sessions.clear();
    let mut surface = OperatorSurface::from_feature_state(&feature);

    surface.handle_key_event(KeyEvent::from(KeyCode::Tab)); // focus sidebar
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    assert_eq!(surface.selected_session(), None);
}

// ── AppShell agent tab event routing tests ────────────────────────────────────

fn shell_with_operator(feature: &FeatureState) -> AppShell {
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let op = OperatorSurface::from_feature_state(feature);
    AppShell::new(doctor).with_operator(op)
}

#[test]
fn app_shell_agents_tab_follow_up_returns_follow_up_submitted() {
    let feature = sample_feature();
    let mut shell = shell_with_operator(&feature);
    shell.tab = calypso_cli::tui::AppTab::Agents;
    let cwd = std::path::Path::new("/tmp");

    // Type a message and submit
    shell.handle_key_event(KeyEvent::from(KeyCode::Char('h')), cwd);
    shell.handle_key_event(KeyEvent::from(KeyCode::Char('i')), cwd);
    let event = shell.handle_key_event(KeyEvent::from(KeyCode::Enter), cwd);

    assert_eq!(event, AppEvent::FollowUpSubmitted("hi".to_string()));
}

#[test]
fn app_shell_agents_tab_interrupt_returns_interrupted() {
    let feature = sample_feature();
    let mut shell = shell_with_operator(&feature);
    shell.tab = calypso_cli::tui::AppTab::Agents;
    let cwd = std::path::Path::new("/tmp");

    let event = shell.handle_key_event(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        cwd,
    );

    // Shell-level Ctrl+C quits (intercepted before delegation)
    assert_eq!(event, AppEvent::Quit);
}

#[test]
fn app_shell_sm_jump_to_agents_focuses_session() {
    let feature = sample_feature(); // has session_01 for "engineer"
    let mut shell = shell_with_operator(&feature);
    let cwd = std::path::Path::new("/tmp");

    // Start on SM tab; press 'a' to jump to Agents with session focus
    shell.tab = calypso_cli::tui::AppTab::StateMachine;
    let sm = calypso_cli::tui::StateMachineSurface::from_feature_state(&feature);
    shell.sm = sm;

    shell.handle_key_event(KeyEvent::from(KeyCode::Char('a')), cwd);

    assert_eq!(shell.tab, calypso_cli::tui::AppTab::Agents);
    // Operator should have session_01 focused (index 0)
    let selected = shell.operator.as_ref().unwrap().selected_session();
    assert_eq!(selected, Some(0));
}

// ── WorkflowNavigator tests ──────────────────────────────────────────────────

#[test]
fn workflow_navigator_loads_all_entry_points() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let nav = WorkflowNavigator::from_interpreter(&interp);
    // The embedded blueprints produce several entry points.
    assert!(
        nav.entry_count() >= 4,
        "expected at least 4 entry points, got {}",
        nav.entry_count()
    );
}

#[test]
fn workflow_navigator_expand_collapse_entry() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);

    // Initial state: all collapsed, cursor at 0.
    let event = nav.handle_key_event(KeyEvent::from(KeyCode::Enter));
    assert_eq!(event, SmEvent::Continue);

    // Down then expand the second entry.
    nav.handle_key_event(KeyEvent::from(KeyCode::Down));
    nav.handle_key_event(KeyEvent::from(KeyCode::Enter));

    // Collapse with Left.
    let event = nav.handle_key_event(KeyEvent::from(KeyCode::Left));
    assert_eq!(event, SmEvent::Continue);

    // Collapse with Esc when nothing expanded should quit.
    let event = nav.handle_key_event(KeyEvent::from(KeyCode::Esc));
    assert_eq!(event, SmEvent::Quit);
}

#[test]
fn workflow_navigator_g_jumps_to_top() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);

    // Move down a few rows.
    nav.handle_key_event(KeyEvent::from(KeyCode::Down));
    nav.handle_key_event(KeyEvent::from(KeyCode::Down));

    // 'g' jumps to top.
    nav.handle_key_event(KeyEvent::from(KeyCode::Char('g')));
    // 'q' quits.
    let event = nav.handle_key_event(KeyEvent::from(KeyCode::Char('q')));
    assert_eq!(event, SmEvent::Quit);
}

#[test]
fn workflow_navigator_a_jumps_to_agents() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);

    let event = nav.handle_key_event(KeyEvent::from(KeyCode::Char('a')));
    assert_eq!(event, SmEvent::JumpToAgents(None));
}

#[test]
fn workflow_navigator_renders_without_crash() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let nav = WorkflowNavigator::from_interpreter(&interp);

    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    nav.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(
        output.contains("Workflows"),
        "header should show 'Workflows'"
    );
}

#[test]
fn workflow_navigator_renders_expanded_states() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);

    // Expand first entry.
    nav.handle_key_event(KeyEvent::from(KeyCode::Enter));

    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 40,
    });
    let mut buf = Vec::new();
    nav.render_paned(&mut buf, &layout).unwrap();
    // Should contain at least the expanded indicator.
    let output = String::from_utf8_lossy(&buf);
    assert!(
        output.contains("▾"),
        "expanded entry should show collapse indicator"
    );
}

#[test]
fn workflow_navigator_set_active_position() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);

    // Use a real workflow name from the embedded blueprints.
    let exec = calypso_cli::interpreter::WorkflowExecutionState {
        position: calypso_cli::interpreter::WorkflowPosition {
            workflow: "calypso-orchestrator-startup".to_string(),
            state: "scan-repository".to_string(),
        },
        call_stack: vec![],
    };
    nav.set_active_position(&exec);

    // The navigator should have auto-expanded the active workflow entry.
    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 40,
    });
    let mut buf = Vec::new();
    nav.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(output.contains("●"), "active state should show ● marker");
}

#[test]
fn app_shell_with_navigator_uses_navigator_for_sm_tab() {
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let nav = WorkflowNavigator::from_interpreter(&interp);
    let shell = AppShell::new(doctor).with_navigator(nav);

    // SM tab should use navigator, not the legacy SM surface.
    assert!(shell.wf_nav.is_some());

    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    // Render SM tab.
    let mut shell = shell;
    shell.tab = calypso_cli::tui::AppTab::StateMachine;
    shell.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(
        output.contains("Workflows"),
        "SM tab should show workflow navigator header"
    );
}

#[test]
fn app_shell_with_operator_shows_agents_tab() {
    let feature = sample_feature();
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let op = OperatorSurface::from_feature_state(&feature);
    let mut shell = AppShell::new(doctor).with_operator(op);
    shell.tab = calypso_cli::tui::AppTab::Agents;

    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    shell.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    // Should show operator surface, not the placeholder.
    assert!(
        output.contains("Feature:"),
        "agents tab with operator should show feature header"
    );
}

#[test]
fn app_shell_sm_tab_delegates_keys_to_navigator() {
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let nav = WorkflowNavigator::from_interpreter(&interp);
    let mut shell = AppShell::new(doctor).with_navigator(nav);
    shell.tab = calypso_cli::tui::AppTab::StateMachine;
    let cwd = std::path::Path::new("/tmp");

    // 'a' should jump to agents tab via navigator.
    let event = shell.handle_key_event(KeyEvent::from(KeyCode::Char('a')), cwd);
    assert_eq!(event, AppEvent::Continue);
    assert_eq!(shell.tab, calypso_cli::tui::AppTab::Agents);
}

// ── Navigator collapse / expand coverage ────────────────────────────────────

#[test]
fn workflow_navigator_left_collapses_expanded_entry() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);

    // Expand then collapse with Left arrow.
    assert_eq!(
        nav.handle_key_event(KeyEvent::from(KeyCode::Enter)),
        SmEvent::Continue
    );
    assert_eq!(
        nav.handle_key_event(KeyEvent::from(KeyCode::Left)),
        SmEvent::Continue
    );

    // After collapse, re-render should not show expanded marker.
    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    nav.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(
        !output.contains("▾"),
        "entry should be collapsed after Left"
    );
}

#[test]
fn workflow_navigator_esc_collapses_expanded_entry_then_quits() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);

    // Expand first entry.
    nav.handle_key_event(KeyEvent::from(KeyCode::Enter));

    // Esc should collapse expanded entries first, not quit.
    assert_eq!(
        nav.handle_key_event(KeyEvent::from(KeyCode::Esc)),
        SmEvent::Continue
    );

    // Second Esc should quit since nothing is expanded.
    assert_eq!(
        nav.handle_key_event(KeyEvent::from(KeyCode::Esc)),
        SmEvent::Quit
    );
}

#[test]
fn workflow_navigator_g_uppercase_jumps_to_bottom() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);

    // Move down a bit to confirm G moves further.
    nav.handle_key_event(KeyEvent::from(KeyCode::Down));
    // Jump to bottom.
    assert_eq!(
        nav.handle_key_event(KeyEvent::from(KeyCode::Char('G'))),
        SmEvent::Continue
    );
}

#[test]
fn workflow_navigator_expand_state_node() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);

    // Expand first entry to reveal states.
    nav.handle_key_event(KeyEvent::from(KeyCode::Enter));
    // Move down to first state row.
    nav.handle_key_event(KeyEvent::from(KeyCode::Down));
    // Try to expand the state.
    nav.handle_key_event(KeyEvent::from(KeyCode::Enter));
    // Left on a state should collapse its sub-expansion or parent.
    nav.handle_key_event(KeyEvent::from(KeyCode::Left));

    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 40,
    });
    let mut buf = Vec::new();
    nav.render_paned(&mut buf, &layout).unwrap();
    // Just verify it renders without panic.
    assert!(!buf.is_empty());
}

#[test]
fn workflow_navigator_left_on_state_collapses_parent_entry() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);

    // Expand first entry.
    nav.handle_key_event(KeyEvent::from(KeyCode::Enter));
    // Move down to a state row.
    nav.handle_key_event(KeyEvent::from(KeyCode::Down));
    // Left on a state row should collapse the parent entry.
    nav.handle_key_event(KeyEvent::from(KeyCode::Left));

    // Esc should now quit since nothing is expanded.
    assert_eq!(
        nav.handle_key_event(KeyEvent::from(KeyCode::Esc)),
        SmEvent::Quit
    );
}

#[test]
fn workflow_navigator_unknown_key_returns_continue() {
    let interp = calypso_cli::interpreter::WorkflowInterpreter::new().unwrap();
    let mut nav = WorkflowNavigator::from_interpreter(&interp);
    assert_eq!(
        nav.handle_key_event(KeyEvent::from(KeyCode::Char('z'))),
        SmEvent::Continue
    );
}

// ── StateMachineSurface coverage ────────────────────────────────────────────

#[test]
fn sm_surface_default_trait_creates_empty_surface() {
    let surface: StateMachineSurface = Default::default();
    let layout = sm_layout();
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    // Should render without crashing.
    assert!(!buf.is_empty());
}

#[test]
fn sm_surface_deprecated_waiting_for_human_maps_to_implementation_visible_rows() {
    let mut feature = feature_with_pending_gates();
    feature.workflow_state = WorkflowState::WaitingForHuman;
    let surface = StateMachineSurface::from_feature_state(&feature);
    let layout = sm_layout();
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    // The active marker should appear on the Implementation step.
    assert!(
        output.contains("Implementation"),
        "WaitingForHuman should map to Implementation"
    );
}

#[test]
fn sm_surface_deprecated_ready_for_review_maps_to_release_ready() {
    let mut feature = feature_with_pending_gates();
    feature.workflow_state = WorkflowState::ReadyForReview;
    let surface = StateMachineSurface::from_feature_state(&feature);
    let layout = sm_layout();
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(
        output.contains("Release Ready"),
        "ReadyForReview should map to Release Ready"
    );
}

#[test]
fn sm_surface_manual_gate_shows_manual_icon() {
    let mut feature = feature_with_pending_gates();
    feature.gate_groups[0].gates[1].status = GateStatus::Manual;
    let surface = StateMachineSurface::from_feature_state(&feature);
    let layout = sm_layout();
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(output.contains("◆"), "manual gate should show ◆ icon");
}

#[test]
fn sm_surface_left_collapses_expanded_step() {
    let feature = feature_with_pending_gates();
    let mut surface = StateMachineSurface::from_feature_state(&feature);
    // Move to a gate group and expand it.
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    surface.handle_key_event(KeyEvent::from(KeyCode::Enter));
    // Left should collapse the gate group.
    surface.handle_key_event(KeyEvent::from(KeyCode::Left));
    assert_eq!(
        surface.handle_key_event(KeyEvent::from(KeyCode::Char('q'))),
        SmEvent::Quit
    );
}

#[test]
fn sm_surface_gate_node_expand_is_noop() {
    let feature = feature_with_pending_gates();
    let mut surface = StateMachineSurface::from_feature_state(&feature);
    // Navigate to the expanded step, then to a gate group, expand it, then to a gate.
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    surface.handle_key_event(KeyEvent::from(KeyCode::Enter));
    surface.handle_key_event(KeyEvent::from(KeyCode::Down));
    // Trying to expand a gate row should be a no-op.
    surface.handle_key_event(KeyEvent::from(KeyCode::Enter));
    let layout = sm_layout();
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    assert!(!buf.is_empty());
}

// ── AppShell coverage ───────────────────────────────────────────────────────

#[test]
fn app_shell_with_sm_sets_sm_surface() {
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let sm = StateMachineSurface::from_feature_state(&feature_with_pending_gates());
    let shell = AppShell::new(doctor).with_sm(sm);
    // wf_nav should remain None when using with_sm.
    assert!(shell.wf_nav.is_none());
}

#[test]
fn app_shell_sm_tab_quit_from_legacy_surface() {
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let sm = StateMachineSurface::new();
    let mut shell = AppShell::new(doctor).with_sm(sm);
    shell.tab = calypso_cli::tui::AppTab::StateMachine;
    let cwd = std::path::Path::new("/tmp");

    // 'q' on legacy SM surface should quit.
    let event = shell.handle_key_event(KeyEvent::from(KeyCode::Char('q')), cwd);
    assert_eq!(event, AppEvent::Quit);
}

#[test]
fn app_shell_tab_switching_at_boundaries() {
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let mut shell = AppShell::new(doctor);
    let cwd = std::path::Path::new("/tmp");

    // Left at Doctor tab stays on Doctor.
    shell.handle_key_event(KeyEvent::from(KeyCode::Left), cwd);
    assert_eq!(shell.tab, calypso_cli::tui::AppTab::Doctor);

    // Right moves to StateMachine then Agents.
    shell.handle_key_event(KeyEvent::from(KeyCode::Right), cwd);
    assert_eq!(shell.tab, calypso_cli::tui::AppTab::StateMachine);
    shell.handle_key_event(KeyEvent::from(KeyCode::Right), cwd);
    assert_eq!(shell.tab, calypso_cli::tui::AppTab::Agents);

    // Right at Agents stays on Agents.
    shell.handle_key_event(KeyEvent::from(KeyCode::Right), cwd);
    assert_eq!(shell.tab, calypso_cli::tui::AppTab::Agents);
}

#[test]
fn app_shell_ctrl_c_quits_from_any_tab() {
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let mut shell = AppShell::new(doctor);
    shell.tab = calypso_cli::tui::AppTab::Agents;
    let cwd = std::path::Path::new("/tmp");

    let event = shell.handle_key_event(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        cwd,
    );
    assert_eq!(event, AppEvent::Quit);
}

#[test]
fn app_shell_agents_tab_without_operator_uses_placeholder() {
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let mut shell = AppShell::new(doctor);
    shell.tab = calypso_cli::tui::AppTab::Agents;
    let cwd = std::path::Path::new("/tmp");

    // 'q' on placeholder should quit.
    let event = shell.handle_key_event(KeyEvent::from(KeyCode::Char('q')), cwd);
    assert_eq!(event, AppEvent::Quit);

    // Unknown key should continue.
    let event = shell.handle_key_event(KeyEvent::from(KeyCode::Char('x')), cwd);
    assert_eq!(event, AppEvent::Continue);
}

#[test]
fn app_shell_renders_sm_tab_with_legacy_surface() {
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let sm = StateMachineSurface::from_feature_state(&feature_with_pending_gates());
    let mut shell = AppShell::new(doctor).with_sm(sm);
    shell.tab = calypso_cli::tui::AppTab::StateMachine;

    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    shell.render_paned(&mut buf, &layout).unwrap();
    assert!(!buf.is_empty());
}

#[test]
fn app_shell_renders_agents_tab_without_operator() {
    let doctor = calypso_cli::tui::DoctorSurface::new(vec![], std::path::PathBuf::from("/tmp"));
    let mut shell = AppShell::new(doctor);
    shell.tab = calypso_cli::tui::AppTab::Agents;

    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    shell.render_paned(&mut buf, &layout).unwrap();
    assert!(!buf.is_empty());
}

// ── OperatorSurface render coverage ─────────────────────────────────────────

#[test]
fn operator_surface_render_with_github_snapshot() {
    let mut feature = sample_feature();
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: false,
        review_status: GithubReviewStatus::Approved,
        checks: EvidenceStatus::Passing,
        mergeability: GithubMergeability::Mergeable,
    });
    let surface = OperatorSurface::from_feature_state(&feature);
    let output = surface.render();
    assert!(
        output.contains("GitHub"),
        "render should include GitHub section"
    );
    assert!(
        output.contains("Review:"),
        "render should show review field"
    );
}

#[test]
fn operator_surface_render_with_github_error() {
    let mut feature = sample_feature();
    feature.github_error = Some("rate limit exceeded".to_string());
    let surface = OperatorSurface::from_feature_state(&feature);
    let output = surface.render();
    assert!(
        output.contains("GitHub  error: rate limit exceeded"),
        "render should display github error"
    );
}

#[test]
fn operator_surface_render_with_manual_gate() {
    let mut feature = sample_feature();
    feature.gate_groups[0].gates[0].status = GateStatus::Manual;
    let surface = OperatorSurface::from_feature_state(&feature);
    let output = surface.render();
    assert!(output.contains("◆"), "manual gate should show ◆ icon");
}

#[test]
fn operator_surface_render_selected_session_failed_status() {
    let mut feature = sample_feature();
    feature.active_sessions[0].status = AgentSessionStatus::Failed;
    feature.active_sessions[0].output.clear();
    let mut surface = OperatorSurface::from_feature_state(&feature);
    // Select the session.
    surface.focus_session("session_01");
    let output = surface.render();
    assert!(output.contains("✗"), "failed session should show ✗ icon");
    // Empty source output maps to "No streamed output yet." in the session view.
    assert!(
        output.contains("No streamed output"),
        "empty session output should show placeholder"
    );
}

#[test]
fn operator_surface_render_session_aborted_status() {
    let mut feature = sample_feature();
    feature.active_sessions[0].status = AgentSessionStatus::Aborted;
    let surface = OperatorSurface::from_feature_state(&feature);
    let output = surface.render();
    assert!(output.contains("⊗"), "aborted session should show ⊗ icon");
}

#[test]
fn operator_surface_render_no_sessions() {
    let mut feature = sample_feature();
    feature.active_sessions.clear();
    let surface = OperatorSurface::from_feature_state(&feature);
    let output = surface.render();
    assert!(
        output.contains("No active sessions"),
        "empty sessions should show placeholder"
    );
}

// ── OperatorSurface paned render coverage ───────────────────────────────────

#[test]
fn operator_surface_render_paned_with_github_snapshot() {
    let mut feature = sample_feature();
    feature.github_snapshot = Some(GithubPullRequestSnapshot {
        is_draft: false,
        review_status: GithubReviewStatus::Approved,
        checks: EvidenceStatus::Passing,
        mergeability: GithubMergeability::Mergeable,
    });
    let surface = OperatorSurface::from_feature_state(&feature);
    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(
        output.contains("GitHub"),
        "paned render should include GitHub section"
    );
}

#[test]
fn operator_surface_render_paned_with_github_error() {
    let mut feature = sample_feature();
    feature.github_error = Some("rate limited".to_string());
    let surface = OperatorSurface::from_feature_state(&feature);
    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(
        output.contains("rate limited"),
        "paned render should show GitHub error"
    );
}

#[test]
fn operator_surface_render_paned_with_manual_gates() {
    let mut feature = sample_feature();
    feature.gate_groups[0].gates[0].status = GateStatus::Manual;
    let surface = OperatorSurface::from_feature_state(&feature);
    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(output.contains("◆"), "paned render should show manual icon");
}

#[test]
fn operator_surface_render_paned_no_sessions() {
    let mut feature = sample_feature();
    feature.active_sessions.clear();
    let surface = OperatorSurface::from_feature_state(&feature);
    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(
        output.contains("No active sessions"),
        "paned render should show 'No active sessions'"
    );
}

#[test]
fn operator_surface_render_paned_with_failed_session() {
    let mut feature = sample_feature();
    feature.active_sessions[0].status = AgentSessionStatus::Failed;
    let surface = OperatorSurface::from_feature_state(&feature);
    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    assert!(!buf.is_empty());
}

#[test]
fn operator_surface_render_paned_with_clarification() {
    let mut feature = sample_feature();
    feature.active_sessions[0].status = AgentSessionStatus::WaitingForHuman;
    feature.clarification_history.push(ClarificationEntry {
        session_id: "session_01".to_string(),
        question: "What database should we use?".to_string(),
        answer: None,
        timestamp: "2026-01-01T00:00:00Z".to_string(),
    });
    let surface = OperatorSurface::from_feature_state(&feature);
    let layout = PanedLayout::from_size(TerminalSize {
        cols: 120,
        rows: 30,
    });
    let mut buf = Vec::new();
    surface.render_paned(&mut buf, &layout).unwrap();
    let output = String::from_utf8_lossy(&buf);
    assert!(
        output.contains("What database"),
        "paned render should show clarification question"
    );
}

// ── run_watch_with coverage ─────────────────────────────────────────────────

#[test]
fn run_watch_with_missing_file_returns_error() {
    let result = calypso_cli::tui::run_watch_with("/tmp/nonexistent_state.json", |_| Ok(()));
    assert!(result.is_err());
}

#[test]
fn run_watch_with_valid_state_roundtrips() {
    use calypso_cli::state::{
        DeploymentRecord, ReleaseRecord, RepositoryIdentity, RepositoryState,
    };
    let dir = std::env::temp_dir().join(format!("calypso_test_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.json");

    let state = RepositoryState {
        version: 1,
        schema_version: 1,
        repo_id: "test-repo".to_string(),
        identity: RepositoryIdentity::default(),
        providers: Vec::new(),
        releases: Vec::<ReleaseRecord>::new(),
        deployments: Vec::<DeploymentRecord>::new(),
        current_feature: sample_feature(),
    };
    state.save_to_path(&path).unwrap();

    let result = calypso_cli::tui::run_watch_with(path.to_str().unwrap(), |feature| {
        feature.workflow_state = WorkflowState::QaValidation;
        Ok(())
    });
    assert!(result.is_ok());

    let reloaded = RepositoryState::load_from_path(&path).unwrap();
    assert_eq!(
        reloaded.current_feature.workflow_state,
        WorkflowState::QaValidation
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_watch_with_runner_error_propagates() {
    use calypso_cli::state::{
        DeploymentRecord, ReleaseRecord, RepositoryIdentity, RepositoryState,
    };
    let dir = std::env::temp_dir().join(format!("calypso_test_err_{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("state.json");

    let state = RepositoryState {
        version: 1,
        schema_version: 1,
        repo_id: "test-repo".to_string(),
        identity: RepositoryIdentity::default(),
        providers: Vec::new(),
        releases: Vec::<ReleaseRecord>::new(),
        deployments: Vec::<DeploymentRecord>::new(),
        current_feature: sample_feature(),
    };
    state.save_to_path(&path).unwrap();

    let result = calypso_cli::tui::run_watch_with(path.to_str().unwrap(), |_| {
        Err(std::io::Error::other("test runner error"))
    });
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("test runner error"));
    let _ = std::fs::remove_dir_all(&dir);
}
