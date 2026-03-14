use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::template::{AgentTaskKind, TemplateSet};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryState {
    pub version: u32,
    pub repo_id: String,
    pub current_feature: FeatureState,
}

impl RepositoryState {
    pub fn to_json_pretty(&self) -> Result<String, StateError> {
        serde_json::to_string_pretty(self).map_err(StateError::Json)
    }

    pub fn from_json(json: &str) -> Result<Self, StateError> {
        serde_json::from_str(json).map_err(StateError::Json)
    }

    pub fn save_to_path(&self, path: &Path) -> Result<(), StateError> {
        let json = self.to_json_pretty()?;
        fs::write(path, json).map_err(StateError::Io)
    }

    pub fn load_from_path(path: &Path) -> Result<Self, StateError> {
        let json = fs::read_to_string(path).map_err(StateError::Io)?;
        Self::from_json(&json)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureState {
    pub feature_id: String,
    pub branch: String,
    pub worktree_path: String,
    pub pull_request: PullRequestRef,
    pub workflow_state: WorkflowState,
    pub gate_groups: Vec<GateGroup>,
    pub active_sessions: Vec<AgentSession>,
}

impl FeatureState {
    pub fn from_template(
        feature_id: &str,
        branch: &str,
        worktree_path: &str,
        pull_request: PullRequestRef,
        template: &TemplateSet,
    ) -> Result<Self, GateInitializationError> {
        Ok(Self {
            feature_id: feature_id.to_string(),
            branch: branch.to_string(),
            worktree_path: worktree_path.to_string(),
            pull_request,
            workflow_state: WorkflowState::from_template_state_name(
                template.state_machine.initial_state.as_str(),
            )?,
            gate_groups: template
                .state_machine
                .gate_groups
                .iter()
                .map(|group| GateGroup {
                    id: group.id.clone(),
                    label: group.label.clone(),
                    gates: group
                        .gates
                        .iter()
                        .map(|gate| Gate {
                            id: gate.id.clone(),
                            label: gate.label.clone(),
                            task: gate.task.clone(),
                            status: GateStatus::Pending,
                        })
                        .collect(),
                })
                .collect(),
            active_sessions: Vec::new(),
        })
    }

    pub fn evaluate_gates(
        &mut self,
        template: &TemplateSet,
        evidence: &BuiltinEvidence,
    ) -> Result<(), GateEvaluationError> {
        for group in &mut self.gate_groups {
            for gate in &mut group.gates {
                let task = template
                    .task_by_name(gate.task.as_str())
                    .ok_or_else(|| GateEvaluationError::UnknownTask(gate.task.clone()))?;

                gate.status = match task.kind {
                    AgentTaskKind::Builtin => {
                        let builtin = task
                            .builtin
                            .as_deref()
                            .expect("validated builtin tasks must define a builtin evaluator");

                        match evidence.result_for(builtin) {
                            Some(true) => GateStatus::Passing,
                            Some(false) => GateStatus::Failing,
                            None => GateStatus::Pending,
                        }
                    }
                    AgentTaskKind::Human => GateStatus::Manual,
                    AgentTaskKind::Agent | AgentTaskKind::Hook => GateStatus::Pending,
                };
            }
        }

        Ok(())
    }

    pub fn blocking_gate_ids(&self) -> Vec<String> {
        self.gate_groups
            .iter()
            .flat_map(|group| group.gates.iter())
            .filter(|gate| gate.status != GateStatus::Passing)
            .map(|gate| gate.id.clone())
            .collect()
    }

    pub fn gate_group_rollups(&self) -> Vec<GateGroupRollup> {
        self.gate_groups.iter().map(GateGroup::rollup).collect()
    }

    pub fn pull_request_checklist(&self) -> Vec<PullRequestChecklistItem> {
        self.gate_groups
            .iter()
            .flat_map(|group| group.gates.iter())
            .map(|gate| PullRequestChecklistItem {
                gate_id: gate.id.clone(),
                label: gate.label.clone(),
                checked: gate.status == GateStatus::Passing,
            })
            .collect()
    }

    pub fn available_transitions(&self, facts: &TransitionFacts) -> Vec<WorkflowState> {
        self.workflow_state.available_transitions(facts)
    }

    pub fn transition_to(
        &mut self,
        target: WorkflowState,
        facts: &TransitionFacts,
    ) -> Result<(), TransitionError> {
        self.workflow_state
            .validate_transition(target.clone(), facts)?;
        self.workflow_state = target;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestRef {
    pub number: u64,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestChecklistItem {
    pub gate_id: String,
    pub label: String,
    pub checked: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum WorkflowState {
    New,
    Implementation,
    WaitingForHuman,
    ReadyForReview,
    Blocked,
}

impl WorkflowState {
    fn from_template_state_name(name: &str) -> Result<Self, GateInitializationError> {
        match name {
            "new" => Ok(Self::New),
            "implementation" => Ok(Self::Implementation),
            "waiting-for-human" => Ok(Self::WaitingForHuman),
            "ready-for-review" => Ok(Self::ReadyForReview),
            "blocked" => Ok(Self::Blocked),
            _ => Err(GateInitializationError::UnknownWorkflowState(
                name.to_string(),
            )),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Implementation => "implementation",
            Self::WaitingForHuman => "waiting-for-human",
            Self::ReadyForReview => "ready-for-review",
            Self::Blocked => "blocked",
        }
    }

    pub fn available_transitions(&self, facts: &TransitionFacts) -> Vec<Self> {
        let mut transitions = Vec::new();

        match self {
            Self::New => {
                if facts.feature_binding_complete {
                    transitions.push(Self::Implementation);
                }
            }
            Self::Implementation => {
                if facts.blocking_issue_present {
                    transitions.push(Self::Blocked);
                }
                if facts.waiting_for_human_input {
                    transitions.push(Self::WaitingForHuman);
                }
                if facts.ready_for_review {
                    transitions.push(Self::ReadyForReview);
                }
            }
            Self::WaitingForHuman => {
                if facts.blocking_issue_present {
                    transitions.push(Self::Blocked);
                }
                if facts.human_response_ready {
                    transitions.push(Self::Implementation);
                }
            }
            Self::ReadyForReview => {
                if facts.blocking_issue_present {
                    transitions.push(Self::Blocked);
                }
                if facts.review_rework_required {
                    transitions.push(Self::Implementation);
                }
            }
            Self::Blocked => {
                if facts.blocker_resolved {
                    transitions.push(Self::Implementation);
                }
            }
        }

        transitions
    }

    pub fn validate_transition(
        &self,
        target: Self,
        facts: &TransitionFacts,
    ) -> Result<(), TransitionError> {
        if self.available_transitions(facts).contains(&target) {
            return Ok(());
        }

        Err(TransitionError::Rejected {
            from: self.clone(),
            to: target.clone(),
            reason: self.missing_transition_reason(&target).to_string(),
        })
    }

    fn missing_transition_reason(&self, target: &Self) -> &'static str {
        match (self, target) {
            (Self::New, Self::Implementation) => "feature binding is incomplete",
            (Self::Implementation, Self::WaitingForHuman) => {
                "no agent session is waiting for human input"
            }
            (Self::Implementation, Self::ReadyForReview) => "feature is not ready for review",
            (Self::Implementation, Self::Blocked) => "no blocking issue is present",
            (Self::WaitingForHuman, Self::Implementation) => "no human response is available",
            (Self::WaitingForHuman, Self::Blocked) => "no blocking issue is present",
            (Self::ReadyForReview, Self::Implementation) => {
                "no follow-up implementation request is present"
            }
            (Self::ReadyForReview, Self::Blocked) => "no blocking issue is present",
            (Self::Blocked, Self::Implementation) => "blocking issue is still present",
            _ => "transition is not supported by the prototype workflow",
        }
    }
}

impl fmt::Display for WorkflowState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct TransitionFacts {
    pub feature_binding_complete: bool,
    pub blocking_issue_present: bool,
    pub waiting_for_human_input: bool,
    pub human_response_ready: bool,
    pub ready_for_review: bool,
    pub review_rework_required: bool,
    pub blocker_resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransitionError {
    Rejected {
        from: WorkflowState,
        to: WorkflowState,
        reason: String,
    },
}

impl fmt::Display for TransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected { from, to, reason } => {
                write!(f, "cannot transition from '{from}' to '{to}': {reason}")
            }
        }
    }
}

impl std::error::Error for TransitionError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateGroup {
    pub id: String,
    pub label: String,
    pub gates: Vec<Gate>,
}

impl GateGroup {
    pub fn rollup(&self) -> GateGroupRollup {
        GateGroupRollup {
            id: self.id.clone(),
            label: self.label.clone(),
            status: self.rollup_status(),
            blocking_gate_ids: self
                .gates
                .iter()
                .filter(|gate| gate.status != GateStatus::Passing)
                .map(|gate| gate.id.clone())
                .collect(),
        }
    }

    pub fn rollup_status(&self) -> GateGroupStatus {
        if self
            .gates
            .iter()
            .any(|gate| gate.status == GateStatus::Failing)
        {
            GateGroupStatus::Blocked
        } else if self
            .gates
            .iter()
            .any(|gate| gate.status == GateStatus::Pending)
        {
            GateGroupStatus::Pending
        } else if self
            .gates
            .iter()
            .any(|gate| gate.status == GateStatus::Manual)
        {
            GateGroupStatus::Manual
        } else {
            GateGroupStatus::Passing
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GateGroupRollup {
    pub id: String,
    pub label: String,
    pub status: GateGroupStatus,
    pub blocking_gate_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateGroupStatus {
    Passing,
    Pending,
    Manual,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gate {
    pub id: String,
    pub label: String,
    pub task: String,
    pub status: GateStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateStatus {
    Pending,
    Passing,
    Failing,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSession {
    pub role: String,
    pub session_id: String,
    #[serde(default)]
    pub provider_session_id: Option<String>,
    pub status: AgentSessionStatus,
    #[serde(default)]
    pub output: Vec<SessionOutput>,
    #[serde(default)]
    pub pending_follow_ups: Vec<String>,
    #[serde(default)]
    pub terminal_outcome: Option<AgentTerminalOutcome>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentSessionStatus {
    Running,
    WaitingForHuman,
    Completed,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionOutput {
    pub stream: SessionOutputStream,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SessionOutputStream {
    Stdout,
    Stderr,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentTerminalOutcome {
    Ok,
    Nok,
    Aborted,
}

#[derive(Debug)]
pub enum StateError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StateError::Io(error) => write!(f, "state I/O error: {error}"),
            StateError::Json(error) => write!(f, "state JSON error: {error}"),
        }
    }
}

impl std::error::Error for StateError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateInitializationError {
    UnknownWorkflowState(String),
}

impl fmt::Display for GateInitializationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GateInitializationError::UnknownWorkflowState(state) => {
                write!(
                    f,
                    "unknown workflow state '{state}' in methodology template"
                )
            }
        }
    }
}

impl std::error::Error for GateInitializationError {}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuiltinEvidence {
    results: BTreeMap<String, bool>,
}

impl BuiltinEvidence {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_result(mut self, builtin: &str, passed: bool) -> Self {
        self.results.insert(builtin.to_string(), passed);
        self
    }

    pub fn result_for(&self, builtin: &str) -> Option<bool> {
        self.results.get(builtin).copied()
    }

    pub fn merge(mut self, other: &Self) -> Self {
        self.results.extend(other.results.clone());
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GateEvaluationError {
    UnknownTask(String),
}

impl fmt::Display for GateEvaluationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            GateEvaluationError::UnknownTask(task) => {
                write!(f, "gate evaluation references unknown task '{task}'")
            }
        }
    }
}

impl std::error::Error for GateEvaluationError {}
