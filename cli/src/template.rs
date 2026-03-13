use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

const DEFAULT_STATE_MACHINE_YAML: &str = include_str!("../templates/default/state-machine.yml");
const DEFAULT_AGENTS_YAML: &str = include_str!("../templates/default/agents.yml");
const DEFAULT_PROMPTS_YAML: &str = include_str!("../templates/default/prompts.yml");
const LOCAL_STATE_MACHINE_FILE: &str = "calypso-state-machine.yml";
const LOCAL_AGENTS_FILE: &str = "calypso-agents.yml";
const LOCAL_PROMPTS_FILE: &str = "calypso-prompts.yml";

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
            .any(|state| state == &self.state_machine.initial_state)
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

        let known_task_names: BTreeSet<&str> = self
            .agents
            .tasks
            .iter()
            .map(|task| task.name.as_str())
            .collect();

        for prompt_name in self.prompts.prompts.keys() {
            if !known_task_names.contains(prompt_name.as_str()) {
                return Err(TemplateError::Validation(format!(
                    "prompt '{}' does not match any known task",
                    prompt_name
                )));
            }
        }

        Ok(())
    }
}

pub fn load_embedded_template_set() -> Result<TemplateSet, TemplateError> {
    TemplateSet::from_yaml_strings(
        DEFAULT_STATE_MACHINE_YAML,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StateMachineTemplate {
    pub initial_state: String,
    pub states: Vec<String>,
    pub gate_groups: Vec<GateGroupTemplate>,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentCatalog {
    pub tasks: Vec<AgentTask>,
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
