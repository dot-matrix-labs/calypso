use std::collections::BTreeMap;
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::template::{AgentTaskKind, TemplateSet};

/// Identity metadata for the repository. Contains no secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RepositoryIdentity {
    pub name: String,
    pub github_remote_url: String,
    pub default_branch: String,
}

/// A reference to a secure credential. Contains only the reference identifier,
/// never the raw secret value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SecureKeyRef {
    pub id: String,
    pub name: String,
    pub purpose: String,
}

/// A summary entry for an active feature, used in the repository-level index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureSummary {
    pub feature_id: String,
    pub branch: String,
    pub worktree_path: String,
    #[serde(default)]
    pub pr_number: Option<u64>,
    pub state: WorkflowState,
}

/// A summary of a known git worktree.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorktreeSummary {
    pub path: String,
    pub branch: String,
    #[serde(default)]
    pub feature_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RepositoryState {
    pub version: u32,
    pub repo_id: String,
    pub current_feature: FeatureState,
    /// Schema version for forward-compatibility. Defaults to 1.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Repository identity metadata.
    #[serde(default)]
    pub identity: RepositoryIdentity,
    /// Names of configured providers (no secrets).
    #[serde(default)]
    pub providers: Vec<String>,
    /// Token name or keychain reference for GitHub auth. Never the raw token.
    #[serde(default)]
    pub github_auth_ref: Option<String>,
    /// References to secure keys. Contains only identifiers, never raw secrets.
    #[serde(default)]
    pub secure_key_refs: Vec<SecureKeyRef>,
    /// Index of all active features.
    #[serde(default)]
    pub active_features: Vec<FeatureSummary>,
    /// All known worktrees for this repository.
    #[serde(default)]
    pub known_worktrees: Vec<WorktreeSummary>,
}

fn default_schema_version() -> u32 {
    1
}

impl RepositoryState {
    pub fn to_json_pretty(&self) -> Result<String, StateError> {
        serde_json::to_string_pretty(self).map_err(StateError::Json)
    }

    pub fn from_json(json: &str) -> Result<Self, StateError> {
        serde_json::from_str(json).map_err(StateError::Json)
    }

    /// Atomically saves state by writing to a `.tmp` file then renaming into place.
    pub fn save_to_path(&self, path: &Path) -> Result<(), StateError> {
        let json = self.to_json_pretty()?;
        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, json).map_err(StateError::Io)?;
        fs::rename(&tmp_path, path).map_err(StateError::Io)
    }

    pub fn load_from_path(path: &Path) -> Result<Self, StateError> {
        let json = fs::read_to_string(path).map_err(StateError::Io)?;
        Self::from_json(&json)
    }
}

/// The type/category of a feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum FeatureType {
    Feat,
    Fix,
    Chore,
}

/// A record of a role and its most recent session within a feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleSession {
    pub role: String,
    #[serde(default)]
    pub session_id: Option<String>,
    #[serde(default)]
    pub last_outcome: Option<String>,
}

/// Scheduling and timing metadata for a feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SchedulingMeta {
    pub created_at: String,
    #[serde(default)]
    pub last_advanced_at: Option<String>,
    #[serde(default)]
    pub last_agent_run_at: Option<String>,
}

/// A reference to an artifact produced during feature work.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactRef {
    pub kind: String,
    pub path: String,
    #[serde(default)]
    pub session_id: Option<String>,
}

/// A single entry in the clarification history for a feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClarificationEntry {
    pub session_id: String,
    pub question: String,
    #[serde(default)]
    pub answer: Option<String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureState {
    pub feature_id: String,
    pub branch: String,
    pub worktree_path: String,
    pub pull_request: PullRequestRef,
    #[serde(default)]
    pub github_snapshot: Option<GithubPullRequestSnapshot>,
    #[serde(default)]
    pub github_error: Option<String>,
    pub workflow_state: WorkflowState,
    pub gate_groups: Vec<GateGroup>,
    pub active_sessions: Vec<AgentSession>,
    /// The type/category of this feature.
    #[serde(default = "default_feature_type")]
    pub feature_type: FeatureType,
    /// Role sessions associated with this feature.
    #[serde(default)]
    pub roles: Vec<RoleSession>,
    /// Scheduling and timing metadata.
    #[serde(default)]
    pub scheduling: SchedulingMeta,
    /// References to artifacts produced during this feature.
    #[serde(default)]
    pub artifact_refs: Vec<ArtifactRef>,
    /// Paths to transcript files.
    #[serde(default)]
    pub transcript_refs: Vec<String>,
    /// History of clarification Q&A for this feature.
    #[serde(default)]
    pub clarification_history: Vec<ClarificationEntry>,
}

fn default_feature_type() -> FeatureType {
    FeatureType::Feat
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
            github_snapshot: None,
            github_error: None,
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
            feature_type: FeatureType::Feat,
            roles: Vec::new(),
            scheduling: SchedulingMeta::default(),
            artifact_refs: Vec::new(),
            transcript_refs: Vec::new(),
            clarification_history: Vec::new(),
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

                        match evidence.status_for(builtin) {
                            Some(EvidenceStatus::Passing) => GateStatus::Passing,
                            Some(EvidenceStatus::Failing) => GateStatus::Failing,
                            Some(EvidenceStatus::Pending) => GateStatus::Pending,
                            Some(EvidenceStatus::Manual) => GateStatus::Manual,
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
pub struct GithubPullRequestSnapshot {
    pub is_draft: bool,
    pub review_status: GithubReviewStatus,
    pub checks: EvidenceStatus,
    pub mergeability: GithubMergeability,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GithubReviewStatus {
    Approved,
    ReviewRequired,
    ChangesRequested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GithubMergeability {
    Mergeable,
    Conflicting,
    Blocked,
    Unknown,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum EvidenceStatus {
    Passing,
    Failing,
    Pending,
    Manual,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BuiltinEvidence {
    results: BTreeMap<String, EvidenceStatus>,
}

impl BuiltinEvidence {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_result(mut self, builtin: &str, passed: bool) -> Self {
        self.results.insert(
            builtin.to_string(),
            if passed {
                EvidenceStatus::Passing
            } else {
                EvidenceStatus::Failing
            },
        );
        self
    }

    pub fn with_status(mut self, builtin: &str, status: EvidenceStatus) -> Self {
        self.results.insert(builtin.to_string(), status);
        self
    }

    pub fn result_for(&self, builtin: &str) -> Option<bool> {
        match self.results.get(builtin).copied() {
            Some(EvidenceStatus::Passing) => Some(true),
            Some(EvidenceStatus::Failing) => Some(false),
            Some(EvidenceStatus::Pending) | Some(EvidenceStatus::Manual) | None => None,
        }
    }

    pub fn status_for(&self, builtin: &str) -> Option<EvidenceStatus> {
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
