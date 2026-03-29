//! Methodology template types and loading — re-exported from `calypso-templates`.
pub use calypso_templates::{
    AgentCatalog, AgentTask, AgentTaskKind, ArtifactPolicies, DEFAULT_AGENTS_YAML,
    DEFAULT_PROMPTS_YAML, DEFAULT_STATE_MACHINE_YAML, DoctorCheckConfig, FeatureUnitConfig,
    GateGroupTemplate, GateStatus, GateTemplate, OnConfig, PolicyGateKind, PolicyGateTemplate,
    PromptCatalog, StateConfig, StateDefinition, StateMachineTemplate, StepType, TemplateError,
    TemplateSet, TimeoutPolicy, TransitionTemplate, WaiverPolicy, load_embedded_template_set,
    load_project_template_set, load_template_set_with_state_machine, resolve_template_set_for_path,
};
