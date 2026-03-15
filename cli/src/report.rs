//! Thin output structs used by `--json` flags across CLI subcommands.
//!
//! All types derive `Serialize`; none carry business logic.

use serde::Serialize;

// ---------------------------------------------------------------------------
// Feature 1 — doctor --json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorJsonReport {
    pub checks: Vec<DoctorJsonCheck>,
    pub summary: DoctorJsonSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorJsonCheck {
    pub id: String,
    pub status: &'static str,
    pub detail: Option<String>,
    pub remediation: Option<String>,
    pub has_auto_fix: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DoctorJsonSummary {
    pub total: usize,
    pub passing: usize,
    pub warnings: usize,
    pub failing: usize,
}

// ---------------------------------------------------------------------------
// Feature 2 — state status --json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateStatusJsonReport {
    pub feature_id: String,
    pub branch: String,
    pub pr_number: Option<u64>,
    pub workflow_state: String,
    pub gate_groups: Vec<StateJsonGateGroup>,
    pub blocking_gate_ids: Vec<String>,
    pub active_session_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateJsonGateGroup {
    pub id: String,
    pub label: String,
    pub status: &'static str,
    pub gates: Vec<StateJsonGate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StateJsonGate {
    pub id: String,
    pub label: String,
    pub status: &'static str,
}

// ---------------------------------------------------------------------------
// Feature 3 — agents --json
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentsJsonReport {
    pub feature_id: String,
    pub sessions: Vec<AgentJsonSession>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AgentJsonSession {
    pub session_id: String,
    pub role: String,
    pub status: String,
    pub output: Vec<String>,
    pub pending_follow_ups: Vec<String>,
}
