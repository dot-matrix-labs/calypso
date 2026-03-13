use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

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
