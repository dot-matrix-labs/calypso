use std::path::Path;

use crate::report::{AgentJsonSession, AgentsJsonReport};
use crate::state::{AgentSession, AgentSessionStatus, FeatureState};

fn agent_status_str(status: &AgentSessionStatus) -> &'static str {
    match status {
        AgentSessionStatus::Running => "running",
        AgentSessionStatus::WaitingForHuman => "waiting-for-human",
        AgentSessionStatus::Completed => "completed",
        AgentSessionStatus::Failed => "failed",
        AgentSessionStatus::Aborted => "aborted",
    }
}

/// Build an `AgentsJsonReport` from a `FeatureState`.
pub fn agents_json_report(feature: &FeatureState) -> AgentsJsonReport {
    let sessions = feature
        .active_sessions
        .iter()
        .map(|session| AgentJsonSession {
            session_id: session.session_id.clone(),
            role: session.role.clone(),
            status: agent_status_str(&session.status).to_string(),
            output: session.output.iter().map(|o| o.text.clone()).collect(),
            pending_follow_ups: session.pending_follow_ups.clone(),
        })
        .collect();

    AgentsJsonReport {
        feature_id: feature.feature_id.clone(),
        sessions,
    }
}

/// Load state from `.calypso/repository-state.json` and return the agents JSON report.
pub fn run_agents_json(cwd: &Path) -> Result<String, String> {
    let state_path = cwd.join(".calypso").join("repository-state.json");
    let state =
        crate::state::RepositoryState::load_from_path(&state_path).map_err(|e| e.to_string())?;
    let json_report = agents_json_report(&state.current_feature);
    serde_json::to_string_pretty(&json_report).map_err(|e| format!("serialization error: {e}"))
}

/// Render a human-readable agents status from a session list.
pub fn render_agents(feature: &FeatureState) -> String {
    let mut lines = Vec::new();
    lines.push(format!("Active sessions for {}", feature.feature_id));
    lines.push("─".repeat(42));

    if feature.active_sessions.is_empty() {
        lines.push("  (no active sessions)".to_string());
    } else {
        for session in &feature.active_sessions {
            let (marker, status_str) = session_display_parts(session);
            lines.push(format!(
                "{} {}  [{}]  {}",
                marker, session.role, session.session_id, status_str
            ));
            for line in &session.output {
                for text_line in line.text.lines() {
                    lines.push(format!("  {text_line}"));
                }
            }
        }
    }

    lines.join("\n")
}

fn session_display_parts(session: &AgentSession) -> (&'static str, &'static str) {
    match session.status {
        AgentSessionStatus::Running => ("▶", "running"),
        AgentSessionStatus::WaitingForHuman => ("⏸", "waiting-for-human"),
        AgentSessionStatus::Completed => ("✓", "completed"),
        AgentSessionStatus::Failed => ("✗", "failed"),
        AgentSessionStatus::Aborted => ("⊗", "aborted"),
    }
}

/// Load state from `.calypso/repository-state.json` and return a plain-text agents summary.
pub fn run_agents_plain(cwd: &Path) -> Result<String, String> {
    let state_path = cwd.join(".calypso").join("repository-state.json");
    let state =
        crate::state::RepositoryState::load_from_path(&state_path).map_err(|e| e.to_string())?;
    Ok(render_agents(&state.current_feature))
}
