use std::collections::BTreeMap;
use std::io::{self, Write};
use std::time::Duration;

use crossterm::cursor::{Hide, MoveTo, Show};
use crossterm::event::{self, Event, KeyCode, KeyEvent};
use crossterm::terminal::{
    Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode,
    enable_raw_mode,
};
use crossterm::{execute, queue};

use crate::state::{AgentSessionStatus, FeatureState, GateStatus, WorkflowState};

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
    blocking_gate_ids: Vec<String>,
    gate_groups: Vec<GateGroupView>,
    sessions: Vec<SessionView>,
    input: InputBuffer,
    queued_follow_ups: Vec<String>,
    last_event: String,
}

impl OperatorSurface {
    pub fn from_feature_state(
        feature: &FeatureState,
        session_output: Vec<(String, Vec<String>)>,
    ) -> Self {
        let output_by_session: BTreeMap<String, Vec<String>> = session_output.into_iter().collect();

        Self {
            feature_id: feature.feature_id.clone(),
            branch: feature.branch.clone(),
            workflow: workflow_label(feature.workflow_state.clone()),
            pull_request_number: feature.pull_request.number,
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
                    output: output_by_session
                        .get(session.session_id.as_str())
                        .cloned()
                        .unwrap_or_else(|| vec!["No streamed output yet.".to_string()]),
                })
                .collect(),
            input: InputBuffer::default(),
            queued_follow_ups: Vec::new(),
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
                Some(follow_up) => {
                    self.queued_follow_ups.push(follow_up.clone());
                    self.last_event = "queued follow-up".to_string();
                    SurfaceEvent::Submitted(follow_up)
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

pub fn run_terminal_surface(
    feature: &FeatureState,
    session_output: Vec<(String, Vec<String>)>,
) -> io::Result<()> {
    let mut stdout = io::stdout();
    let mut surface = OperatorSurface::from_feature_state(feature, session_output);

    enable_raw_mode()?;
    execute!(stdout, EnterAlternateScreen, Hide)?;

    let result = run_terminal_loop(&mut stdout, &mut surface);

    execute!(stdout, Show, LeaveAlternateScreen)?;
    disable_raw_mode()?;
    result
}

fn run_terminal_loop(stdout: &mut impl Write, surface: &mut OperatorSurface) -> io::Result<()> {
    loop {
        queue!(stdout, Clear(ClearType::All), MoveTo(0, 0))?;
        write!(stdout, "{}", surface.render())?;
        stdout.flush()?;

        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(key_event) = event::read()? {
                if surface.handle_key_event(key_event) == SurfaceEvent::Quit {
                    return Ok(());
                }
            }
        }
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
    }
}
