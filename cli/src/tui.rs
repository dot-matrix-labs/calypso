use std::io::{self, Write};
#[cfg(not(coverage))]
use std::time::Duration;

use crossterm::cursor::MoveTo;
#[cfg(not(coverage))]
use crossterm::cursor::{Hide, Show};
#[cfg(not(coverage))]
use crossterm::event::{self};
use crossterm::event::{Event, KeyCode, KeyEvent, KeyModifiers};
#[cfg(not(coverage))]
use crossterm::execute;
use crossterm::queue;
use crossterm::terminal::{Clear, ClearType};
#[cfg(not(coverage))]
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::state::{
    AgentSessionStatus, AgentTerminalOutcome, EvidenceStatus, FeatureState, GateGroupStatus,
    GateStatus, GithubMergeability, GithubReviewStatus, WorkflowState,
};

// TODO: browser view — serve operator surface as WASM inside the binary (no external bundle files).
// Both surfaces must read from the same underlying state and event model. Deferred: WASM packaging
// complexity is non-trivial; implement once the TUI surface is stable.

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct InputBuffer {
    value: String,
}

impl InputBuffer {
    pub fn push(&mut self, character: char) {
        self.value.push(character);
    }

    pub fn backspace(&mut self) {
        self.value.pop();
    }

    pub fn submit(&mut self) -> Option<String> {
        if self.value.trim().is_empty() {
            self.value.clear();
            None
        } else {
            Some(std::mem::take(&mut self.value))
        }
    }

    pub fn as_str(&self) -> &str {
        self.value.as_str()
    }
}

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
    input: InputBuffer,
    queued_follow_ups: Vec<String>,
    pub last_event: String,
    /// Row offset for scrolling the main pane (paned rendering only).
    scroll_offset: usize,
    /// Whether keyboard focus is on the sidebar (paned rendering only).
    sidebar_focused: bool,
    /// Index of the session currently focused in the sidebar sidebar (None = overview).
    selected_session: Option<usize>,
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
            input: InputBuffer::default(),
            queued_follow_ups: feature
                .active_sessions
                .iter()
                .flat_map(|session| session.pending_follow_ups.iter().cloned())
                .collect(),
            last_event: "idle".to_string(),
            scroll_offset: 0,
            sidebar_focused: false,
            selected_session: None,
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
                    lines.push(format!("    {}", output));
                }
            }
        }

        lines.push(String::new());

        if !self.pending_clarifications.is_empty() {
            lines.push(format!(
                "  Answer (Enter to submit, Ctrl+C to abort): {}▌",
                self.input.as_str()
            ));
        } else {
            lines.push(format!("  Follow-up input: {}▌", self.input.as_str()));
        }

        lines.push(String::new());
        lines.push("  [Ctrl+C] Interrupt session  [Esc] Quit".to_string());

        lines.join("\n")
    }

    pub fn handle_key_event(&mut self, event: KeyEvent) -> SurfaceEvent {
        // Ctrl+C triggers session interrupt (abort)
        if event.code == KeyCode::Char('c') && event.modifiers.contains(KeyModifiers::CONTROL) {
            self.last_event = "interrupt requested".to_string();
            return SurfaceEvent::Interrupt;
        }

        match event.code {
            KeyCode::Char(character) => {
                self.input.push(character);
                self.last_event = "typing".to_string();
                SurfaceEvent::Continue
            }
            KeyCode::Backspace => {
                self.input.backspace();
                self.last_event = "editing".to_string();
                SurfaceEvent::Continue
            }
            KeyCode::Enter => match self.input.submit() {
                Some(text) => {
                    if !self.pending_clarifications.is_empty() {
                        // Answer routes as a clarification reply
                        SurfaceEvent::ClarificationAnswered {
                            session_id: self.pending_clarifications[0].session_id.clone(),
                            answer: text,
                        }
                    } else {
                        SurfaceEvent::Submitted(text)
                    }
                }
                None => {
                    self.last_event = "ignored empty follow-up".to_string();
                    SurfaceEvent::Continue
                }
            },
            KeyCode::Esc => {
                self.last_event = "quit requested".to_string();
                SurfaceEvent::Quit
            }
            KeyCode::Tab => {
                self.sidebar_focused = !self.sidebar_focused;
                SurfaceEvent::Continue
            }
            KeyCode::Up => {
                if self.sidebar_focused && !self.sessions.is_empty() {
                    if let Some(sel) = self.selected_session {
                        self.selected_session = sel.checked_sub(1).or(Some(0));
                    }
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_sub(1);
                }
                SurfaceEvent::Continue
            }
            KeyCode::Down => {
                if self.sidebar_focused && !self.sessions.is_empty() {
                    let max = self.sessions.len().saturating_sub(1);
                    self.selected_session =
                        Some(self.selected_session.map(|s| (s + 1).min(max)).unwrap_or(0));
                } else {
                    self.scroll_offset = self.scroll_offset.saturating_add(1);
                }
                SurfaceEvent::Continue
            }
            _ => SurfaceEvent::Continue,
        }
    }

    /// Returns the number of pending clarifications visible in this surface snapshot.
    pub fn pending_clarification_count(&self) -> usize {
        self.pending_clarifications.len()
    }

    /// Focus a specific session by session_id. Sets `selected_session` to its index in the
    /// sessions list, enabling the focused-output view in the main pane.
    pub fn focus_session(&mut self, session_id: &str) {
        self.selected_session = self
            .sessions
            .iter()
            .position(|s| s.session_id == session_id);
    }

    /// Returns the index of the currently selected session, if any.
    pub fn selected_session(&self) -> Option<usize> {
        self.selected_session
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceEvent {
    Continue,
    Submitted(String),
    /// Operator answered a pending clarification question.
    ClarificationAnswered {
        session_id: String,
        answer: String,
    },
    /// Operator requested interruption of the active session (Ctrl+C).
    Interrupt,
    Quit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GithubView {
    pr_state: String,
    review: String,
    checks: String,
    mergeability: String,
}

#[cfg(not(coverage))]
pub fn run_terminal_surface(feature: &mut FeatureState) -> io::Result<()> {
    let mut stdout = io::stdout();
    run_terminal_surface_with(
        &mut stdout,
        feature,
        |stdout| {
            enable_raw_mode()?;
            execute!(stdout, EnterAlternateScreen, Hide)?;
            Ok(())
        },
        |stdout| {
            execute!(stdout, Show, LeaveAlternateScreen)?;
            disable_raw_mode()?;
            Ok(())
        },
        run_terminal_loop,
    )
}

#[cfg(coverage)]
pub fn run_terminal_surface(feature: &mut FeatureState) -> io::Result<()> {
    let mut stdout = io::sink();
    let mut surface = OperatorSurface::from_feature_state(feature);
    let mut layout: Option<PanedLayout> = None;

    let resize = Some(Event::Resize(80, 24));
    let type_a = Some(Event::Key(KeyEvent::from(KeyCode::Char('a'))));
    let submit = Some(Event::Key(KeyEvent::from(KeyCode::Enter)));
    let type_b = Some(Event::Key(KeyEvent::from(KeyCode::Char('b'))));
    let interrupt = Some(Event::Key(KeyEvent::new(
        KeyCode::Char('c'),
        KeyModifiers::CONTROL,
    )));
    let type_c = Some(Event::Key(KeyEvent::from(KeyCode::Char('c'))));
    let tab = Some(Event::Key(KeyEvent::from(KeyCode::Tab)));
    let scroll_down = Some(Event::Key(KeyEvent::from(KeyCode::Down)));
    let scroll_up = Some(Event::Key(KeyEvent::from(KeyCode::Up)));
    let quit = Some(Event::Key(KeyEvent::from(KeyCode::Esc)));

    // Resize first — sets layout, activates paned rendering for subsequent frames.
    run_terminal_iteration(&mut stdout, feature, &mut surface, resize, &mut layout)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, type_a, &mut layout)?;
    run_terminal_iteration(
        &mut stdout,
        feature,
        &mut surface,
        submit.clone(),
        &mut layout,
    )?;
    if let Some(active_session) = feature.active_sessions.first_mut() {
        active_session.status = AgentSessionStatus::Completed;
    }
    run_terminal_iteration(&mut stdout, feature, &mut surface, type_b, &mut layout)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, submit, &mut layout)?;
    // Exercise interrupt path — should NOT quit (just aborts active sessions)
    run_terminal_iteration(&mut stdout, feature, &mut surface, interrupt, &mut layout)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, type_c, &mut layout)?;
    // Exercise Tab (sidebar focus) + session navigation (Down/Up selects sessions)
    run_terminal_iteration(&mut stdout, feature, &mut surface, tab.clone(), &mut layout)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, scroll_down.clone(), &mut layout)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, scroll_up.clone(), &mut layout)?;
    // Tab back to main focus and exercise scroll
    run_terminal_iteration(&mut stdout, feature, &mut surface, tab, &mut layout)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, scroll_down, &mut layout)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, scroll_up, &mut layout)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, quit, &mut layout)?;
    Ok(())
}

#[cfg(not(coverage))]
fn run_terminal_loop(
    stdout: &mut impl Write,
    feature: &mut FeatureState,
    surface: &mut OperatorSurface,
) -> io::Result<()> {
    // Initialise the paned layout from the current terminal size.
    let mut layout: Option<PanedLayout> = crossterm::terminal::size()
        .ok()
        .map(|(cols, rows)| PanedLayout::from_size(TerminalSize { cols, rows }));

    // Full clear before the first frame.
    queue!(stdout, Clear(ClearType::All))?;

    loop {
        let next_event = if event::poll(Duration::from_millis(250))? {
            Some(event::read()?)
        } else {
            None
        };

        if !run_terminal_iteration(stdout, feature, surface, next_event, &mut layout)? {
            return Ok(());
        }
    }
}

fn run_terminal_iteration(
    stdout: &mut impl Write,
    feature: &mut FeatureState,
    surface: &mut OperatorSurface,
    next_event: Option<Event>,
    layout: &mut Option<PanedLayout>,
) -> io::Result<bool> {
    // Render: use paned layout when available, fall back to flat text.
    match layout {
        Some(l) => surface.render_paned(stdout, l)?,
        None => {
            queue!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
            write!(stdout, "{}", surface.render())?;
        }
    }
    stdout.flush()?;

    match next_event {
        Some(Event::Resize(cols, rows)) => {
            *layout = Some(PanedLayout::from_size(TerminalSize { cols, rows }));
            queue!(stdout, Clear(ClearType::All))?;
        }
        Some(Event::Key(key_event)) => match surface.handle_key_event(key_event) {
            SurfaceEvent::Continue => {}
            SurfaceEvent::Submitted(follow_up) => {
                let queued = queue_follow_up(feature, follow_up);
                *surface = OperatorSurface::from_feature_state(feature);
                surface.last_event = if queued {
                    "queued follow-up".to_string()
                } else {
                    "no active session for follow-up".to_string()
                };
            }
            SurfaceEvent::ClarificationAnswered { session_id, answer } => {
                answer_clarification(feature, &session_id, answer);
                *surface = OperatorSurface::from_feature_state(feature);
                surface.last_event = "clarification answered".to_string();
            }
            SurfaceEvent::Interrupt => {
                interrupt_active_sessions(feature);
                *surface = OperatorSurface::from_feature_state(feature);
                surface.last_event = "session interrupted".to_string();
            }
            SurfaceEvent::Quit => return Ok(false),
        },
        _ => {}
    }

    Ok(true)
}

#[cfg(not(coverage))]
fn run_terminal_surface_with<W, Setup, Teardown, LoopRunner>(
    stdout: &mut W,
    feature: &mut FeatureState,
    setup: Setup,
    teardown: Teardown,
    loop_runner: LoopRunner,
) -> io::Result<()>
where
    W: Write,
    Setup: FnOnce(&mut W) -> io::Result<()>,
    Teardown: FnOnce(&mut W) -> io::Result<()>,
    LoopRunner: FnOnce(&mut W, &mut FeatureState, &mut OperatorSurface) -> io::Result<()>,
{
    let mut surface = OperatorSurface::from_feature_state(feature);
    setup(stdout)?;
    let loop_result = loop_runner(stdout, feature, &mut surface);
    let teardown_result = teardown(stdout);

    match (loop_result, teardown_result) {
        (Err(error), _) => Err(error),
        (Ok(()), Err(error)) => Err(error),
        (Ok(()), Ok(())) => Ok(()),
    }
}

/// Queue a follow-up message for the first active (running or waiting) session.
/// Returns true if the message was queued, false if no eligible session was found.
pub fn queue_follow_up(feature: &mut FeatureState, follow_up: String) -> bool {
    if let Some(active_session) = feature.active_sessions.iter_mut().find(|session| {
        matches!(
            session.status,
            AgentSessionStatus::Running | AgentSessionStatus::WaitingForHuman
        )
    }) {
        active_session.pending_follow_ups.push(follow_up);
        true
    } else {
        false
    }
}

/// Record an operator answer for a pending clarification.
///
/// Finds the clarification entry for `session_id` that has no answer yet and
/// fills it in. The clarification history on the feature is the source of truth;
/// this does not mutate the session output stream.
pub fn answer_clarification(feature: &mut FeatureState, session_id: &str, answer: String) -> bool {
    if let Some(entry) = feature
        .clarification_history
        .iter_mut()
        .find(|e| e.session_id == session_id && e.answer.is_none())
    {
        entry.answer = Some(answer);
        true
    } else {
        false
    }
}

/// Abort all running or waiting sessions by setting their status to Aborted
/// and recording an Aborted terminal outcome.
pub fn interrupt_active_sessions(feature: &mut FeatureState) {
    for session in feature.active_sessions.iter_mut() {
        if matches!(
            session.status,
            AgentSessionStatus::Running | AgentSessionStatus::WaitingForHuman
        ) {
            session.status = AgentSessionStatus::Aborted;
            session.terminal_outcome = Some(AgentTerminalOutcome::Aborted);
        }
    }
}

/// Extract pending (unanswered) clarifications from the feature's clarification history.
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

fn workflow_label(state: WorkflowState) -> String {
    state.as_str().to_string()
}

/// Render a one-line visual pipeline showing the current position in the workflow.
///
/// Completed states: ✓label   Active state: ●label   Upcoming states: ○label
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

/// Run the interactive operator surface, loading state from a file path.
///
/// This is the entry point for `calypso watch` and `calypso watch --state <path>`.
/// State is persisted back to the file when the operator quits.
pub fn run_watch(state_path: &str) {
    run_watch_with(state_path, run_terminal_surface).unwrap_or_else(|error| {
        eprintln!("watch error: {error}");
        std::process::exit(1);
    });
}

/// Testable inner implementation of `run_watch`.
pub fn run_watch_with<Runner>(state_path: &str, runner: Runner) -> Result<(), String>
where
    Runner: FnOnce(&mut crate::state::FeatureState) -> io::Result<()>,
{
    let mut state = crate::state::RepositoryState::load_from_path(std::path::Path::new(state_path))
        .map_err(|error| error.to_string())?;
    runner(&mut state.current_feature).map_err(|error| error.to_string())?;
    state
        .save_to_path(std::path::Path::new(state_path))
        .map_err(|error| error.to_string())
}

fn evidence_status_label(status: &EvidenceStatus) -> &'static str {
    match status {
        EvidenceStatus::Passing => "passing",
        EvidenceStatus::Failing => "failing",
        EvidenceStatus::Pending => "pending",
        EvidenceStatus::Manual => "manual",
    }
}

// ── Paned layout ─────────────────────────────────────────────────────────────

/// Terminal dimensions, obtained from `crossterm::terminal::size()` or a Resize event.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TerminalSize {
    pub cols: u16,
    pub rows: u16,
}

/// Fixed pane boundaries derived from a `TerminalSize`.
///
/// All coordinates follow the crossterm `MoveTo(col, row)` convention.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PanedLayout {
    /// Total terminal columns.
    pub cols: u16,
    /// Total terminal rows.
    pub rows: u16,
    /// Width of the main (left) pane in columns.
    pub main_width: u16,
    /// Column where the vertical divider `│` sits.
    pub divider_col: u16,
    /// First column of the sidebar pane.
    pub sidebar_col: u16,
    /// Width of the sidebar pane in columns.
    pub sidebar_width: u16,
    /// Number of content rows shared by all panes (excludes input + ribbon rows).
    pub content_rows: u16,
    /// The row reserved for the text input field.
    pub input_row: u16,
    /// The bottom row used for the keybinding ribbon.
    pub ribbon_row: u16,
}

impl PanedLayout {
    /// Compute a layout from terminal dimensions.
    ///
    /// Minimum guaranteed dimensions: 80 columns × 24 rows.
    pub fn from_size(size: TerminalSize) -> Self {
        let cols = size.cols.max(80);
        let rows = size.rows.max(24);
        // Sidebar takes 28 cols on a standard 80-col terminal, up to 34 at wider widths.
        let sidebar_width: u16 = if cols >= 100 { 34 } else { 28 };
        let main_width = cols - sidebar_width - 1; // 1 for the divider column
        let divider_col = main_width;
        let sidebar_col = main_width + 1;
        // Reserve 2 rows at the bottom: input row + ribbon row.
        let ribbon_row = rows - 1;
        let input_row = rows - 2;
        let content_rows = rows - 2;
        Self {
            cols,
            rows,
            main_width,
            divider_col,
            sidebar_col,
            sidebar_width,
            content_rows,
            input_row,
            ribbon_row,
        }
    }
}

/// Write `text` at terminal position `(col, row)`, padded or truncated to exactly `width`
/// visible characters. Padding with spaces clears stale content from a previous frame.
fn write_at(
    stdout: &mut impl Write,
    col: u16,
    row: u16,
    text: &str,
    width: usize,
) -> io::Result<()> {
    queue!(stdout, MoveTo(col, row))?;
    // Count chars (not bytes) so multi-byte Unicode is handled correctly.
    let char_count = text.chars().count();
    if char_count >= width {
        let truncated: String = text.chars().take(width).collect();
        write!(stdout, "{truncated}")
    } else {
        write!(stdout, "{text}{:width$}", "", width = width - char_count)
    }
}

/// Render the vertical divider `│` for all content + input rows.
fn render_vertical_divider(stdout: &mut impl Write, layout: &PanedLayout) -> io::Result<()> {
    for row in 0..layout.rows {
        queue!(stdout, MoveTo(layout.divider_col, row))?;
        write!(stdout, "│")?;
    }
    Ok(())
}

// ── OperatorSurface paned rendering ──────────────────────────────────────────

impl OperatorSurface {
    /// Render the operator surface into a multi-pane layout.
    ///
    /// The caller is responsible for issuing a full `Clear(ClearType::All)` on the first frame
    /// and on resize. Subsequent frames overwrite each cell with padded strings, avoiding flicker.
    pub fn render_paned(&self, stdout: &mut impl Write, layout: &PanedLayout) -> io::Result<()> {
        render_vertical_divider(stdout, layout)?;
        self.render_operator_main_pane(stdout, layout)?;
        self.render_operator_sidebar(stdout, layout)?;
        self.render_operator_input_row(stdout, layout)?;
        render_keybinding_ribbon_operator(stdout, layout, self.sidebar_focused)?;
        Ok(())
    }

    fn render_operator_main_pane(
        &self,
        stdout: &mut impl Write,
        layout: &PanedLayout,
    ) -> io::Result<()> {
        let w = layout.main_width as usize;

        // Build content lines for the main pane.
        let mut lines: Vec<String> = Vec::new();

        // Header box
        let inner = w.saturating_sub(2);
        lines.push(format!("┌{}┐", "─".repeat(w.saturating_sub(2))));
        lines.push(format!(
            "│ {:<width$}│",
            format!("Feature: {}", self.feature_id),
            width = inner.saturating_sub(1)
        ));
        lines.push(format!(
            "│ {:<width$}│",
            format!(
                "Branch: {}   PR: #{}",
                self.branch, self.pull_request_number
            ),
            width = inner.saturating_sub(1)
        ));
        lines.push(format!("└{}┘", "─".repeat(w.saturating_sub(2))));
        lines.push(String::new());

        // Workflow pipeline
        lines.push(render_workflow_pipeline(&self.workflow));
        lines.push(String::new());

        // Status row
        let blocking_str = if self.blocking_gate_ids.is_empty() {
            "none".to_string()
        } else {
            self.blocking_gate_ids.join(", ")
        };
        lines.push(format!(
            "  FU: {}  Blocking: {}  {}",
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
            lines.push(format!("  {}", "─".repeat(w.saturating_sub(4))));
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

        // Sessions — focused view when a session is selected, overview otherwise
        lines.push(String::new());
        match self.selected_session.and_then(|i| self.sessions.get(i)) {
            Some(session) => {
                let status_icon = match session.status.as_str() {
                    "running" => "▶",
                    "completed" => "✓",
                    "failed" => "✗",
                    "aborted" => "⊗",
                    _ => "○",
                };
                lines.push(format!(
                    "  {} {}  ·  {}  ·  [{}]",
                    status_icon, session.role, session.session_id, session.status
                ));
                lines.push(format!("  {}", "─".repeat(w.saturating_sub(4))));
                if session.output.is_empty() {
                    lines.push("    No output yet.".to_string());
                } else {
                    for line in &session.output {
                        lines.push(format!("    {}", line));
                    }
                }
            }
            None => {
                lines.push("  Sessions".to_string());
                lines.push(format!("  {}", "─".repeat(w.saturating_sub(4))));
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
                            lines.push(format!("    {}", output));
                        }
                    }
                }
            }
        }

        // Render with scroll offset
        let available_rows = layout.content_rows as usize;
        let start = self.scroll_offset.min(lines.len().saturating_sub(1));
        for (i, line) in lines.iter().skip(start).enumerate() {
            if i >= available_rows {
                break;
            }
            write_at(stdout, 0, i as u16, line, w)?;
        }
        // Clear any rows below content
        let rendered = lines.len().saturating_sub(start).min(available_rows);
        for i in rendered..available_rows {
            write_at(stdout, 0, i as u16, "", w)?;
        }
        Ok(())
    }

    fn render_operator_sidebar(
        &self,
        stdout: &mut impl Write,
        layout: &PanedLayout,
    ) -> io::Result<()> {
        let col = layout.sidebar_col;
        let w = layout.sidebar_width as usize;
        let content_rows = layout.content_rows as usize;

        let focus_indicator = if self.sidebar_focused { "●" } else { " " };
        let header = format!(
            "┌─{focus_indicator} Sessions {}",
            "─".repeat(w.saturating_sub(12))
        );
        write_at(stdout, col, 0, &format!("{header}┐"), w)?;

        let mut row: usize = 1;

        // Session list
        if self.sessions.is_empty() {
            if row < content_rows.saturating_sub(1) {
                write_at(stdout, col, row as u16, "│  No active sessions", w)?;
                row += 1;
            }
        } else {
            for (i, session) in self.sessions.iter().enumerate() {
                if row >= content_rows.saturating_sub(1) {
                    break;
                }
                let cursor = if self.selected_session == Some(i) { "●" } else { " " };
                let status_icon = match session.status.as_str() {
                    "running" => "▶",
                    "completed" => "✓",
                    "failed" => "✗",
                    "aborted" => "⊗",
                    _ => "○",
                };
                let line = format!("│ {} {} {}", cursor, status_icon, session.role);
                write_at(stdout, col, row as u16, &line, w)?;
                row += 1;
            }
        }

        // Divider before clarifications / follow-ups
        let has_extra =
            !self.pending_clarifications.is_empty() || !self.queued_follow_ups.is_empty();
        if has_extra && row < content_rows.saturating_sub(1) {
            write_at(
                stdout,
                col,
                row as u16,
                &format!("│ {}", "─".repeat(w.saturating_sub(4))),
                w,
            )?;
            row += 1;
        }

        // Pending clarifications
        for clarification in &self.pending_clarifications {
            if row >= content_rows.saturating_sub(1) {
                break;
            }
            let q = format!("│ ? {}", clarification.question);
            write_at(stdout, col, row as u16, &q, w)?;
            row += 1;
        }

        // Queued follow-ups
        for follow_up in &self.queued_follow_ups {
            if row >= content_rows.saturating_sub(1) {
                break;
            }
            let msg = format!("│ > {}", follow_up);
            write_at(stdout, col, row as u16, &msg, w)?;
            row += 1;
        }

        // Fill empty rows
        while row < content_rows.saturating_sub(1) {
            write_at(stdout, col, row as u16, "│", w)?;
            row += 1;
        }

        // Footer
        if row < content_rows {
            write_at(
                stdout,
                col,
                row as u16,
                &format!("└{}┘", "─".repeat(w.saturating_sub(2))),
                w,
            )?;
        }

        Ok(())
    }

    fn render_operator_input_row(
        &self,
        stdout: &mut impl Write,
        layout: &PanedLayout,
    ) -> io::Result<()> {
        let prompt = if !self.pending_clarifications.is_empty() {
            format!("  Answer: {}▌", self.input.as_str())
        } else {
            format!("  Input:  {}▌", self.input.as_str())
        };
        write_at(stdout, 0, layout.input_row, &prompt, layout.cols as usize)
    }
}

fn render_keybinding_ribbon_operator(
    stdout: &mut impl Write,
    layout: &PanedLayout,
    sidebar_focused: bool,
) -> io::Result<()> {
    let (focus_hint, navigate_hint) = if sidebar_focused {
        ("[Tab] Main", "[↑/↓] Select session")
    } else {
        ("[Tab] Sessions", "[↑/↓] Scroll")
    };
    let ribbon = format!(" [Ctrl+C] Interrupt  [Esc] Quit  {focus_hint}  {navigate_hint}");
    write_at(stdout, 0, layout.ribbon_row, &ribbon, layout.cols as usize)
}

// ── DoctorSurface paned rendering ────────────────────────────────────────────

impl DoctorSurface {
    /// Render the doctor surface as a single full-width panel.
    ///
    /// The check list is followed inline by the detail section for the selected check.
    /// No vertical divider or sidebar — the detail travels with the list.
    pub fn render_paned(&self, stdout: &mut impl Write, layout: &PanedLayout) -> io::Result<()> {
        let w = layout.cols as usize;
        let content_rows = layout.content_rows; // u16 — used as row boundary throughout

        let passing = self
            .checks
            .iter()
            .filter(|c| matches!(c.status, DoctorStatus::Passing))
            .count();
        let total = self.checks.len();

        // Header
        write_at(
            stdout,
            0,
            0,
            &format!("┌{}┐", "─".repeat(w.saturating_sub(2))),
            w,
        )?;
        write_at(
            stdout,
            0,
            1,
            &format!("│  Calypso Doctor  {passing}/{total} passing"),
            w,
        )?;
        write_at(
            stdout,
            0,
            2,
            &format!("└{}┘", "─".repeat(w.saturating_sub(2))),
            w,
        )?;
        write_at(stdout, 0, 3, "", w)?;

        // Check list
        let mut row: u16 = 4;
        for (index, check) in self.checks.iter().enumerate() {
            if row >= content_rows {
                break;
            }
            let pointer = if index == self.selected { "▶" } else { " " };
            let status_icon = if matches!(check.status, DoctorStatus::Passing) {
                "✓"
            } else {
                "✗"
            };
            let fix_tag = if check.has_auto_fix() {
                "  [auto-fix]"
            } else {
                ""
            };
            write_at(
                stdout,
                0,
                row,
                &format!("  {pointer} {status_icon}  {}{fix_tag}", check.id),
                w,
            )?;
            row += 1;
        }

        // Inline detail for the selected check
        if let Some(check) = self.checks.get(self.selected) {
            if row + 1 < content_rows {
                write_at(stdout, 0, row, "", w)?;
                row += 1;
            }
            if row < content_rows {
                write_at(
                    stdout,
                    0,
                    row,
                    &format!("  {}", "─".repeat(w.saturating_sub(4))),
                    w,
                )?;
                row += 1;
            }
            if row < content_rows {
                let status_icon = if matches!(check.status, DoctorStatus::Passing) {
                    "✓"
                } else {
                    "✗"
                };
                write_at(stdout, 0, row, &format!("  {status_icon}  {}", check.id), w)?;
                row += 1;
            }
            if let Some(detail) = &check.detail
                && row < content_rows
            {
                write_at(stdout, 0, row, &format!("     Detail: {detail}"), w)?;
                row += 1;
            }
            if let Some(remediation) = &check.remediation
                && row < content_rows
            {
                write_at(stdout, 0, row, &format!("     Fix: {remediation}"), w)?;
                row += 1;
            }
            if let Some(output) = &self.fix_output
                && row < content_rows
            {
                write_at(stdout, 0, row, &format!("     Output: {output}"), w)?;
                row += 1;
            }
        }

        // Clear remaining rows
        while row < content_rows {
            write_at(stdout, 0, row, "", w)?;
            row += 1;
        }

        Ok(())
    }
}

// ── Doctor TUI surface ────────────────────────────────────────────────────────

use crate::app::resolve_repo_root;
use crate::doctor::HostDoctorEnvironment;
use crate::doctor::{DoctorFix, DoctorStatus, apply_fix, collect_doctor_report};

/// A view-model for a single doctor check rendered in the doctor TUI surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DoctorCheckView {
    pub id: String,
    pub status: DoctorStatus,
    pub detail: Option<String>,
    pub remediation: Option<String>,
    pub fix: Option<DoctorFix>,
}

impl DoctorCheckView {
    pub fn has_auto_fix(&self) -> bool {
        self.fix.as_ref().is_some_and(DoctorFix::is_automatic)
    }
}

/// A self-contained TUI surface for running and displaying doctor checks.
#[derive(Debug)]
pub struct DoctorSurface {
    checks: Vec<DoctorCheckView>,
    selected: usize,
    last_refresh: std::time::Instant,
    fix_output: Option<String>,
    cwd: std::path::PathBuf,
}

impl DoctorSurface {
    /// Create a new `DoctorSurface` from a slice of check views and the working directory.
    pub fn new(checks: Vec<DoctorCheckView>, cwd: std::path::PathBuf) -> Self {
        Self {
            checks,
            selected: 0,
            last_refresh: std::time::Instant::now(),
            fix_output: None,
            cwd,
        }
    }

    /// Reload checks from the surface's working directory.
    pub fn refresh(&mut self, cwd: &std::path::Path) {
        let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
        let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);
        self.checks = doctor_check_views_from_report(&report);
        self.cwd = cwd.to_path_buf();
        self.last_refresh = std::time::Instant::now();
        self.fix_output = None;
        if self.selected >= self.checks.len() {
            self.selected = self.checks.len().saturating_sub(1);
        }
    }

    /// Render the surface to a plain-text string.
    pub fn render(&self) -> String {
        let passing = self
            .checks
            .iter()
            .filter(|c| matches!(c.status, DoctorStatus::Passing))
            .count();
        let total = self.checks.len();

        let mut lines = vec![
            "┌─ Calypso Doctor ───────────────────────────────────────────────────────────┐"
                .to_string(),
            format!(
                "│ Environment diagnostics  {passing}/{total} passing{:<48} │",
                ""
            ),
            "└────────────────────────────────────────────────────────────────────────────┘"
                .to_string(),
            String::new(),
        ];

        for (index, check) in self.checks.iter().enumerate() {
            let pointer = if index == self.selected { "▶" } else { " " };
            let status_icon = if matches!(check.status, DoctorStatus::Passing) {
                "✓"
            } else {
                "✗"
            };
            let fix_tag = if check.has_auto_fix() {
                "  [auto-fix]"
            } else {
                ""
            };
            lines.push(format!("  {pointer} {status_icon}  {}{fix_tag}", check.id));
        }

        if let Some(selected_check) = self.checks.get(self.selected) {
            lines.push(String::new());
            lines.push(
                "  ─────────────────────────────────────────────────────────────────────────"
                    .to_string(),
            );
            let status_icon = if matches!(selected_check.status, DoctorStatus::Passing) {
                "✓"
            } else {
                "✗"
            };
            lines.push(format!("  Selected: {} {status_icon}", selected_check.id));
            if let Some(detail) = &selected_check.detail {
                lines.push(format!("     Detail: {detail}"));
            }
            if let Some(remediation) = &selected_check.remediation {
                lines.push(format!("     Fix: {remediation}"));
            }
        }

        if let Some(output) = &self.fix_output {
            lines.push(String::new());
            lines.push(format!("  Fix output: {output}"));
        }

        lines.push(String::new());
        lines.push(
            "  ─────────────────────────────────────────────────────────────────────────"
                .to_string(),
        );
        lines.push("  [↑/↓] Select  [f] Apply fix  [r] Refresh  [q/Esc] Quit".to_string());

        lines.join("\n")
    }

    /// Handle a key event, returning a `DoctorSurfaceEvent`.
    pub fn handle_key_event(
        &mut self,
        event: crossterm::event::KeyEvent,
        cwd: &std::path::Path,
    ) -> DoctorSurfaceEvent {
        use crossterm::event::KeyCode;

        if event.code == KeyCode::Char('c')
            && event
                .modifiers
                .contains(crossterm::event::KeyModifiers::CONTROL)
        {
            return DoctorSurfaceEvent::Quit;
        }

        match event.code {
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                }
                DoctorSurfaceEvent::Continue
            }
            KeyCode::Down => {
                if self.selected + 1 < self.checks.len() {
                    self.selected += 1;
                }
                DoctorSurfaceEvent::Continue
            }
            KeyCode::Char('r') => {
                self.refresh(cwd);
                DoctorSurfaceEvent::Continue
            }
            KeyCode::Char('f') => {
                self.apply_selected_fix();
                DoctorSurfaceEvent::Continue
            }
            KeyCode::Char('q') | KeyCode::Esc => DoctorSurfaceEvent::Quit,
            _ => DoctorSurfaceEvent::Continue,
        }
    }

    fn apply_selected_fix(&mut self) {
        if let Some(check) = self.checks.get(self.selected).cloned() {
            match &check.fix {
                None | Some(DoctorFix::Manual { .. }) => {
                    self.fix_output = check
                        .remediation
                        .clone()
                        .or_else(|| Some("No automated fix available.".to_string()));
                }
                Some(fix) => {
                    let cwd = self.cwd.clone();
                    self.fix_output = Some(match apply_fix(fix, &cwd) {
                        Ok(output) => output,
                        Err(error) => format!("Error: {error}"),
                    });
                }
            }
        }
    }

    /// Return the index of the currently selected check.
    pub fn selected(&self) -> usize {
        self.selected
    }

    /// Return the number of checks in the surface.
    pub fn check_count(&self) -> usize {
        self.checks.len()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DoctorSurfaceEvent {
    Continue,
    Quit,
}

fn doctor_check_views_from_report(report: &crate::doctor::DoctorReport) -> Vec<DoctorCheckView> {
    report
        .checks
        .iter()
        .map(|check| DoctorCheckView {
            id: check.id.label().to_string(),
            status: check.status,
            detail: check.detail.clone(),
            remediation: check.remediation.clone(),
            fix: check.fix.clone(),
        })
        .collect()
}

// ── State Machine TUI surface ─────────────────────────────────────────────────

/// The ordered feature lifecycle pipeline steps (excludes side states Blocked/Aborted).
fn sm_pipeline() -> [WorkflowState; 9] {
    [
        WorkflowState::New,
        WorkflowState::PrdReview,
        WorkflowState::ArchitecturePlan,
        WorkflowState::ScaffoldTdd,
        WorkflowState::ArchitectureReview,
        WorkflowState::Implementation,
        WorkflowState::QaValidation,
        WorkflowState::ReleaseReady,
        WorkflowState::Done,
    ]
}

fn sm_step_label(state: &WorkflowState) -> &'static str {
    match state {
        WorkflowState::New => "New",
        WorkflowState::PrdReview => "PRD Review",
        WorkflowState::ArchitecturePlan => "Architecture Plan",
        WorkflowState::ScaffoldTdd => "Scaffold TDD",
        WorkflowState::ArchitectureReview => "Architecture Review",
        WorkflowState::Implementation => "Implementation",
        WorkflowState::QaValidation => "QA Validation",
        WorkflowState::ReleaseReady => "Release Ready",
        WorkflowState::Done => "Done",
        WorkflowState::Blocked => "Blocked",
        WorkflowState::Aborted => "Aborted",
        WorkflowState::WaitingForHuman => "Implementation",
        WorkflowState::ReadyForReview => "Release Ready",
    }
}

/// Status of a node in the state machine tree.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SmStatus {
    Pending,
    Active,
    Done,
    Failed,
    Manual,
    Blocked,
}

impl SmStatus {
    fn icon(self) -> &'static str {
        match self {
            Self::Pending => "○",
            Self::Active => "●",
            Self::Done => "✓",
            Self::Failed => "✗",
            Self::Manual => "◆",
            Self::Blocked => "⚠",
        }
    }
}

/// Identity of a node in the tree, used for expand/collapse operations.
#[derive(Debug, Clone, PartialEq, Eq)]
enum SmNodeId {
    PipelineStep(usize),
    GateGroup(usize),
    Gate { group: usize, gate: usize },
}

/// A flat visible row in the rendered state machine tree.
#[derive(Debug, Clone)]
struct SmRow {
    node_id: SmNodeId,
    depth: usize,
    label: String,
    status: SmStatus,
    is_expandable: bool,
    is_expanded: bool,
    /// Number of concurrent activities (> 1 shows "N -").
    activity_count: usize,
    /// Running agent session ID, if any (triggers "*" indicator).
    agent_session_id: Option<String>,
}

#[derive(Debug, Clone)]
struct SmGateGroup {
    label: String,
    status: SmStatus,
    gates: Vec<SmGate>,
    /// Number of pending gates (= concurrent CI-style activities in progress).
    pending_count: usize,
}

#[derive(Debug, Clone)]
struct SmGate {
    label: String,
    status: SmStatus,
}

#[derive(Debug, Clone)]
struct SmSessionSnap {
    session_id: String,
    is_running: bool,
}

/// Events emitted by the state machine surface.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SmEvent {
    Continue,
    Quit,
    /// Switch to the Agents tab, optionally pre-selecting a session.
    JumpToAgents(Option<String>),
}

/// Interactive state machine tree view with collapsible sub-state-machines.
///
/// The top-level rows are the feature lifecycle pipeline steps. Each active step can
/// be expanded (Enter) to reveal its gate groups (sub-state-machines). Gate groups
/// can in turn be expanded to show individual gates. Only one sub-state-machine may
/// be open at a time at each nesting level; Esc collapses from the inside out.
pub struct StateMachineSurface {
    workflow_state: WorkflowState,
    gate_groups: Vec<SmGateGroup>,
    sessions: Vec<SmSessionSnap>,
    /// Which pipeline step index is currently expanded (one at a time).
    expanded_step: Option<usize>,
    /// Which gate group index is expanded within the expanded step (one at a time).
    expanded_gate_group: Option<usize>,
    /// Cursor position (index into the visible row list).
    selected: usize,
    /// Scroll offset: index of the first visible row.
    scroll: usize,
}

impl Default for StateMachineSurface {
    fn default() -> Self {
        Self::new()
    }
}

impl StateMachineSurface {
    /// Create an empty surface (no feature loaded).
    pub fn new() -> Self {
        Self {
            workflow_state: WorkflowState::New,
            gate_groups: Vec::new(),
            sessions: Vec::new(),
            expanded_step: None,
            expanded_gate_group: None,
            selected: 0,
            scroll: 0,
        }
    }

    /// Build the surface from a loaded feature state.
    pub fn from_feature_state(feature: &FeatureState) -> Self {
        let gate_groups = feature
            .gate_groups
            .iter()
            .map(|group| {
                let pending_count = group
                    .gates
                    .iter()
                    .filter(|g| g.status == GateStatus::Pending)
                    .count();
                SmGateGroup {
                    label: group.label.clone(),
                    status: sm_gate_group_status(group.rollup_status()),
                    gates: group
                        .gates
                        .iter()
                        .map(|gate| SmGate {
                            label: gate.label.clone(),
                            status: sm_gate_status(gate.status.clone()),
                        })
                        .collect(),
                    pending_count,
                }
            })
            .collect();

        let sessions = feature
            .active_sessions
            .iter()
            .map(|s| SmSessionSnap {
                session_id: s.session_id.clone(),
                is_running: matches!(
                    s.status,
                    AgentSessionStatus::Running | AgentSessionStatus::WaitingForHuman
                ),
            })
            .collect();

        // Normalise deprecated variant aliases for pipeline position lookup.
        let canonical = match &feature.workflow_state {
            WorkflowState::WaitingForHuman => WorkflowState::Implementation,
            WorkflowState::ReadyForReview => WorkflowState::ReleaseReady,
            other => other.clone(),
        };
        let pipeline = sm_pipeline();
        let current_step_idx = pipeline.iter().position(|s| *s == canonical);

        // Auto-expand the active step when gate groups are present.
        let expanded_step = if feature.gate_groups.is_empty() {
            None
        } else {
            current_step_idx
        };

        let mut surface = Self {
            workflow_state: feature.workflow_state.clone(),
            gate_groups,
            sessions,
            expanded_step,
            expanded_gate_group: None,
            selected: 0,
            scroll: 0,
        };

        // Place cursor on the active pipeline step.
        if let Some(idx) = current_step_idx {
            let rows = surface.visible_rows();
            surface.selected = rows
                .iter()
                .position(|r| r.node_id == SmNodeId::PipelineStep(idx))
                .unwrap_or(0);
        }

        surface
    }

    /// Build the flat visible row list, reflecting the current expand/collapse state.
    fn visible_rows(&self) -> Vec<SmRow> {
        let pipeline = sm_pipeline();
        let canonical = match &self.workflow_state {
            WorkflowState::WaitingForHuman => WorkflowState::Implementation,
            WorkflowState::ReadyForReview => WorkflowState::ReleaseReady,
            other => other.clone(),
        };
        let is_side_state = matches!(
            self.workflow_state,
            WorkflowState::Blocked | WorkflowState::Aborted
        );
        let current_idx = if is_side_state {
            None
        } else {
            pipeline.iter().position(|s| *s == canonical)
        };

        let mut rows: Vec<SmRow> = Vec::new();

        for (i, step) in pipeline.iter().enumerate() {
            let is_current = current_idx == Some(i);
            let is_before = current_idx.is_some_and(|pos| i < pos);

            let status = if is_before {
                SmStatus::Done
            } else if is_current {
                SmStatus::Active
            } else {
                SmStatus::Pending
            };

            let has_children = is_current && !self.gate_groups.is_empty();
            let is_expanded = self.expanded_step == Some(i);

            let running: Vec<&SmSessionSnap> = if is_current {
                self.sessions.iter().filter(|s| s.is_running).collect()
            } else {
                Vec::new()
            };

            rows.push(SmRow {
                node_id: SmNodeId::PipelineStep(i),
                depth: 0,
                label: sm_step_label(step).to_string(),
                status,
                is_expandable: has_children,
                is_expanded,
                activity_count: running.len(),
                agent_session_id: running.first().map(|s| s.session_id.clone()),
            });

            if is_expanded {
                for (gi, group) in self.gate_groups.iter().enumerate() {
                    let group_expanded = self.expanded_gate_group == Some(gi);
                    rows.push(SmRow {
                        node_id: SmNodeId::GateGroup(gi),
                        depth: 1,
                        label: group.label.clone(),
                        status: group.status,
                        is_expandable: !group.gates.is_empty(),
                        is_expanded: group_expanded,
                        activity_count: group.pending_count,
                        agent_session_id: None,
                    });

                    if group_expanded {
                        for (ki, gate) in group.gates.iter().enumerate() {
                            rows.push(SmRow {
                                node_id: SmNodeId::Gate {
                                    group: gi,
                                    gate: ki,
                                },
                                depth: 2,
                                label: gate.label.clone(),
                                status: gate.status,
                                is_expandable: false,
                                is_expanded: false,
                                activity_count: 0,
                                agent_session_id: None,
                            });
                        }
                    }
                }
            }
        }

        // Append side-state rows when the feature is blocked or aborted.
        if matches!(self.workflow_state, WorkflowState::Blocked) {
            rows.push(SmRow {
                node_id: SmNodeId::PipelineStep(pipeline.len()),
                depth: 0,
                label: "Blocked".to_string(),
                status: SmStatus::Blocked,
                is_expandable: false,
                is_expanded: false,
                activity_count: 0,
                agent_session_id: None,
            });
        }
        if matches!(self.workflow_state, WorkflowState::Aborted) {
            rows.push(SmRow {
                node_id: SmNodeId::PipelineStep(pipeline.len() + 1),
                depth: 0,
                label: "Aborted".to_string(),
                status: SmStatus::Failed,
                is_expandable: false,
                is_expanded: false,
                activity_count: 0,
                agent_session_id: None,
            });
        }

        rows
    }

    /// Render the surface into the full terminal width of the paned layout.
    pub fn render_paned(&self, stdout: &mut impl Write, layout: &PanedLayout) -> io::Result<()> {
        const HEADER_ROWS: u16 = 4;
        let w = layout.cols as usize;
        let content_rows = layout.content_rows;
        let viewport_height = content_rows.saturating_sub(HEADER_ROWS) as usize;

        write_at(
            stdout,
            0,
            0,
            &format!("┌{}┐", "─".repeat(w.saturating_sub(2))),
            w,
        )?;
        write_at(stdout, 0, 1, "│  State Machine", w)?;
        write_at(
            stdout,
            0,
            2,
            &format!("└{}┘", "─".repeat(w.saturating_sub(2))),
            w,
        )?;
        write_at(stdout, 0, 3, "", w)?;

        let rows = self.visible_rows();
        let visible_start = self.scroll.min(rows.len().saturating_sub(1));

        let mut render_row: u16 = HEADER_ROWS;
        for (list_idx, sm_row) in rows.iter().enumerate() {
            if render_row >= content_rows {
                break;
            }
            if list_idx < visible_start {
                continue;
            }
            if list_idx >= visible_start + viewport_height {
                break;
            }

            let cursor = if list_idx == self.selected {
                "▶"
            } else {
                " "
            };
            let expand_icon = if sm_row.is_expandable {
                if sm_row.is_expanded { "▾" } else { "▸" }
            } else {
                " "
            };
            let indent: String = "  ".repeat(sm_row.depth);

            // Activity indicator: `N *` for agentic (with count), `*` for single agent,
            // `N -` for N concurrent non-agentic activities.
            let activity = if sm_row.agent_session_id.is_some() {
                if sm_row.activity_count > 1 {
                    format!("  {} *", sm_row.activity_count)
                } else {
                    "  *".to_string()
                }
            } else if sm_row.activity_count > 1 {
                format!("  {} -", sm_row.activity_count)
            } else {
                String::new()
            };

            let line = format!(
                "  {} {} {} {}{}{}",
                cursor,
                expand_icon,
                sm_row.status.icon(),
                indent,
                sm_row.label,
                activity,
            );
            write_at(stdout, 0, render_row, &line, w)?;
            render_row += 1;
        }

        while render_row < content_rows {
            write_at(stdout, 0, render_row, "", w)?;
            render_row += 1;
        }

        Ok(())
    }

    /// Handle a key event, returning an `SmEvent`.
    pub fn handle_key_event(&mut self, event: KeyEvent) -> SmEvent {
        let rows = self.visible_rows();
        let row_count = rows.len();

        match event.code {
            KeyCode::Up => {
                if self.selected > 0 {
                    self.selected -= 1;
                    self.adjust_scroll(row_count);
                }
                SmEvent::Continue
            }
            KeyCode::Down => {
                if self.selected + 1 < row_count {
                    self.selected += 1;
                    self.adjust_scroll(row_count);
                }
                SmEvent::Continue
            }
            KeyCode::Enter => {
                // Expand the selected row if it has children and is not yet open.
                if let Some(row) = rows.get(self.selected).cloned()
                    && row.is_expandable
                    && !row.is_expanded
                {
                    self.expand_node(row.node_id);
                }
                SmEvent::Continue
            }
            KeyCode::Esc => {
                // Collapse from innermost outward; quit when nothing remains open.
                if self.expanded_gate_group.is_some() {
                    self.expanded_gate_group = None;
                    SmEvent::Continue
                } else if self.expanded_step.is_some() {
                    self.expanded_step = None;
                    SmEvent::Continue
                } else {
                    SmEvent::Quit
                }
            }
            KeyCode::Char('a') => {
                // Jump to the Agents tab, carrying the session ID if one is active here.
                let session_id = rows
                    .get(self.selected)
                    .and_then(|r| r.agent_session_id.clone());
                SmEvent::JumpToAgents(session_id)
            }
            KeyCode::Char('q') => SmEvent::Quit,
            _ => SmEvent::Continue,
        }
    }

    fn expand_node(&mut self, node_id: SmNodeId) {
        match node_id {
            SmNodeId::PipelineStep(i) => {
                // Opening a different step closes any previously open gate group.
                self.expanded_step = Some(i);
                self.expanded_gate_group = None;
            }
            SmNodeId::GateGroup(gi) => {
                self.expanded_gate_group = Some(gi);
            }
            SmNodeId::Gate { .. } => {}
        }
    }

    /// Adjust the scroll offset to keep the cursor within the visible viewport.
    fn adjust_scroll(&mut self, total_rows: usize) {
        // Use a conservative viewport estimate; real size comes from the layout at render time.
        const VIEWPORT: usize = 15;
        if self.selected < self.scroll {
            self.scroll = self.selected;
        } else if self.selected >= self.scroll + VIEWPORT {
            self.scroll = self.selected + 1 - VIEWPORT;
        }
        if total_rows > VIEWPORT && self.scroll + VIEWPORT > total_rows {
            self.scroll = total_rows - VIEWPORT;
        }
    }
}

fn sm_gate_status(status: GateStatus) -> SmStatus {
    match status {
        GateStatus::Pending => SmStatus::Pending,
        GateStatus::Passing => SmStatus::Done,
        GateStatus::Failing => SmStatus::Failed,
        GateStatus::Manual => SmStatus::Manual,
    }
}

fn sm_gate_group_status(status: GateGroupStatus) -> SmStatus {
    match status {
        GateGroupStatus::Passing => SmStatus::Done,
        GateGroupStatus::Pending => SmStatus::Pending,
        GateGroupStatus::Manual => SmStatus::Manual,
        GateGroupStatus::Blocked => SmStatus::Failed,
    }
}

// ── App shell (three-tab TUI) ─────────────────────────────────────────────────

/// The three top-level screens reachable with Left/Right arrow keys.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppTab {
    Doctor,
    StateMachine,
    Agents,
}

impl AppTab {
    const ALL: [AppTab; 3] = [AppTab::Doctor, AppTab::StateMachine, AppTab::Agents];

    fn label(self) -> &'static str {
        match self {
            AppTab::Doctor => "Doctor",
            AppTab::StateMachine => "State Machine",
            AppTab::Agents => "Agents",
        }
    }

    /// Context-sensitive keybinding hints shown in the ribbon for this tab.
    fn screen_hints(self) -> &'static str {
        match self {
            AppTab::Doctor => "[↑/↓] Select  [f] Fix  [r] Refresh",
            AppTab::StateMachine => "[↑/↓] Navigate  [Enter] Expand  [Esc] Collapse  [a] Agent",
            AppTab::Agents => "[Tab] Sessions  [↑/↓] Navigate  [Ctrl+C] Interrupt",
        }
    }

    fn next(self) -> Self {
        match self {
            AppTab::Doctor => AppTab::StateMachine,
            AppTab::StateMachine => AppTab::Agents,
            AppTab::Agents => AppTab::Agents,
        }
    }

    fn prev(self) -> Self {
        match self {
            AppTab::Doctor => AppTab::Doctor,
            AppTab::StateMachine => AppTab::Doctor,
            AppTab::Agents => AppTab::StateMachine,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppEvent {
    Continue,
    Quit,
    /// A follow-up message was submitted from the Agents tab.
    FollowUpSubmitted(String),
    /// A clarification question was answered from the Agents tab.
    ClarificationAnswered { session_id: String, answer: String },
    /// The operator requested interruption of all active sessions.
    Interrupted,
}

/// Top-level TUI shell with three navigable screens.
///
/// Left/Right arrows switch tabs. All other keys are forwarded to the active screen.
pub struct AppShell {
    pub tab: AppTab,
    pub doctor: DoctorSurface,
    /// State machine surface (always present; populated from feature state when available).
    pub sm: StateMachineSurface,
    /// Operator surface used for the Agents tab (absent when no feature is active).
    pub operator: Option<OperatorSurface>,
}

impl AppShell {
    pub fn new(doctor: DoctorSurface) -> Self {
        Self {
            tab: AppTab::Doctor,
            doctor,
            sm: StateMachineSurface::new(),
            operator: None,
        }
    }

    pub fn with_sm(mut self, sm: StateMachineSurface) -> Self {
        self.sm = sm;
        self
    }

    pub fn with_operator(mut self, surface: OperatorSurface) -> Self {
        self.operator = Some(surface);
        self
    }

    pub fn handle_key_event(&mut self, event: KeyEvent, cwd: &std::path::Path) -> AppEvent {
        // Ctrl+C always exits at the app level.
        if event.code == KeyCode::Char('c') && event.modifiers.contains(KeyModifiers::CONTROL) {
            return AppEvent::Quit;
        }
        // Left/Right switch tabs without forwarding to the active screen.
        match event.code {
            KeyCode::Left => {
                self.tab = self.tab.prev();
                return AppEvent::Continue;
            }
            KeyCode::Right => {
                self.tab = self.tab.next();
                return AppEvent::Continue;
            }
            _ => {}
        }
        // Delegate remaining keys to the active screen.
        match self.tab {
            AppTab::Doctor => match self.doctor.handle_key_event(event, cwd) {
                DoctorSurfaceEvent::Continue => AppEvent::Continue,
                DoctorSurfaceEvent::Quit => AppEvent::Quit,
            },
            AppTab::StateMachine => match self.sm.handle_key_event(event) {
                SmEvent::Continue => AppEvent::Continue,
                SmEvent::Quit => AppEvent::Quit,
                SmEvent::JumpToAgents(session_id) => {
                    self.tab = AppTab::Agents;
                    if let (Some(id), Some(op)) = (session_id, &mut self.operator) {
                        op.focus_session(&id);
                    }
                    AppEvent::Continue
                }
            },
            AppTab::Agents => {
                if let Some(op) = &mut self.operator {
                    match op.handle_key_event(event) {
                        SurfaceEvent::Continue => AppEvent::Continue,
                        SurfaceEvent::Quit => AppEvent::Quit,
                        SurfaceEvent::Submitted(text) => AppEvent::FollowUpSubmitted(text),
                        SurfaceEvent::ClarificationAnswered { session_id, answer } => {
                            AppEvent::ClarificationAnswered { session_id, answer }
                        }
                        SurfaceEvent::Interrupt => AppEvent::Interrupted,
                    }
                } else {
                    placeholder_key_event(event)
                }
            }
        }
    }

    /// Render the active screen then overwrite the ribbon row with the app-level ribbon.
    pub fn render_paned(&self, stdout: &mut impl Write, layout: &PanedLayout) -> io::Result<()> {
        match self.tab {
            AppTab::Doctor => self.doctor.render_paned(stdout, layout)?,
            AppTab::StateMachine => self.sm.render_paned(stdout, layout)?,
            AppTab::Agents => {
                if let Some(op) = &self.operator {
                    op.render_paned(stdout, layout)?;
                } else {
                    render_agents_scaffold(stdout, layout)?;
                }
            }
        }
        // Always overwrite the ribbon row last so the app-level tab bar wins.
        render_app_ribbon(stdout, layout, self.tab)?;
        Ok(())
    }
}

/// Bottom ribbon showing the three tabs (active tab highlighted) and context-sensitive hints.
fn render_app_ribbon(
    stdout: &mut impl Write,
    layout: &PanedLayout,
    active: AppTab,
) -> io::Result<()> {
    let mut tabs = String::new();
    for (i, tab) in AppTab::ALL.iter().enumerate() {
        if i > 0 {
            tabs.push_str("  ");
        }
        tabs.push_str(if *tab == active { "◆" } else { "○" });
        tabs.push(' ');
        tabs.push_str(tab.label());
    }
    let ribbon = format!(
        "  {tabs}    {}  [←/→] Switch  [Esc] Quit",
        active.screen_hints()
    );
    write_at(stdout, 0, layout.ribbon_row, &ribbon, layout.cols as usize)
}

/// Key handler for scaffold placeholder screens: only Esc/q quit, everything else is ignored.
fn placeholder_key_event(event: KeyEvent) -> AppEvent {
    match event.code {
        KeyCode::Esc | KeyCode::Char('q') => AppEvent::Quit,
        _ => AppEvent::Continue,
    }
}

/// Render a scaffold placeholder screen: header box, one body line, cleared remaining rows.
fn render_placeholder_screen(
    stdout: &mut impl Write,
    layout: &PanedLayout,
    title: &str,
    body: &str,
) -> io::Result<()> {
    let w = layout.cols as usize;
    write_at(
        stdout,
        0,
        0,
        &format!("┌{}┐", "─".repeat(w.saturating_sub(2))),
        w,
    )?;
    write_at(stdout, 0, 1, &format!("│  {title}"), w)?;
    write_at(
        stdout,
        0,
        2,
        &format!("└{}┘", "─".repeat(w.saturating_sub(2))),
        w,
    )?;
    write_at(stdout, 0, 3, "", w)?;
    write_at(stdout, 0, 4, &format!("  {body}"), w)?;
    for row in 5..layout.content_rows {
        write_at(stdout, 0, row, "", w)?;
    }
    Ok(())
}

fn render_agents_scaffold(stdout: &mut impl Write, layout: &PanedLayout) -> io::Result<()> {
    render_placeholder_screen(
        stdout,
        layout,
        "Active Agents",
        "No active feature — run calypso doctor to initialize.",
    )
}

/// Run the interactive app shell (three-tab TUI) from the given working directory.
///
/// This is the entry point for the default `calypso` invocation when no state file is present.
#[cfg(not(coverage))]
pub fn run_doctor_surface(cwd: &std::path::Path) -> io::Result<()> {
    use crossterm::cursor::{Hide, Show};
    use crossterm::execute;
    use crossterm::terminal::{
        EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
    };
    use std::time::Duration;

    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);
    let doctor = DoctorSurface::new(doctor_check_views_from_report(&report), cwd.to_path_buf());
    let mut shell = AppShell::new(doctor);

    let mut stdout = io::stdout();
    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let mut layout: Option<PanedLayout> = crossterm::terminal::size()
        .ok()
        .map(|(cols, rows)| PanedLayout::from_size(TerminalSize { cols, rows }));
    queue!(stdout, Clear(ClearType::All))?;

    let loop_result = loop {
        match &layout {
            Some(l) => shell.render_paned(&mut stdout, l)?,
            None => {
                queue!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
                write!(stdout, "{}", shell.doctor.render())?;
            }
        }
        stdout.flush()?;

        if crossterm::event::poll(Duration::from_millis(250))? {
            match crossterm::event::read()? {
                crossterm::event::Event::Resize(cols, rows) => {
                    layout = Some(PanedLayout::from_size(TerminalSize { cols, rows }));
                    queue!(stdout, Clear(ClearType::All))?;
                }
                crossterm::event::Event::Key(key_event) => {
                    if matches!(shell.handle_key_event(key_event, cwd), AppEvent::Quit) {
                        break Ok(());
                    }
                }
                _ => {}
            }
        }
    };

    execute!(stdout, Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    loop_result
}

#[cfg(coverage)]
pub fn run_doctor_surface(cwd: &std::path::Path) -> io::Result<()> {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);
    let doctor = DoctorSurface::new(doctor_check_views_from_report(&report), cwd.to_path_buf());
    let mut shell = AppShell::new(doctor);

    let mut stdout = io::sink();
    let mut layout: Option<PanedLayout> = None;

    // Exercise all three tabs and screen-specific keys. Resize first activates paned rendering.
    let events = vec![
        Some(crossterm::event::Event::Resize(80, 24)),
        // Navigate to State Machine and render the new SM surface.
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Right))),
        // Exercise SM navigation: Down / Up / Enter (no-op on leaf) / 'a' (jump to Agents).
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Down))),
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Up))),
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Enter))),
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Char(
            'a',
        )))),
        // 'a' switched to Agents tab; navigate back to SM then back to Doctor.
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Left))),
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Left))),
        // Exercise doctor-specific keys while on Doctor tab.
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Down))),
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Up))),
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Char(
            'f',
        )))),
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Char(
            'r',
        )))),
        Some(crossterm::event::Event::Key(KeyEvent::from(KeyCode::Esc))),
    ];

    for event in events {
        match &layout {
            Some(l) => shell.render_paned(&mut stdout, l)?,
            None => {
                queue!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
                write!(stdout, "{}", shell.doctor.render())?;
            }
        }
        stdout.flush()?;

        match event {
            Some(crossterm::event::Event::Resize(cols, rows)) => {
                layout = Some(PanedLayout::from_size(TerminalSize { cols, rows }));
            }
            Some(crossterm::event::Event::Key(key_event)) => {
                if matches!(shell.handle_key_event(key_event, cwd), AppEvent::Quit) {
                    break;
                }
            }
            _ => {}
        }
    }

    // Exercise doctor's own Ctrl+C handler (shell intercepts it before delegation normally).
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    shell.doctor.handle_key_event(ctrl_c, cwd);
    // Exercise shell-level Ctrl+C.
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    shell.handle_key_event(ctrl_c, cwd);

    Ok(())
}
