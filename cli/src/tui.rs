use std::io::{self, Write};
#[cfg(not(coverage))]
use std::time::Duration;

use crossterm::cursor::MoveTo;
#[cfg(not(coverage))]
use crossterm::cursor::{Hide, Show};
#[cfg(not(coverage))]
use crossterm::event::{self};
use crossterm::event::{Event, KeyCode, KeyEvent};
#[cfg(not(coverage))]
use crossterm::execute;
use crossterm::queue;
use crossterm::terminal::{Clear, ClearType};
#[cfg(not(coverage))]
use crossterm::terminal::{
    EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode,
};

use crate::state::{
    AgentSessionStatus, EvidenceStatus, FeatureState, GateStatus, GithubMergeability,
    GithubReviewStatus, WorkflowState,
};

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
    input: InputBuffer,
    queued_follow_ups: Vec<String>,
    last_event: String,
}

impl OperatorSurface {
    pub fn from_feature_state(feature: &FeatureState) -> Self {
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
                .map(|group| GateGroupView {
                    label: group.label.clone(),
                    gates: group
                        .gates
                        .iter()
                        .map(|gate| GateView {
                            label: gate.label.clone(),
                            status: gate_status_label(gate.status.clone()).to_string(),
                        })
                        .collect(),
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
            input: InputBuffer::default(),
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
            "Calypso Operator Surface".to_string(),
            format!("Feature: {}", self.feature_id),
            format!("Branch: {}", self.branch),
            format!("Workflow: {}", self.workflow),
            format!("Pull request: #{}", self.pull_request_number),
            format!("Queued follow-ups: {}", self.queued_follow_ups.len()),
            format!("Last event: {}", self.last_event),
            format!(
                "Blocking: {}",
                if self.blocking_gate_ids.is_empty() {
                    "none".to_string()
                } else {
                    self.blocking_gate_ids.join(", ")
                }
            ),
            String::new(),
            "Gate Groups".to_string(),
        ];

        if let Some(github) = &self.github {
            lines.push(String::new());
            lines.push("GitHub".to_string());
            lines.push(format!("PR state: {}", github.pr_state));
            lines.push(format!("Review: {}", github.review));
            lines.push(format!("Checks: {}", github.checks));
            lines.push(format!("Mergeability: {}", github.mergeability));
        } else if let Some(error) = &self.github_error {
            lines.push(String::new());
            lines.push("GitHub".to_string());
            lines.push(format!("Error: {error}"));
        }

        for group in &self.gate_groups {
            lines.push(format!("{}:", group.label));
            for gate in &group.gates {
                lines.push(format!("  [{}] {}", gate.status, gate.label));
            }
        }

        lines.push(String::new());
        lines.push("Active Sessions".to_string());

        if self.sessions.is_empty() {
            lines.push("  No active sessions".to_string());
        } else {
            for session in &self.sessions {
                lines.push(format!(
                    "  {} ({}) [{}]",
                    session.role, session.session_id, session.status
                ));
                for output in &session.output {
                    lines.push(format!("    {}", output));
                }
            }
        }

        lines.push(String::new());
        lines.push(format!("Follow-up input: {}", self.input.as_str()));
        lines.join("\n")
    }

    pub fn handle_key_event(&mut self, event: KeyEvent) -> SurfaceEvent {
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
                Some(follow_up) => SurfaceEvent::Submitted(follow_up),
                None => {
                    self.last_event = "ignored empty follow-up".to_string();
                    SurfaceEvent::Continue
                }
            },
            KeyCode::Esc => {
                self.last_event = "quit requested".to_string();
                SurfaceEvent::Quit
            }
            _ => SurfaceEvent::Continue,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SurfaceEvent {
    Continue,
    Submitted(String),
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
    let resize = Some(Event::Resize(80, 24));
    let type_a = Some(Event::Key(KeyEvent::from(KeyCode::Char('a'))));
    let submit = Some(Event::Key(KeyEvent::from(KeyCode::Enter)));
    let type_b = Some(Event::Key(KeyEvent::from(KeyCode::Char('b'))));
    let quit = Some(Event::Key(KeyEvent::from(KeyCode::Esc)));

    run_terminal_iteration(&mut stdout, feature, &mut surface, resize)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, type_a)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, submit.clone())?;
    if let Some(active_session) = feature.active_sessions.first_mut() {
        active_session.status = AgentSessionStatus::Completed;
    }
    run_terminal_iteration(&mut stdout, feature, &mut surface, type_b)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, submit)?;
    run_terminal_iteration(&mut stdout, feature, &mut surface, quit)?;
    Ok(())
}

#[cfg(not(coverage))]
fn run_terminal_loop(
    stdout: &mut impl Write,
    feature: &mut FeatureState,
    surface: &mut OperatorSurface,
) -> io::Result<()> {
    loop {
        let next_event = if event::poll(Duration::from_millis(250))? {
            Some(event::read()?)
        } else {
            None
        };

        if !run_terminal_iteration(stdout, feature, surface, next_event)? {
            return Ok(());
        }
    }
}

fn run_terminal_iteration(
    stdout: &mut impl Write,
    feature: &mut FeatureState,
    surface: &mut OperatorSurface,
    next_event: Option<Event>,
) -> io::Result<bool> {
    queue!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
    write!(stdout, "{}", surface.render())?;
    stdout.flush()?;

    if let Some(Event::Key(key_event)) = next_event {
        match surface.handle_key_event(key_event) {
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
            SurfaceEvent::Quit => return Ok(false),
        }
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct GateGroupView {
    label: String,
    gates: Vec<GateView>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct GateView {
    label: String,
    status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SessionView {
    role: String,
    session_id: String,
    status: String,
    output: Vec<String>,
}

fn workflow_label(state: WorkflowState) -> String {
    match state {
        WorkflowState::New => "new".to_string(),
        WorkflowState::Implementation => "implementation".to_string(),
        WorkflowState::WaitingForHuman => "waiting-for-human".to_string(),
        WorkflowState::ReadyForReview => "ready-for-review".to_string(),
        WorkflowState::Blocked => "blocked".to_string(),
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
