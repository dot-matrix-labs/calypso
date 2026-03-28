//! Methodology template types and loading — re-exported from `calypso-templates`.
pub use calypso_templates::{
    AgentCatalog, AgentTask, AgentTaskKind, ArtifactPolicies, DoctorCheckConfig, FeatureUnitConfig,
    GateGroupTemplate, GateStatus, GateTemplate, OnConfig, PolicyGateKind, PolicyGateTemplate,
    PromptCatalog, StateConfig, StateDefinition, StateMachineTemplate, StepType, TemplateError,
    TemplateSet, TimeoutPolicy, TransitionTemplate, WaiverPolicy, DEFAULT_AGENTS_YAML,
    DEFAULT_PROMPTS_YAML, DEFAULT_STATE_MACHINE_YAML, load_embedded_template_set,
    load_project_template_set, resolve_template_set_for_path,
};
