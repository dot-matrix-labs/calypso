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

// FUTURE: #42 — vault-backed credential references
// #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
// pub struct SecureKeyRef {
//     pub id: String,
//     pub name: String,
//     pub purpose: String,
// }

/// A summary entry for an active feature, used in the repository-level index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureSummary {
    pub feature_id: String,
    pub branch: String,
    pub worktree_path: String,
    #[serde(default)]
    pub pr_number: Option<u64>,
    pub state: String,
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
    /// Schema version for forward-compatibility. Incremented to 2 for 11-state lifecycle.
    #[serde(default = "default_schema_version")]
    pub schema_version: u32,
    /// Repository identity metadata.
    #[serde(default)]
    pub identity: RepositoryIdentity,
    /// Names of configured providers (no secrets).
    // FUTURE: #42/#48 — parsed and persisted but not yet enforced; provider registry
    // integration is pending multi-vendor support.
    #[serde(default)]
    pub providers: Vec<String>,
    // FUTURE: #42 — token name or keychain reference for GitHub auth; never the raw token
    // #[serde(default)]
    // pub github_auth_ref: Option<String>,
    // FUTURE: #42 — vault-backed credential references; contains only identifiers, never raw secrets
    // #[serde(default)]
    // pub secure_key_refs: Vec<SecureKeyRef>,
    // FUTURE: #40 — repository-level index of all active features
    // #[serde(default)]
    // pub active_features: Vec<FeatureSummary>,
    // FUTURE: #40 — registry of all known git worktrees for this repository
    // #[serde(default)]
    // pub known_worktrees: Vec<WorktreeSummary>,
    /// Release records for this repository.
    #[serde(default)]
    pub releases: Vec<ReleaseRecord>,
    /// Deployment records, one per environment.
    #[serde(default)]
    pub deployments: Vec<DeploymentRecord>,
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
    pub workflow_state: String,
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
    ) -> Self {
        Self {
            feature_id: feature_id.to_string(),
            branch: branch.to_string(),
            worktree_path: worktree_path.to_string(),
            pull_request,
            github_snapshot: None,
            github_error: None,
            workflow_state: template.state_machine.initial_state.clone(),
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
        }
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

// ---------------------------------------------------------------------------
// Release state machine
// ---------------------------------------------------------------------------

/// The lifecycle state of a software release.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReleaseState {
    Planned,
    InProgress,
    Candidate,
    Validated,
    Approved,
    Deployed,
    RolledBack,
    Aborted,
}

impl ReleaseState {
    /// Returns the set of states that are valid next states from `self`.
    pub fn valid_next_states(&self) -> Vec<Self> {
        match self {
            Self::Planned => vec![Self::InProgress, Self::Aborted],
            Self::InProgress => vec![Self::Candidate],
            Self::Candidate => vec![Self::Validated, Self::InProgress],
            Self::Validated => vec![Self::Approved, Self::Candidate],
            Self::Approved => vec![Self::Deployed],
            Self::Deployed => vec![Self::RolledBack],
            Self::RolledBack | Self::Aborted => vec![],
        }
    }

    /// Validates that transitioning from `self` to `target` is permitted.
    pub fn validate_transition(&self, target: &Self) -> Result<(), ReleaseTransitionError> {
        if self.valid_next_states().contains(target) {
            return Ok(());
        }
        Err(ReleaseTransitionError::Rejected {
            from: self.clone(),
            to: target.clone(),
            reason: self.rejection_reason(target).to_string(),
        })
    }

    fn rejection_reason(&self, target: &Self) -> &'static str {
        match (self, target) {
            (Self::RolledBack, _) | (Self::Aborted, _) => "state is terminal",
            _ => "transition is not permitted by the release state machine",
        }
    }

    /// Returns `true` if this state is terminal (no further transitions allowed).
    pub fn is_terminal(&self) -> bool {
        self.valid_next_states().is_empty()
    }
}

impl fmt::Display for ReleaseState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Planned => "planned",
            Self::InProgress => "in-progress",
            Self::Candidate => "candidate",
            Self::Validated => "validated",
            Self::Approved => "approved",
            Self::Deployed => "deployed",
            Self::RolledBack => "rolled-back",
            Self::Aborted => "aborted",
        };
        f.write_str(s)
    }
}

/// A release lifecycle record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReleaseRecord {
    pub release_id: String,
    pub candidate_version: String,
    pub state: ReleaseState,
    /// Session or gate ref that validated this release.
    #[serde(default)]
    pub validation_ref: Option<String>,
    /// Human sign-off reference.
    #[serde(default)]
    pub approval_ref: Option<String>,
    /// Deployment record ID associated with this release.
    #[serde(default)]
    pub deployment_ref: Option<String>,
    /// ID of the deployment that was rolled back.
    #[serde(default)]
    pub rollback_state: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Error type for release state transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReleaseTransitionError {
    Rejected {
        from: ReleaseState,
        to: ReleaseState,
        reason: String,
    },
}

impl fmt::Display for ReleaseTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected { from, to, reason } => {
                write!(
                    f,
                    "cannot transition release from '{from}' to '{to}': {reason}"
                )
            }
        }
    }
}

impl std::error::Error for ReleaseTransitionError {}

// ---------------------------------------------------------------------------
// Deployment state machine
// ---------------------------------------------------------------------------

/// The lifecycle state of a deployment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DeploymentState {
    Idle,
    Pending,
    Deploying,
    Deployed,
    Failed,
    RollingBack,
    RolledBack,
}

impl DeploymentState {
    /// Returns the set of states that are valid next states from `self`.
    pub fn valid_next_states(&self) -> Vec<Self> {
        match self {
            Self::Idle => vec![Self::Pending],
            Self::Pending => vec![Self::Deploying, Self::Idle],
            Self::Deploying => vec![Self::Deployed, Self::Failed],
            Self::Deployed => vec![Self::RollingBack, Self::Idle],
            Self::Failed => vec![Self::RollingBack, Self::Idle],
            Self::RollingBack => vec![Self::RolledBack, Self::Failed],
            Self::RolledBack => vec![Self::Idle],
        }
    }

    /// Validates that transitioning from `self` to `target` is permitted.
    pub fn validate_transition(&self, target: &Self) -> Result<(), DeploymentTransitionError> {
        if self.valid_next_states().contains(target) {
            return Ok(());
        }
        Err(DeploymentTransitionError::Rejected {
            from: self.clone(),
            to: target.clone(),
            reason: "transition is not permitted by the deployment state machine".to_string(),
        })
    }
}

impl fmt::Display for DeploymentState {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::Idle => "idle",
            Self::Pending => "pending",
            Self::Deploying => "deploying",
            Self::Deployed => "deployed",
            Self::Failed => "failed",
            Self::RollingBack => "rolling-back",
            Self::RolledBack => "rolled-back",
        };
        f.write_str(s)
    }
}

/// A deployment record tracking the state of a deployment to a specific environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentRecord {
    pub deployment_id: String,
    /// Target environment, e.g. "prod", "staging", "demo".
    pub environment: String,
    pub desired_code_version: String,
    #[serde(default)]
    pub deployed_code_version: Option<String>,
    #[serde(default)]
    pub desired_migration_version: Option<String>,
    #[serde(default)]
    pub deployed_migration_version: Option<String>,
    pub state: DeploymentState,
    #[serde(default)]
    pub last_result: Option<String>,
    /// deployment_id to roll back to.
    #[serde(default)]
    pub rollback_target: Option<String>,
    #[serde(default)]
    pub actor: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// Error type for deployment state transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeploymentTransitionError {
    Rejected {
        from: DeploymentState,
        to: DeploymentState,
        reason: String,
    },
}

impl fmt::Display for DeploymentTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected { from, to, reason } => {
                write!(
                    f,
                    "cannot transition deployment from '{from}' to '{to}': {reason}"
                )
            }
        }
    }
}

impl std::error::Error for DeploymentTransitionError {}

// ---------------------------------------------------------------------------
// Development state machine
// ---------------------------------------------------------------------------

/// The top-level phase of a project in the Calypso development lifecycle.
///
/// The `Init` phase delegates to the init state machine (`InitStep`). Once
/// init completes, the project automatically transitions to `Development`.
/// This is the outer state machine that wraps the init sub-state-machine and
/// subsequent active work phases.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DevelopmentPhase {
    /// Repository setup — delegates to the init state machine.
    #[default]
    Init,
    /// Active feature development.
    Development,
    /// Testing and QA.
    Testing,
}

impl DevelopmentPhase {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Init => "init",
            Self::Development => "development",
            Self::Testing => "testing",
        }
    }

    /// Returns the valid next phases from the current phase.
    pub fn valid_next_phases(&self) -> Vec<Self> {
        match self {
            Self::Init => vec![Self::Development],
            Self::Development => vec![Self::Testing, Self::Init],
            Self::Testing => vec![Self::Development, Self::Init],
        }
    }

    /// Returns `true` if transitioning to `target` is permitted.
    pub fn can_transition_to(&self, target: &Self) -> bool {
        self.valid_next_phases().contains(target)
    }

    /// Returns `true` if this is the initial setup phase.
    pub fn is_init(&self) -> bool {
        matches!(self, Self::Init)
    }

    /// Validates that transitioning to `target` is permitted.
    /// Returns `Err` with a reason string when the transition is not allowed.
    pub fn validate_transition(&self, target: &Self) -> Result<(), DevelopmentTransitionError> {
        if self.can_transition_to(target) {
            return Ok(());
        }
        Err(DevelopmentTransitionError::Rejected {
            from: self.clone(),
            to: target.clone(),
            reason: match (self, target) {
                (Self::Init, Self::Testing) => {
                    "init must complete before entering testing".to_string()
                }
                (Self::Testing, Self::Testing) | (Self::Development, Self::Development) => {
                    "already in this phase".to_string()
                }
                _ => format!(
                    "transition from '{}' to '{}' is not permitted",
                    self.as_str(),
                    target.as_str()
                ),
            },
        })
    }
}

impl fmt::Display for DevelopmentPhase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Error type for development phase transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DevelopmentTransitionError {
    Rejected {
        from: DevelopmentPhase,
        to: DevelopmentPhase,
        reason: String,
    },
}

impl fmt::Display for DevelopmentTransitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rejected { from, to, reason } => {
                write!(
                    f,
                    "cannot transition development phase from '{from}' to '{to}': {reason}"
                )
            }
        }
    }
}

impl std::error::Error for DevelopmentTransitionError {}

/// Persisted state for the development lifecycle state machine.
///
/// Written to `.calypso/dev-state.json`. This is the outer state machine
/// that wraps the init sub-state-machine (`InitStep`) and tracks which
/// development phase the project is currently in.
///
/// When `phase` is `Init`, the `init_step` field tracks progress within
/// the init sub-state-machine. When init reaches `Complete`, the phase
/// automatically transitions to `Development`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevelopmentState {
    /// Current outer phase.
    pub phase: DevelopmentPhase,
    /// When in `Init` phase, tracks the current step of the init sub-state-machine.
    /// `None` when not in the init phase (or init has not yet started).
    #[serde(default)]
    pub init_step: Option<String>,
    /// Timestamp of the last phase transition (ISO 8601).
    #[serde(default)]
    pub last_transition_at: Option<String>,
    /// History of phase transitions, most recent last.
    #[serde(default)]
    pub transition_log: Vec<DevelopmentTransitionEntry>,
}

/// A single entry recording a phase transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DevelopmentTransitionEntry {
    pub from: DevelopmentPhase,
    pub to: DevelopmentPhase,
    pub timestamp: String,
}

impl DevelopmentState {
    /// Create a new `DevelopmentState` starting in the `Init` phase.
    pub fn new() -> Self {
        Self {
            phase: DevelopmentPhase::Init,
            init_step: None,
            last_transition_at: None,
            transition_log: Vec::new(),
        }
    }

    /// Transition to a new phase, recording the transition in the log.
    ///
    /// Returns `Err` if the transition is not permitted by the state machine.
    pub fn transition_to(
        &mut self,
        target: DevelopmentPhase,
        timestamp: &str,
    ) -> Result<(), DevelopmentTransitionError> {
        self.phase.validate_transition(&target)?;
        let entry = DevelopmentTransitionEntry {
            from: self.phase.clone(),
            to: target.clone(),
            timestamp: timestamp.to_string(),
        };
        self.transition_log.push(entry);
        self.last_transition_at = Some(timestamp.to_string());
        // Clear init_step when leaving Init, set it when entering Init
        if target == DevelopmentPhase::Init {
            self.init_step = None; // will be populated by the init runner
        } else {
            self.init_step = None;
        }
        self.phase = target;
        Ok(())
    }

    /// Called when the init sub-state-machine advances a step.
    /// Updates the tracked `init_step`.
    pub fn update_init_step(&mut self, step_name: &str) {
        self.init_step = Some(step_name.to_string());
    }

    /// Check whether the init sub-state-machine has completed, and if so,
    /// automatically transition to `Development`.
    ///
    /// Returns `true` if the auto-transition occurred.
    pub fn auto_advance_from_init(&mut self, timestamp: &str) -> bool {
        if self.phase == DevelopmentPhase::Init && self.init_step.as_deref() == Some("complete") {
            // This transition is always valid (Init -> Development)
            let _ = self.transition_to(DevelopmentPhase::Development, timestamp);
            true
        } else {
            false
        }
    }

    /// Atomically saves state by writing to a `.tmp` file then renaming into place.
    pub fn save_to_path(&self, path: &Path) -> Result<(), StateError> {
        let json = serde_json::to_string_pretty(self).map_err(StateError::Json)?;
        let tmp_path = path.with_extension("tmp");
        fs::write(&tmp_path, &json).map_err(StateError::Io)?;
        fs::rename(&tmp_path, path).map_err(StateError::Io)
    }

    /// Load state from a JSON file.
    pub fn load_from_path(path: &Path) -> Result<Self, StateError> {
        let json = fs::read_to_string(path).map_err(StateError::Io)?;
        serde_json::from_str(&json).map_err(StateError::Json)
    }
}

impl Default for DevelopmentState {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── DevelopmentPhase ──────────────────────────────────────────────────

    #[test]
    fn development_phase_as_str_values() {
        assert_eq!(DevelopmentPhase::Init.as_str(), "init");
        assert_eq!(DevelopmentPhase::Development.as_str(), "development");
        assert_eq!(DevelopmentPhase::Testing.as_str(), "testing");
    }

    #[test]
    fn development_phase_default_is_init() {
        assert_eq!(DevelopmentPhase::default(), DevelopmentPhase::Init);
    }

    #[test]
    fn development_phase_display_matches_as_str() {
        for phase in [
            DevelopmentPhase::Init,
            DevelopmentPhase::Development,
            DevelopmentPhase::Testing,
        ] {
            assert_eq!(phase.to_string(), phase.as_str());
        }
    }

    #[test]
    fn development_phase_valid_transitions_from_init() {
        let valid = DevelopmentPhase::Init.valid_next_phases();
        assert_eq!(valid, vec![DevelopmentPhase::Development]);
    }

    #[test]
    fn development_phase_valid_transitions_from_development() {
        let valid = DevelopmentPhase::Development.valid_next_phases();
        assert!(valid.contains(&DevelopmentPhase::Testing));
        assert!(valid.contains(&DevelopmentPhase::Init));
    }

    #[test]
    fn development_phase_valid_transitions_from_testing() {
        let valid = DevelopmentPhase::Testing.valid_next_phases();
        assert!(valid.contains(&DevelopmentPhase::Development));
        assert!(valid.contains(&DevelopmentPhase::Init));
    }

    #[test]
    fn development_phase_can_transition_to_checks() {
        assert!(DevelopmentPhase::Init.can_transition_to(&DevelopmentPhase::Development));
        assert!(!DevelopmentPhase::Init.can_transition_to(&DevelopmentPhase::Testing));
        assert!(DevelopmentPhase::Development.can_transition_to(&DevelopmentPhase::Init));
        assert!(DevelopmentPhase::Testing.can_transition_to(&DevelopmentPhase::Init));
    }

    #[test]
    fn development_phase_is_init() {
        assert!(DevelopmentPhase::Init.is_init());
        assert!(!DevelopmentPhase::Development.is_init());
        assert!(!DevelopmentPhase::Testing.is_init());
    }

    #[test]
    fn development_phase_validate_transition_ok() {
        let result = DevelopmentPhase::Init.validate_transition(&DevelopmentPhase::Development);
        assert!(result.is_ok());
    }

    #[test]
    fn development_phase_validate_transition_rejected() {
        let result = DevelopmentPhase::Init.validate_transition(&DevelopmentPhase::Testing);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("cannot transition"));
    }

    #[test]
    fn development_phase_serializes_to_kebab_case() {
        let json = serde_json::to_string(&DevelopmentPhase::Development).unwrap();
        assert_eq!(json, "\"development\"");
    }

    #[test]
    fn development_phase_deserializes_from_kebab_case() {
        let phase: DevelopmentPhase = serde_json::from_str("\"testing\"").unwrap();
        assert_eq!(phase, DevelopmentPhase::Testing);
    }

    #[test]
    fn development_phase_round_trips_through_json() {
        for phase in [
            DevelopmentPhase::Init,
            DevelopmentPhase::Development,
            DevelopmentPhase::Testing,
        ] {
            let json = serde_json::to_string(&phase).unwrap();
            let decoded: DevelopmentPhase = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, phase);
        }
    }

    // ── DevelopmentTransitionError ────────────────────────────────────────

    #[test]
    fn development_transition_error_display() {
        let err = DevelopmentTransitionError::Rejected {
            from: DevelopmentPhase::Init,
            to: DevelopmentPhase::Testing,
            reason: "not allowed".to_string(),
        };
        let msg = err.to_string();
        assert!(msg.contains("init"));
        assert!(msg.contains("testing"));
        assert!(msg.contains("not allowed"));
    }

    // ── DevelopmentState ─────────────────────────────────────────────────

    #[test]
    fn development_state_new_starts_at_init() {
        let state = DevelopmentState::new();
        assert_eq!(state.phase, DevelopmentPhase::Init);
        assert!(state.init_step.is_none());
        assert!(state.last_transition_at.is_none());
        assert!(state.transition_log.is_empty());
    }

    #[test]
    fn development_state_default_equals_new() {
        assert_eq!(DevelopmentState::default(), DevelopmentState::new());
    }

    #[test]
    fn development_state_transition_to_records_log() {
        let mut state = DevelopmentState::new();
        state
            .transition_to(DevelopmentPhase::Development, "2026-03-15T00:00:00Z")
            .unwrap();

        assert_eq!(state.phase, DevelopmentPhase::Development);
        assert_eq!(
            state.last_transition_at.as_deref(),
            Some("2026-03-15T00:00:00Z")
        );
        assert_eq!(state.transition_log.len(), 1);
        assert_eq!(state.transition_log[0].from, DevelopmentPhase::Init);
        assert_eq!(state.transition_log[0].to, DevelopmentPhase::Development);
    }

    #[test]
    fn development_state_transition_to_rejects_invalid() {
        let mut state = DevelopmentState::new();
        let result = state.transition_to(DevelopmentPhase::Testing, "2026-03-15T00:00:00Z");
        assert!(result.is_err());
        assert_eq!(state.phase, DevelopmentPhase::Init); // unchanged
    }

    #[test]
    fn development_state_update_init_step() {
        let mut state = DevelopmentState::new();
        state.update_init_step("create-git-repo");
        assert_eq!(state.init_step.as_deref(), Some("create-git-repo"));
    }

    #[test]
    fn development_state_auto_advance_from_init_when_complete() {
        let mut state = DevelopmentState::new();
        state.update_init_step("complete");
        let advanced = state.auto_advance_from_init("2026-03-15T00:00:00Z");
        assert!(advanced);
        assert_eq!(state.phase, DevelopmentPhase::Development);
    }

    #[test]
    fn development_state_auto_advance_from_init_not_complete() {
        let mut state = DevelopmentState::new();
        state.update_init_step("verify-setup");
        let advanced = state.auto_advance_from_init("2026-03-15T00:00:00Z");
        assert!(!advanced);
        assert_eq!(state.phase, DevelopmentPhase::Init);
    }

    #[test]
    fn development_state_auto_advance_from_non_init_phase() {
        let mut state = DevelopmentState::new();
        state
            .transition_to(DevelopmentPhase::Development, "2026-03-15T00:00:00Z")
            .unwrap();
        state.update_init_step("complete");
        let advanced = state.auto_advance_from_init("2026-03-15T01:00:00Z");
        assert!(!advanced);
        assert_eq!(state.phase, DevelopmentPhase::Development);
    }

    #[test]
    fn development_state_re_entry_to_init_from_development() {
        let mut state = DevelopmentState::new();
        state
            .transition_to(DevelopmentPhase::Development, "t1")
            .unwrap();
        state.transition_to(DevelopmentPhase::Init, "t2").unwrap();
        assert_eq!(state.phase, DevelopmentPhase::Init);
        assert_eq!(state.transition_log.len(), 2);
    }

    #[test]
    fn development_state_re_entry_to_init_from_testing() {
        let mut state = DevelopmentState::new();
        state
            .transition_to(DevelopmentPhase::Development, "t1")
            .unwrap();
        state
            .transition_to(DevelopmentPhase::Testing, "t2")
            .unwrap();
        state.transition_to(DevelopmentPhase::Init, "t3").unwrap();
        assert_eq!(state.phase, DevelopmentPhase::Init);
        assert_eq!(state.transition_log.len(), 3);
    }

    #[test]
    fn development_state_transition_clears_init_step() {
        let mut state = DevelopmentState::new();
        state.update_init_step("verify-setup");
        state
            .transition_to(DevelopmentPhase::Development, "t1")
            .unwrap();
        assert!(state.init_step.is_none());
    }

    #[test]
    fn development_state_serializes_and_deserializes() {
        let mut state = DevelopmentState::new();
        state.update_init_step("complete");
        state.auto_advance_from_init("2026-03-15T00:00:00Z");

        let json = serde_json::to_string_pretty(&state).unwrap();
        let decoded: DevelopmentState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, state);
    }

    #[test]
    fn development_state_save_and_load_round_trip() {
        let tmp = std::env::temp_dir().join("calypso-dev-state-test.json");
        let mut state = DevelopmentState::new();
        state
            .transition_to(DevelopmentPhase::Development, "t1")
            .unwrap();

        state.save_to_path(&tmp).unwrap();
        let loaded = DevelopmentState::load_from_path(&tmp).unwrap();
        assert_eq!(loaded, state);

        let _ = std::fs::remove_file(&tmp);
    }

    #[test]
    fn development_state_load_from_missing_file_returns_error() {
        let result =
            DevelopmentState::load_from_path(std::path::Path::new("/nonexistent/path.json"));
        assert!(result.is_err());
    }

    // ── validate_transition error branches ────────────────────────────────

    #[test]
    fn validate_transition_same_phase_development_says_already() {
        let result =
            DevelopmentPhase::Development.validate_transition(&DevelopmentPhase::Development);
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("already in this phase"),
            "expected 'already in this phase': {err}",
        );
    }

    #[test]
    fn validate_transition_same_phase_testing_says_already() {
        let result = DevelopmentPhase::Testing.validate_transition(&DevelopmentPhase::Testing);
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("already in this phase"),
            "expected 'already in this phase': {err}",
        );
    }

    #[test]
    fn validate_transition_init_to_init_generic_fallback() {
        let result = DevelopmentPhase::Init.validate_transition(&DevelopmentPhase::Init);
        let err = result.unwrap_err();
        assert!(
            err.to_string().contains("is not permitted"),
            "expected generic fallback message: {err}",
        );
    }

    #[test]
    fn validate_transition_init_to_testing_specific_reason() {
        let result = DevelopmentPhase::Init.validate_transition(&DevelopmentPhase::Testing);
        let err = result.unwrap_err();
        assert!(
            err.to_string()
                .contains("init must complete before entering testing"),
            "expected init-to-testing reason: {err}",
        );
    }

    #[test]
    fn development_transition_error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(DevelopmentTransitionError::Rejected {
            from: DevelopmentPhase::Init,
            to: DevelopmentPhase::Testing,
            reason: "test".to_string(),
        });
        assert!(err.to_string().contains("cannot transition"));
    }

    // ── DevelopmentState re-entry to init clears init_step ────────────────

    #[test]
    fn development_state_re_entry_to_init_clears_init_step() {
        let mut state = DevelopmentState::new();
        state.update_init_step("verify-setup");
        state
            .transition_to(DevelopmentPhase::Development, "t1")
            .unwrap();
        assert!(
            state.init_step.is_none(),
            "init_step should be cleared after leaving Init"
        );
        state.transition_to(DevelopmentPhase::Init, "t2").unwrap();
        assert!(
            state.init_step.is_none(),
            "init_step should be None on re-entry to Init"
        );
    }
}
