use calypso_cli::template::{
    AgentTaskKind, TemplateError, TemplateSet, load_embedded_template_set,
    resolve_template_set_for_path,
};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const VALID_STATE_MACHINE: &str = r#"
initial_state: new
states:
  - new
  - implementation
  - ready-for-review
gate_groups:
  - id: specification
    label: Specification
    gates:
      - id: pr-canonicalized
        label: PR canonicalized
        task: pr-editor
  - id: validation
    label: Validation
    gates:
      - id: rust-quality-green
        label: Rust quality green
        task: rust-quality
"#;

const VALID_AGENTS: &str = r#"
tasks:
  - name: pr-editor
    kind: agent
    role: pr-editor
  - name: rust-quality
    kind: builtin
    builtin: builtin.ci.rust_quality_green
"#;

const VALID_PROMPTS: &str = r#"
prompts:
  pr-editor: |
    Keep the pull request description aligned with the current feature state.
"#;

static TEMPLATE_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

fn temp_template_dir() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let counter = TEMPLATE_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "calypso-template-test-{}-{unique}-{counter}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("temp template directory should be created");
    path
}

fn write_override_templates(root: &Path) {
    fs::write(root.join("calypso-state-machine.yml"), VALID_STATE_MACHINE)
        .expect("state machine override should write");
    fs::write(root.join("calypso-agents.yml"), VALID_AGENTS).expect("agents override should write");
    fs::write(root.join("calypso-prompts.yml"), VALID_PROMPTS)
        .expect("prompts override should write");
}

#[test]
fn template_set_parses_and_validates_across_split_yaml_files() {
    let template = TemplateSet::from_yaml_strings(VALID_STATE_MACHINE, VALID_AGENTS, VALID_PROMPTS)
        .expect("template should parse and validate");

    assert_eq!(template.state_machine.initial_state, "new");
    assert_eq!(template.state_machine.gate_groups.len(), 2);
    assert_eq!(template.agents.tasks.len(), 2);
    assert_eq!(template.agents.tasks[0].kind, AgentTaskKind::Agent);
    assert_eq!(
        template.prompts.prompts["pr-editor"],
        "Keep the pull request description aligned with the current feature state.\n"
    );
}

#[test]
fn template_validation_rejects_unknown_gate_task_reference() {
    let error = TemplateSet::from_yaml_strings(
        &VALID_STATE_MACHINE.replace("rust-quality", "missing-task"),
        VALID_AGENTS,
        VALID_PROMPTS,
    )
    .expect_err("unknown task should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("missing-task"));
}

#[test]
fn template_validation_requires_prompts_for_agent_tasks() {
    let error = TemplateSet::from_yaml_strings(VALID_STATE_MACHINE, VALID_AGENTS, "prompts: {}\n")
        .expect_err("agent task without prompt should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("pr-editor"));
}

#[test]
fn template_validation_requires_builtin_keyword_for_builtin_tasks() {
    let invalid_agents = VALID_AGENTS.replace("builtin.ci.rust_quality_green", "rust-quality");

    let error = TemplateSet::from_yaml_strings(VALID_STATE_MACHINE, &invalid_agents, VALID_PROMPTS)
        .expect_err("builtin task without builtin keyword should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("builtin."));
}

#[test]
fn embedded_default_template_set_loads_and_validates() {
    let template = load_embedded_template_set().expect("embedded defaults should load");

    assert!(!template.state_machine.states.is_empty());
    assert!(!template.state_machine.gate_groups.is_empty());
    assert!(!template.agents.tasks.is_empty());
}

#[test]
fn template_validation_requires_at_least_one_state() {
    let invalid_state_machine = VALID_STATE_MACHINE.replace(
        "states:\n  - new\n  - implementation\n  - ready-for-review\n",
        "states: []\n",
    );

    let error = TemplateSet::from_yaml_strings(&invalid_state_machine, VALID_AGENTS, VALID_PROMPTS)
        .expect_err("missing states should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("at least one state"));
}

#[test]
fn template_validation_requires_initial_state_to_exist() {
    let invalid_state_machine =
        VALID_STATE_MACHINE.replace("initial_state: new", "initial_state: blocked");

    let error = TemplateSet::from_yaml_strings(&invalid_state_machine, VALID_AGENTS, VALID_PROMPTS)
        .expect_err("unknown initial state should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("initial state"));
}

#[test]
fn template_validation_requires_at_least_one_gate_group() {
    let invalid_state_machine = VALID_STATE_MACHINE.replace(
        "gate_groups:\n  - id: specification\n    label: Specification\n    gates:\n      - id: pr-canonicalized\n        label: PR canonicalized\n        task: pr-editor\n  - id: validation\n    label: Validation\n    gates:\n      - id: rust-quality-green\n        label: Rust quality green\n        task: rust-quality\n",
        "gate_groups: []\n",
    );

    let error = TemplateSet::from_yaml_strings(&invalid_state_machine, VALID_AGENTS, VALID_PROMPTS)
        .expect_err("missing gate groups should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("at least one gate group"));
}

#[test]
fn template_validation_rejects_empty_gate_group() {
    let invalid_state_machine = VALID_STATE_MACHINE.replace(
        "gates:\n      - id: pr-canonicalized\n        label: PR canonicalized\n        task: pr-editor\n",
        "gates: []\n",
    );

    let error = TemplateSet::from_yaml_strings(&invalid_state_machine, VALID_AGENTS, VALID_PROMPTS)
        .expect_err("empty gate group should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("must contain at least one gate"));
}

#[test]
fn template_validation_requires_builtin_evaluator_value() {
    let invalid_agents = VALID_AGENTS.replace(
        "  - name: rust-quality\n    kind: builtin\n    builtin: builtin.ci.rust_quality_green\n",
        "  - name: rust-quality\n    kind: builtin\n",
    );

    let error = TemplateSet::from_yaml_strings(VALID_STATE_MACHINE, &invalid_agents, VALID_PROMPTS)
        .expect_err("builtin task without evaluator should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(
        error
            .to_string()
            .contains("must define a builtin evaluator")
    );
}

#[test]
fn template_validation_rejects_prompt_without_matching_task() {
    let invalid_prompts = r#"
prompts:
  pr-editor: |
    Keep the pull request description aligned with the current feature state.
  unknown-task: |
    This should not exist.
"#;

    let error = TemplateSet::from_yaml_strings(VALID_STATE_MACHINE, VALID_AGENTS, invalid_prompts)
        .expect_err("stray prompt should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("unknown-task"));
}

#[test]
fn template_validation_accepts_human_and_hook_tasks_without_prompts() {
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: coordination
    label: Coordination
    gates:
      - id: human-approval
        label: Human approval
        task: human-approval
      - id: push-sync
        label: Push sync
        task: push-sync
"#;
    let agents = r#"
tasks:
  - name: human-approval
    kind: human
  - name: push-sync
    kind: hook
"#;

    let template = TemplateSet::from_yaml_strings(state_machine, agents, "prompts: {}\n")
        .expect("human and hook tasks should validate");

    assert_eq!(template.agents.tasks[0].kind, AgentTaskKind::Human);
    assert_eq!(template.agents.tasks[1].kind, AgentTaskKind::Hook);
}

#[test]
fn template_error_formats_yaml_failures() {
    let error = TemplateSet::from_yaml_strings(":", VALID_AGENTS, VALID_PROMPTS)
        .expect_err("invalid YAML should fail parsing");

    assert!(matches!(error, TemplateError::Yaml(_)));
    assert!(error.to_string().contains("template YAML error"));
}

#[test]
fn template_resolution_falls_back_to_embedded_defaults_when_no_local_files_exist() {
    let temp_dir = temp_template_dir();

    let template = resolve_template_set_for_path(&temp_dir)
        .expect("embedded template set should load without local overrides");

    assert_eq!(template.state_machine.initial_state, "new");
    assert!(
        template
            .state_machine
            .gate_groups
            .iter()
            .any(|group| group.id == "merge-readiness")
    );

    fs::remove_dir_all(temp_dir).expect("temp template directory should be removed");
}

#[test]
fn template_resolution_prefers_complete_local_override_set() {
    let temp_dir = temp_template_dir();
    write_override_templates(&temp_dir);

    let template =
        resolve_template_set_for_path(&temp_dir).expect("local override template set should load");

    assert_eq!(template.state_machine.gate_groups.len(), 2);
    assert_eq!(template.agents.tasks.len(), 2);
    assert_eq!(template.prompts.prompts.len(), 1);

    fs::remove_dir_all(temp_dir).expect("temp template directory should be removed");
}

#[test]
fn template_resolution_rejects_partial_local_override_set() {
    let temp_dir = temp_template_dir();
    fs::write(
        temp_dir.join("calypso-state-machine.yml"),
        VALID_STATE_MACHINE,
    )
    .expect("partial override should write");

    let error = resolve_template_set_for_path(&temp_dir)
        .expect_err("partial local override should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("calypso-agents.yml"));
    assert!(error.to_string().contains("calypso-prompts.yml"));

    fs::remove_dir_all(temp_dir).expect("temp template directory should be removed");
}

#[test]
fn template_resolution_reports_local_io_failures() {
    let temp_dir = temp_template_dir();
    fs::write(
        temp_dir.join("calypso-state-machine.yml"),
        VALID_STATE_MACHINE,
    )
    .expect("state machine override should write");
    fs::write(temp_dir.join("calypso-agents.yml"), VALID_AGENTS)
        .expect("agents override should write");
    fs::create_dir(temp_dir.join("calypso-prompts.yml"))
        .expect("prompts path directory should be created");

    let error = resolve_template_set_for_path(&temp_dir)
        .expect_err("unreadable local override should fail with an I/O error");

    assert!(matches!(error, TemplateError::Io(_)));
    assert!(error.to_string().contains("template I/O error"));

    fs::remove_dir_all(temp_dir).expect("temp template directory should be removed");
}
