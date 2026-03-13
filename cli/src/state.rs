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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PullRequestRef {
    pub number: u64,
    pub url: String,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateGroup {
    pub id: String,
    pub label: String,
    pub gates: Vec<Gate>,
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
    pub status: AgentSessionStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentSessionStatus {
    Running,
    WaitingForHuman,
    Completed,
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
