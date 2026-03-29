use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

pub const DEFAULT_STATE_MACHINE_YAML: &str = include_str!("../templates/default/state-machine.yml");
pub const DEFAULT_AGENTS_YAML: &str = include_str!("../templates/default/agents.yml");
pub const DEFAULT_PROMPTS_YAML: &str = include_str!("../templates/default/prompts.yml");
const LOCAL_STATE_MACHINE_FILE: &str = "calypso-state-machine.yml";
const LOCAL_AGENTS_FILE: &str = "calypso-agents.yml";
const LOCAL_PROMPTS_FILE: &str = "calypso-prompts.yml";
const DOT_CALYPSO_STATE_MACHINE_FILE: &str = ".calypso/state-machine.yml";
const DOT_CALYPSO_AGENTS_FILE: &str = ".calypso/agents.yml";
const DOT_CALYPSO_PROMPTS_FILE: &str = ".calypso/prompts.yml";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TemplateSet {
    pub state_machine: StateMachineTemplate,
    pub agents: AgentCatalog,
    pub prompts: PromptCatalog,
}

impl TemplateSet {
    pub fn from_yaml_strings(
        state_machine_yaml: &str,
        agents_yaml: &str,
        prompts_yaml: &str,
    ) -> Result<Self, TemplateError> {
        let template = Self {
            state_machine: serde_yaml::from_str(state_machine_yaml).map_err(TemplateError::Yaml)?,
            agents: serde_yaml::from_str(agents_yaml).map_err(TemplateError::Yaml)?,
            prompts: serde_yaml::from_str(prompts_yaml).map_err(TemplateError::Yaml)?,
        };

        template.validate()?;
        Ok(template)
    }

    /// Load template from a project directory, merging `.calypso/` overrides over defaults.
    ///
    /// For each of `state-machine.yml`, `agents.yml`, and `prompts.yml`, if a local file exists
    /// under `.calypso/`, its top-level keys override the corresponding defaults. Missing local
    /// files simply use the default entirely. After merging, the combined result is validated.
    pub fn load_from_directory(path: &Path) -> Result<Self, TemplateError> {
        let sm_path = path.join(DOT_CALYPSO_STATE_MACHINE_FILE);
        let agents_path = path.join(DOT_CALYPSO_AGENTS_FILE);
        let prompts_path = path.join(DOT_CALYPSO_PROMPTS_FILE);

        let state_machine_yaml = if sm_path.exists() {
            let local = fs::read_to_string(&sm_path).map_err(TemplateError::Io)?;
            merge_yaml_strings(DEFAULT_STATE_MACHINE_YAML, &local)?
        } else {
            DEFAULT_STATE_MACHINE_YAML.to_string()
        };

        let agents_yaml = if agents_path.exists() {
            let local = fs::read_to_string(&agents_path).map_err(TemplateError::Io)?;
            merge_yaml_strings(DEFAULT_AGENTS_YAML, &local)?
        } else {
            DEFAULT_AGENTS_YAML.to_string()
        };

        let prompts_yaml = if prompts_path.exists() {
            let local = fs::read_to_string(&prompts_path).map_err(TemplateError::Io)?;
            merge_yaml_strings(DEFAULT_PROMPTS_YAML, &local)?
        } else {
            DEFAULT_PROMPTS_YAML.to_string()
        };

        Self::from_yaml_strings(&state_machine_yaml, &agents_yaml, &prompts_yaml)
    }

    fn validate(&self) -> Result<(), TemplateError> {
        if self.state_machine.states.is_empty() {
            return Err(TemplateError::Validation(
                "state machine must define at least one state".to_string(),
            ));
        }

        if !self
            .state_machine
            .states
            .iter()
            .any(|state| state.name() == self.state_machine.initial_state)
        {
            return Err(TemplateError::Validation(format!(
                "initial state '{}' is not present in states",
                self.state_machine.initial_state
            )));
        }

        if self.state_machine.gate_groups.is_empty() {
            return Err(TemplateError::Validation(
                "state machine must define at least one gate group".to_string(),
            ));
        }

        let tasks_by_name: BTreeMap<&str, &AgentTask> = self
            .agents
            .tasks
            .iter()
            .map(|task| (task.name.as_str(), task))
            .collect();

        for group in &self.state_machine.gate_groups {
            if group.gates.is_empty() {
                return Err(TemplateError::Validation(format!(
                    "gate group '{}' must contain at least one gate",
                    group.id
                )));
            }

            for gate in &group.gates {
                let task = tasks_by_name.get(gate.task.as_str()).ok_or_else(|| {
                    TemplateError::Validation(format!(
                        "gate '{}' references unknown task '{}'",
                        gate.id, gate.task
                    ))
                })?;

                match task.kind {
                    AgentTaskKind::Agent => {
                        if !self.prompts.prompts.contains_key(task.name.as_str()) {
                            return Err(TemplateError::Validation(format!(
                                "agent task '{}' requires a prompt entry",
                                task.name
                            )));
                        }
                    }
                    AgentTaskKind::Builtin => {
                        let builtin = task.builtin.as_deref().ok_or_else(|| {
                            TemplateError::Validation(format!(
                                "builtin task '{}' must define a builtin evaluator",
                                task.name
                            ))
                        })?;

                        if !builtin.starts_with("builtin.") {
                            return Err(TemplateError::Validation(format!(
                                "builtin task '{}' must use a reserved builtin.* evaluator keyword",
                                task.name
                            )));
                        }
                    }
                    AgentTaskKind::Human | AgentTaskKind::Hook => {}
                }
            }
        }

        let gates_by_id: BTreeMap<&str, &GateTemplate> = self
            .state_machine
            .gate_groups
            .iter()
            .flat_map(|group| group.gates.iter())
            .map(|gate| (gate.id.as_str(), gate))
            .collect();

        for policy_gate in &self.state_machine.policy_gates {
            let gate = gates_by_id
                .get(policy_gate.gate_id.as_str())
                .ok_or_else(|| {
                    TemplateError::Validation(format!(
                        "policy gate '{}' references unknown gate '{}'",
                        policy_gate.evaluator, policy_gate.gate_id
                    ))
                })?;

            let task = tasks_by_name
                .get(gate.task.as_str())
                .expect("task referenced by policy gate was already validated in gate_groups loop");

            if task.kind != AgentTaskKind::Builtin {
                return Err(TemplateError::Validation(format!(
                    "policy gate '{}' must bind to a builtin task",
                    policy_gate.gate_id
                )));
            }

            let builtin = task
                .builtin
                .as_deref()
                .expect("builtin field was already validated in gate_groups loop");

            if builtin != policy_gate.evaluator {
                return Err(TemplateError::Validation(format!(
                    "policy gate '{}' evaluator '{}' does not match task builtin '{}'",
                    policy_gate.gate_id, policy_gate.evaluator, builtin
                )));
            }

            validate_policy_gate(policy_gate)?;
        }

        let known_task_names: BTreeSet<&str> = self
            .agents
            .tasks
            .iter()
            .map(|task| task.name.as_str())
            .collect();

        for prompt_name in self.prompts.prompts.keys() {
            if !known_task_names.contains(prompt_name.as_str()) {
                return Err(TemplateError::Validation(format!(
                    "prompt '{prompt_name}' does not match any known task"
                )));
            }
        }

        Ok(())
    }

    /// Check coherence of cross-references within the template set.
    ///
    /// Returns a list of human-readable error strings. An empty list means the template is
    /// coherent. This is a softer check than `validate` — it does not parse YAML but instead
    /// inspects the already-parsed structures for logical consistency.
    pub fn validate_coherence(&self) -> Vec<String> {
        let mut errors = Vec::new();

        let known_task_names: BTreeSet<&str> = self
            .agents
            .tasks
            .iter()
            .map(|task| task.name.as_str())
            .collect();

        let known_states: BTreeSet<&str> =
            self.state_machine.states.iter().map(|s| s.name()).collect();

        // Known builtin prefixes accepted by the system
        const KNOWN_BUILTIN_PREFIXES: &[&str] = &[
            "builtin.doctor.",
            "builtin.github.",
            "builtin.policy.",
            "builtin.git.",
            "builtin.ci.",
        ];

        // Check gate task references and builtin keyword validity
        for group in &self.state_machine.gate_groups {
            for gate in &group.gates {
                if !known_task_names.contains(gate.task.as_str()) {
                    errors.push(format!(
                        "gate '{}' references non-existent task '{}'",
                        gate.id, gate.task
                    ));
                }

                // Check applies_to states exist
                if let Some(applies_to) = &gate.applies_to {
                    for state in applies_to {
                        if !known_states.contains(state.as_str()) {
                            errors.push(format!(
                                "gate '{}' applies_to references non-existent state '{}'",
                                gate.id, state
                            ));
                        }
                    }
                }

                // Check blocking_scope references a known state if set
                if let Some(scope) = &gate.blocking_scope
                    && !known_states.contains(scope.as_str())
                {
                    errors.push(format!(
                        "gate '{}' blocking_scope references non-existent state '{}'",
                        gate.id, scope
                    ));
                }
            }
        }

        // Check builtin keyword references
        for task in &self.agents.tasks {
            if task.kind == AgentTaskKind::Builtin
                && let Some(builtin) = &task.builtin
            {
                let recognized = KNOWN_BUILTIN_PREFIXES
                    .iter()
                    .any(|prefix| builtin.starts_with(prefix));
                if !recognized {
                    errors.push(format!(
                        "task '{}' uses unrecognized builtin keyword '{}'",
                        task.name, builtin
                    ));
                }
            }
        }

        // Check that transition target states exist (if state_machine has transitions)
        for transition in &self.state_machine.transitions {
            if !known_states.contains(transition.from.as_str()) {
                errors.push(format!(
                    "transition from '{}' references non-existent state",
                    transition.from
                ));
            }
            if !known_states.contains(transition.to.as_str()) {
                errors.push(format!(
                    "transition to '{}' references non-existent state",
                    transition.to
                ));
            }
        }

        errors
    }

    pub fn task_by_name(&self, task_name: &str) -> Option<&AgentTask> {
        self.agents.tasks.iter().find(|task| task.name == task_name)
    }

    /// Returns the `StepType` for the named state, defaulting to `Agent` if unknown.
    pub fn step_type_for_state(&self, state_name: &str) -> StepType {
        self.state_machine
            .states
            .iter()
            .find(|s| s.name() == state_name)
            .map(|s| s.step_type())
            .unwrap_or(StepType::Agent)
    }

    /// Returns the function name bound to the named state, or `None` if not a function step.
    pub fn function_for_state(&self, state_name: &str) -> Option<String> {
        self.state_machine
            .states
            .iter()
            .find(|s| s.name() == state_name)
            .and_then(|s| match s {
                StateDefinition::Detailed(c) => c.function.clone(),
                StateDefinition::Simple(_) => None,
            })
    }
}

/// Merge two YAML documents, with keys from `override_yaml` taking precedence over `base_yaml`.
/// Both documents must be YAML mappings at the top level.
fn merge_yaml_strings(base_yaml: &str, override_yaml: &str) -> Result<String, TemplateError> {
    let mut base: serde_yaml::Value =
        serde_yaml::from_str(base_yaml).map_err(TemplateError::Yaml)?;
    let overlay: serde_yaml::Value =
        serde_yaml::from_str(override_yaml).map_err(TemplateError::Yaml)?;

    if let (serde_yaml::Value::Mapping(base_map), serde_yaml::Value::Mapping(overlay_map)) =
        (&mut base, overlay)
    {
        for (k, v) in overlay_map {
            base_map.insert(k, v);
        }
    }

    serde_yaml::to_string(&base).map_err(TemplateError::Yaml)
}

/// Load a project's `.calypso/` templates directly, without merging with embedded defaults.
///
/// Falls back to the embedded defaults if none of the project files exist.
pub fn load_project_template_set(repo_root: &Path) -> Result<TemplateSet, TemplateError> {
    let sm_path = repo_root.join(DOT_CALYPSO_STATE_MACHINE_FILE);
    let agents_path = repo_root.join(DOT_CALYPSO_AGENTS_FILE);
    let prompts_path = repo_root.join(DOT_CALYPSO_PROMPTS_FILE);

    let all_exist = sm_path.exists() && agents_path.exists() && prompts_path.exists();
    if !all_exist {
        return load_embedded_template_set();
    }

    let state_machine_yaml = fs::read_to_string(&sm_path).map_err(TemplateError::Io)?;
    let agents_yaml = fs::read_to_string(&agents_path).map_err(TemplateError::Io)?;
    let prompts_yaml = fs::read_to_string(&prompts_path).map_err(TemplateError::Io)?;

    TemplateSet::from_yaml_strings(&state_machine_yaml, &agents_yaml, &prompts_yaml)
}

pub fn load_embedded_template_set() -> Result<TemplateSet, TemplateError> {
    TemplateSet::from_yaml_strings(
        DEFAULT_STATE_MACHINE_YAML,
        DEFAULT_AGENTS_YAML,
        DEFAULT_PROMPTS_YAML,
    )
}

/// Load a TemplateSet using a custom state-machine YAML file with embedded defaults for agents
/// and prompts.
///
/// Use this when the user selects a specific workflow file via `--select-flow`.
pub fn load_template_set_with_state_machine(
    state_machine_path: &Path,
) -> Result<TemplateSet, TemplateError> {
    let state_machine_yaml = fs::read_to_string(state_machine_path).map_err(TemplateError::Io)?;
    TemplateSet::from_yaml_strings(
        &state_machine_yaml,
        DEFAULT_AGENTS_YAML,
        DEFAULT_PROMPTS_YAML,
    )
}

pub fn resolve_template_set_for_path(root: &Path) -> Result<TemplateSet, TemplateError> {
    let state_machine_path = root.join(LOCAL_STATE_MACHINE_FILE);
    let agents_path = root.join(LOCAL_AGENTS_FILE);
    let prompts_path = root.join(LOCAL_PROMPTS_FILE);

    let local_files = [
        state_machine_path.as_path(),
        agents_path.as_path(),
        prompts_path.as_path(),
    ];
    let existing_count = local_files.iter().filter(|path| path.exists()).count();

    if existing_count == 0 {
        return load_embedded_template_set();
    }

    if existing_count != local_files.len() {
        let missing_files: Vec<&str> = [
            (state_machine_path.exists(), LOCAL_STATE_MACHINE_FILE),
            (agents_path.exists(), LOCAL_AGENTS_FILE),
            (prompts_path.exists(), LOCAL_PROMPTS_FILE),
        ]
        .into_iter()
        .filter_map(|(exists, name)| (!exists).then_some(name))
        .collect();

        return Err(TemplateError::Validation(format!(
            "local methodology override is incomplete; missing files: {}",
            missing_files.join(", ")
        )));
    }

    let state_machine_yaml = fs::read_to_string(&state_machine_path).map_err(TemplateError::Io)?;
    let agents_yaml = fs::read_to_string(&agents_path).map_err(TemplateError::Io)?;
    let prompts_yaml = fs::read_to_string(&prompts_path).map_err(TemplateError::Io)?;

    TemplateSet::from_yaml_strings(&state_machine_yaml, &agents_yaml, &prompts_yaml)
}

/// A state definition in the state machine template.
///
/// Supports two forms in YAML:
/// - Simple: `- my-state` (bare string, defaults to agent step type)
/// - Detailed: `- name: my-state\n  type: function\n  function: my_fn`
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StateDefinition {
    Simple(String),
    Detailed(StateConfig),
}

impl StateDefinition {
    pub fn name(&self) -> &str {
        match self {
            Self::Simple(s) => s.as_str(),
            Self::Detailed(c) => c.name.as_str(),
        }
    }

    pub fn step_type(&self) -> StepType {
        match self {
            Self::Simple(_) => StepType::Agent,
            Self::Detailed(c) => c.step_type.clone(),
        }
    }
}

/// Detailed state configuration used when a state needs explicit step type or function binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateConfig {
    pub name: String,
    #[serde(rename = "type", default)]
    pub step_type: StepType,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub function: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
}

/// Whether a state is driven by a built-in function or a supervised agent session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum StepType {
    #[default]
    Agent,
    Function,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateMachineTemplate {
    pub initial_state: String,
    pub states: Vec<StateDefinition>,
    pub gate_groups: Vec<GateGroupTemplate>,
    #[serde(default)]
    pub policy_gates: Vec<PolicyGateTemplate>,
    #[serde(default)]
    pub transitions: Vec<TransitionTemplate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_unit: Option<FeatureUnitConfig>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub artifact_policies: Option<ArtifactPolicies>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub on: Option<OnConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OnConfig {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cron: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransitionTemplate {
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FeatureUnitConfig {
    pub branch_prefix: Option<String>,
    pub worktree_base: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ArtifactPolicies {
    /// Map from state name to list of required artifact paths/patterns
    #[serde(default)]
    pub required_per_state: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateGroupTemplate {
    pub id: String,
    pub label: String,
    pub gates: Vec<GateTemplate>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GateTemplate {
    pub id: String,
    pub label: String,
    pub task: String,
    // FUTURE: #72 — parsed but not yet enforced by the workflow engine
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub recheck_trigger: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_policy: Option<TimeoutPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub waiver_policy: Option<WaiverPolicy>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auto_open_task_on_fail: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pr_checklist_label: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_parallel_with: Option<Vec<String>>,
    // FUTURE: #72 — parsed and validated but not yet enforced by the workflow engine
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blocking_scope: Option<String>,
    // FUTURE: #72 — parsed and validated but not yet enforced by the workflow engine
    #[serde(skip_serializing_if = "Option::is_none")]
    pub applies_to: Option<Vec<String>>,
    // FUTURE: #72 — parsed but not yet enforced by the workflow engine
    // #[serde(skip_serializing_if = "Option::is_none")]
    // pub status_source: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeoutPolicy {
    pub duration_secs: u64,
    pub on_timeout: GateStatus,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GateStatus {
    Open,
    Closed,
    Skipped,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaiverPolicy {
    pub requires_role: String,
    pub recorded_in_state: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PolicyGateTemplate {
    pub gate_id: String,
    pub evaluator: String,
    pub kind: PolicyGateKind,
    #[serde(default)]
    pub paths: Vec<String>,
    #[serde(default)]
    pub watched_paths: Vec<String>,
    #[serde(default)]
    pub skip_on_tag_push: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PolicyGateKind {
    Hook,
    Workflow,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCatalog {
    pub tasks: Vec<AgentTask>,
    #[serde(default)]
    pub doctor_checks: Vec<DoctorCheckConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DoctorCheckConfig {
    pub id: String,
    pub label: String,
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTask {
    pub name: String,
    pub kind: AgentTaskKind,
    pub role: Option<String>,
    pub builtin: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AgentTaskKind {
    Agent,
    Builtin,
    Human,
    Hook,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PromptCatalog {
    pub prompts: BTreeMap<String, String>,
}

#[derive(Debug)]
pub enum TemplateError {
    Io(std::io::Error),
    Yaml(serde_yaml::Error),
    Validation(String),
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TemplateError::Io(error) => write!(f, "template I/O error: {error}"),
            TemplateError::Yaml(error) => write!(f, "template YAML error: {error}"),
            TemplateError::Validation(error) => write!(f, "template validation error: {error}"),
        }
    }
}

impl std::error::Error for TemplateError {}

fn validate_policy_gate(policy_gate: &PolicyGateTemplate) -> Result<(), TemplateError> {
    match policy_gate.evaluator.as_str() {
        "builtin.policy.implementation_plan_present"
        | "builtin.policy.next_prompt_present"
        | "builtin.policy.required_workflows_present" => {
            if policy_gate.paths.is_empty() {
                return Err(TemplateError::Validation(format!(
                    "policy gate '{}' must define at least one path",
                    policy_gate.gate_id
                )));
            }

            if !policy_gate.watched_paths.is_empty() {
                return Err(TemplateError::Validation(format!(
                    "policy gate '{}' does not allow watched_paths",
                    policy_gate.gate_id
                )));
            }
        }
        "builtin.policy.implementation_plan_fresh" => {
            if policy_gate.paths.len() != 1 {
                return Err(TemplateError::Validation(format!(
                    "policy gate '{}' must define exactly one primary path",
                    policy_gate.gate_id
                )));
            }

            if policy_gate.watched_paths.is_empty() {
                return Err(TemplateError::Validation(format!(
                    "policy gate '{}' must define at least one watched path",
                    policy_gate.gate_id
                )));
            }
        }
        "builtin.git.is_main_compatible" => {
            if !policy_gate.paths.is_empty() || !policy_gate.watched_paths.is_empty() {
                return Err(TemplateError::Validation(format!(
                    "policy gate '{}' does not accept file paths",
                    policy_gate.gate_id
                )));
            }
        }
        _ => {
            return Err(TemplateError::Validation(format!(
                "policy gate '{}' uses unsupported evaluator '{}'",
                policy_gate.gate_id, policy_gate.evaluator
            )));
        }
    }

    if policy_gate.kind == PolicyGateKind::Workflow && policy_gate.skip_on_tag_push {
        return Err(TemplateError::Validation(format!(
            "workflow policy gate '{}' cannot be tag-push exempt",
            policy_gate.gate_id
        )));
    }

    Ok(())
}
