use calypso_cli::template::{
    AgentCatalog, AgentTask, AgentTaskKind, GateGroupTemplate, GateTemplate, PromptCatalog,
    StateDefinition, StateMachineTemplate, TemplateError, TemplateSet, TransitionTemplate,
    load_embedded_template_set, resolve_template_set_for_path,
};
#[allow(unused_imports)]
use calypso_cli::template::{GateStatus, TimeoutPolicy};
use std::collections::BTreeMap;
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
    assert!(!template.state_machine.policy_gates.is_empty());
    assert!(!template.agents.tasks.is_empty());
    assert!(
        template
            .state_machine
            .gate_groups
            .iter()
            .any(|group| group.id == "policy")
    );
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
fn template_validation_rejects_duplicate_state_names() {
    let invalid_state_machine = r#"
initial_state: new
states:
  - new
  - implementation
  - new
gate_groups:
  - id: validation
    label: Validation
    gates:
      - id: rust-quality-green
        label: Rust quality green
        task: rust-quality
"#;

    let error = TemplateSet::from_yaml_strings(invalid_state_machine, VALID_AGENTS, VALID_PROMPTS)
        .expect_err("duplicate states should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("duplicate state"));
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
fn template_validation_rejects_policy_gate_with_unknown_gate_reference() {
    let state_machine = format!(
        "{VALID_STATE_MACHINE}\npolicy_gates:\n  - gate_id: missing-gate\n    evaluator: builtin.policy.next_prompt_present\n    kind: hook\n    paths:\n      - docs/plans/next-prompt.md\n"
    );
    let agents = format!(
        "{VALID_AGENTS}  - name: next-prompt-present\n    kind: builtin\n    builtin: builtin.policy.next_prompt_present\n"
    );

    let error = TemplateSet::from_yaml_strings(&state_machine, &agents, VALID_PROMPTS)
        .expect_err("unknown policy gate should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("unknown gate"));
}

#[test]
fn template_validation_rejects_workflow_policy_gate_marked_tag_push_exempt() {
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: required-workflows-present
        label: Required workflows present
        task: required-workflows-present
policy_gates:
  - gate_id: required-workflows-present
    evaluator: builtin.policy.required_workflows_present
    kind: workflow
    skip_on_tag_push: true
    paths:
      - .github/workflows/rust-quality.yml
"#;
    let agents = r#"
tasks:
  - name: required-workflows-present
    kind: builtin
    builtin: builtin.policy.required_workflows_present
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, agents, "prompts: {}\n")
        .expect_err("tag-push-exempt workflow rule should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("tag-push exempt"));
}

#[test]
fn template_validation_rejects_stale_plan_rule_without_watched_paths() {
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: implementation-plan-fresh
        label: Implementation plan fresh
        task: implementation-plan-fresh
policy_gates:
  - gate_id: implementation-plan-fresh
    evaluator: builtin.policy.implementation_plan_fresh
    kind: hook
    paths:
      - docs/plans/implementation-plan.md
"#;
    let agents = r#"
tasks:
  - name: implementation-plan-fresh
    kind: builtin
    builtin: builtin.policy.implementation_plan_fresh
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, agents, "prompts: {}\n")
        .expect_err("stale-plan rule without watched paths should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("watched path"));
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

#[test]
fn template_validation_rejects_policy_gate_referencing_gate_with_unknown_task() {
    // Gate exists in gate_groups but its `task` field has no matching agent task.
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: impl-plan-present
        label: Implementation plan present
        task: no-such-task
policy_gates:
  - gate_id: impl-plan-present
    evaluator: builtin.policy.implementation_plan_present
    kind: hook
    paths:
      - docs/plans/implementation-plan.md
"#;
    let agents = r#"
tasks:
  - name: impl-plan-present
    kind: builtin
    builtin: builtin.policy.implementation_plan_present
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, agents, "prompts: {}\n")
        .expect_err("policy gate referencing gate with unknown task should fail");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("unknown task"));
}

#[test]
fn template_validation_rejects_policy_gate_bound_to_non_builtin_task() {
    // The gate's task exists but is an agent task, not a builtin.
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: impl-plan-present
        label: Implementation plan present
        task: impl-plan-agent
policy_gates:
  - gate_id: impl-plan-present
    evaluator: builtin.policy.implementation_plan_present
    kind: hook
    paths:
      - docs/plans/implementation-plan.md
"#;
    let agents = r#"
tasks:
  - name: impl-plan-agent
    kind: agent
    role: impl-plan-agent
"#;
    let prompts = r#"
prompts:
  impl-plan-agent: |
    Write the implementation plan.
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, agents, prompts)
        .expect_err("policy gate bound to non-builtin task should fail");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("must bind to a builtin task"));
}

#[test]
fn template_validation_rejects_policy_gate_with_evaluator_mismatch() {
    // The gate task has a different builtin evaluator than what the policy gate declares.
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: impl-plan-present
        label: Implementation plan present
        task: impl-plan-present
policy_gates:
  - gate_id: impl-plan-present
    evaluator: builtin.policy.next_prompt_present
    kind: hook
    paths:
      - docs/plans/next-prompt.md
"#;
    let agents = r#"
tasks:
  - name: impl-plan-present
    kind: builtin
    builtin: builtin.policy.implementation_plan_present
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, agents, "prompts: {}\n")
        .expect_err("policy gate evaluator mismatch should fail");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("does not match task builtin"));
}

#[test]
fn template_validation_rejects_present_evaluator_with_empty_paths() {
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: impl-plan-present
        label: Implementation plan present
        task: impl-plan-present
policy_gates:
  - gate_id: impl-plan-present
    evaluator: builtin.policy.implementation_plan_present
    kind: hook
    paths: []
"#;
    let agents = r#"
tasks:
  - name: impl-plan-present
    kind: builtin
    builtin: builtin.policy.implementation_plan_present
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, agents, "prompts: {}\n")
        .expect_err("present evaluator with empty paths should fail");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("at least one path"));
}

#[test]
fn template_validation_rejects_present_evaluator_with_watched_paths() {
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: impl-plan-present
        label: Implementation plan present
        task: impl-plan-present
policy_gates:
  - gate_id: impl-plan-present
    evaluator: builtin.policy.implementation_plan_present
    kind: hook
    paths:
      - docs/plans/implementation-plan.md
    watched_paths:
      - docs/prd.md
"#;
    let agents = r#"
tasks:
  - name: impl-plan-present
    kind: builtin
    builtin: builtin.policy.implementation_plan_present
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, agents, "prompts: {}\n")
        .expect_err("present evaluator with watched_paths should fail");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("does not allow watched_paths"));
}

#[test]
fn template_validation_rejects_fresh_evaluator_without_exactly_one_primary_path() {
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: implementation-plan-fresh
        label: Implementation plan fresh
        task: implementation-plan-fresh
policy_gates:
  - gate_id: implementation-plan-fresh
    evaluator: builtin.policy.implementation_plan_fresh
    kind: hook
    paths: []
    watched_paths:
      - docs/prd.md
"#;
    let agents = r#"
tasks:
  - name: implementation-plan-fresh
    kind: builtin
    builtin: builtin.policy.implementation_plan_fresh
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, agents, "prompts: {}\n")
        .expect_err("fresh evaluator with zero paths should fail");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("exactly one primary path"));
}

#[test]
fn template_validation_rejects_main_compatible_evaluator_with_file_paths() {
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: merge-drift-reviewed
        label: Merge drift reviewed
        task: merge-drift-reviewed
policy_gates:
  - gate_id: merge-drift-reviewed
    evaluator: builtin.git.is_main_compatible
    kind: hook
    paths:
      - some/file.txt
"#;
    let agents = r#"
tasks:
  - name: merge-drift-reviewed
    kind: builtin
    builtin: builtin.git.is_main_compatible
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, agents, "prompts: {}\n")
        .expect_err("is_main_compatible with file paths should fail");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("does not accept file paths"));
}

#[test]
fn template_validation_rejects_policy_gate_with_unknown_evaluator() {
    // Evaluator matches the task's builtin field (passes mismatch check) but is not
    // one of the recognised evaluator strings in validate_policy_gate.
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: custom-gate
        label: Custom gate
        task: custom-task
policy_gates:
  - gate_id: custom-gate
    evaluator: builtin.custom.unknown_evaluator
    kind: hook
    paths:
      - some/path.md
"#;
    let agents = r#"
tasks:
  - name: custom-task
    kind: builtin
    builtin: builtin.custom.unknown_evaluator
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, agents, "prompts: {}\n")
        .expect_err("unknown evaluator should fail validate_policy_gate");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("unsupported evaluator"));
}

#[test]
fn template_validation_rejects_transition_from_unknown_state() {
    let state_machine = r#"
initial_state: new
states:
  - new
  - implementation
gate_groups:
  - id: validation
    label: Validation
    gates:
      - id: rust-quality-green
        label: Rust quality green
        task: rust-quality
transitions:
  - from: ghost
    to: implementation
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, VALID_AGENTS, VALID_PROMPTS)
        .expect_err("unknown transition source should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("transition from 'ghost'"));
}

#[test]
fn template_validation_rejects_transition_to_unknown_state() {
    let state_machine = r#"
initial_state: new
states:
  - new
  - implementation
gate_groups:
  - id: validation
    label: Validation
    gates:
      - id: rust-quality-green
        label: Rust quality green
        task: rust-quality
transitions:
  - from: new
    to: ghost
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, VALID_AGENTS, VALID_PROMPTS)
        .expect_err("unknown transition target should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("transition to 'ghost'"));
}

#[test]
fn template_validation_rejects_ambiguous_outgoing_transitions() {
    let state_machine = r#"
initial_state: new
states:
  - name: new
    type: function
    function: noop
  - implementation
  - ready-for-review
gate_groups:
  - id: validation
    label: Validation
    gates:
      - id: rust-quality-green
        label: Rust quality green
        task: rust-quality
transitions:
  - from: new
    to: implementation
  - from: new
    to: ready-for-review
"#;

    let error = TemplateSet::from_yaml_strings(state_machine, VALID_AGENTS, VALID_PROMPTS)
        .expect_err("multiple outgoing transitions should fail validation");

    assert!(matches!(error, TemplateError::Validation(_)));
    assert!(error.to_string().contains("ambiguous transitions"));
}

// ── New tests for configurable templates (issue #37) ─────────────────────────

#[test]
fn gate_with_auto_open_task_on_fail_parses_correctly() {
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: validation
    label: Validation
    gates:
      - id: rust-quality-green
        label: Rust quality green
        task: rust-quality
        auto_open_task_on_fail: fix-rust-quality
"#;

    let template = TemplateSet::from_yaml_strings(state_machine, VALID_AGENTS, VALID_PROMPTS)
        .expect("gate with auto_open_task_on_fail should parse");

    let gate = &template.state_machine.gate_groups[0].gates[0];
    assert_eq!(
        gate.auto_open_task_on_fail.as_deref(),
        Some("fix-rust-quality")
    );
}

#[test]
fn gate_with_timeout_policy_parses_correctly() {
    let state_machine = r#"
initial_state: new
states:
  - new
gate_groups:
  - id: validation
    label: Validation
    gates:
      - id: rust-quality-green
        label: Rust quality green
        task: rust-quality
        timeout_policy:
          duration_secs: 3600
          on_timeout: failed
"#;

    let template = TemplateSet::from_yaml_strings(state_machine, VALID_AGENTS, VALID_PROMPTS)
        .expect("gate with timeout_policy should parse");

    let gate = &template.state_machine.gate_groups[0].gates[0];
    let timeout = gate
        .timeout_policy
        .as_ref()
        .expect("timeout_policy should be present");
    assert_eq!(timeout.duration_secs, 3600);
    assert_eq!(timeout.on_timeout, GateStatus::Failed);
}

#[test]
fn validate_coherence_returns_error_for_gate_referencing_nonexistent_task() {
    // Build a TemplateSet directly, bypassing from_yaml_strings strict validation,
    // to test that validate_coherence catches the bad task reference.
    let template = TemplateSet {
        state_machine: StateMachineTemplate {
            initial_state: "new".to_string(),
            states: vec![StateDefinition::Simple("new".to_string())],
            gate_groups: vec![GateGroupTemplate {
                id: "validation".to_string(),
                label: "Validation".to_string(),
                gates: vec![GateTemplate {
                    id: "some-gate".to_string(),
                    label: "Some gate".to_string(),
                    task: "nonexistent-task".to_string(),
                    timeout_policy: None,
                    waiver_policy: None,
                    auto_open_task_on_fail: None,
                    pr_checklist_label: None,
                    allow_parallel_with: None,
                    blocking_scope: None,
                    applies_to: None,
                }],
            }],
            policy_gates: vec![],
            transitions: vec![],
            feature_unit: None,
            artifact_policies: None,
            on: None,
        },
        agents: AgentCatalog {
            tasks: vec![AgentTask {
                name: "other-task".to_string(),
                kind: AgentTaskKind::Human,
                role: None,
                builtin: None,
            }],
            doctor_checks: vec![],
        },
        prompts: PromptCatalog {
            prompts: BTreeMap::new(),
        },
    };

    let errors = template.validate_coherence();
    assert!(
        !errors.is_empty(),
        "validate_coherence should return errors for gate with nonexistent task"
    );
    assert!(
        errors.iter().any(|e| e.contains("nonexistent-task")),
        "error should mention the bad task name; errors: {errors:?}"
    );
}

#[test]
fn validate_coherence_returns_empty_for_embedded_default_template() {
    let embedded = load_embedded_template_set().expect("embedded template should load");
    let errors = embedded.validate_coherence();
    assert!(
        errors.is_empty(),
        "embedded default template coherence errors: {errors:?}"
    );
}

#[test]
fn load_from_directory_merges_local_override_over_default() {
    let temp_dir = temp_template_dir();
    let dot_calypso = temp_dir.join(".calypso");
    fs::create_dir_all(&dot_calypso).expect(".calypso directory should be created");

    // Override only the `initial_state` key; all other keys come from defaults.
    // This tests that local keys win while default keys not present locally are preserved.
    let override_sm = r#"
initial_state: implementation
"#;
    fs::write(dot_calypso.join("state-machine.yml"), override_sm)
        .expect("state-machine override should write");

    // No agents.yml or prompts.yml override — should fall back to defaults
    let template =
        TemplateSet::load_from_directory(&temp_dir).expect("load_from_directory should succeed");

    // The merged state machine uses our local override for initial_state
    assert_eq!(template.state_machine.initial_state, "implementation");

    // All other keys (states, gate_groups, policy_gates) come from the defaults
    assert!(
        template
            .state_machine
            .states
            .iter()
            .any(|s| s.name() == "new")
    );
    assert!(!template.state_machine.gate_groups.is_empty());

    // The agents and prompts come from the defaults (has more than 1 task)
    assert!(template.agents.tasks.len() > 1);

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn template_validate_subcommand_exits_zero_for_valid_template() {
    use std::process::Command;

    // Build path to calypso binary
    let bin = std::env::current_exe()
        .expect("test executable path should resolve")
        .parent()
        .expect("test dir should have parent")
        .parent()
        .expect("debug dir should have parent")
        .join("calypso");

    // Use a temporary directory with no .calypso overrides — falls back to embedded defaults
    let temp_dir = temp_template_dir();

    let output = Command::new(&bin)
        .arg("template")
        .arg("validate")
        .current_dir(&temp_dir)
        .output();

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");

    match output {
        Ok(out) => {
            assert!(
                out.status.success(),
                "template validate should exit 0 for valid template; stderr: {}",
                String::from_utf8_lossy(&out.stderr)
            );
            assert!(
                String::from_utf8_lossy(&out.stdout).contains("OK"),
                "template validate should print OK"
            );
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            // Binary not built in this test run — skip gracefully
            eprintln!("calypso binary not found at {bin:?}, skipping subprocess test");
        }
        Err(e) => panic!("unexpected error spawning calypso binary: {e}"),
    }
}

// ── load_from_directory coverage ─────────────────────────────────────────────

#[test]
fn load_from_directory_falls_back_to_defaults_when_no_calypso_dir_exists() {
    let temp_dir = temp_template_dir();
    // No .calypso directory at all — every file is absent, all fallbacks exercised.
    let template = TemplateSet::load_from_directory(&temp_dir)
        .expect("load_from_directory should succeed with no overrides");

    assert_eq!(template.state_machine.initial_state, "new");
    assert!(!template.state_machine.gate_groups.is_empty());

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn load_from_directory_merges_agents_override_without_sm_or_prompts() {
    let temp_dir = temp_template_dir();
    let dot_calypso = temp_dir.join(".calypso");
    fs::create_dir_all(&dot_calypso).expect(".calypso dir should be created");

    // Override only the doctor_checks key, leaving tasks unchanged so cross-references hold.
    let agents_override = r#"
doctor_checks:
  - id: custom-check
    label: Custom check
    command: echo
    args: ["ok"]
"#;
    fs::write(dot_calypso.join("agents.yml"), agents_override)
        .expect("agents override should write");

    let template =
        TemplateSet::load_from_directory(&temp_dir).expect("load_from_directory should succeed");

    // The merged agents catalog includes the custom doctor check from the override
    assert!(
        template
            .agents
            .doctor_checks
            .iter()
            .any(|c| c.id == "custom-check"),
        "merged agent catalog should contain overridden doctor check"
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn load_from_directory_merges_prompts_override_without_sm_or_agents() {
    let temp_dir = temp_template_dir();
    let dot_calypso = temp_dir.join(".calypso");
    fs::create_dir_all(&dot_calypso).expect(".calypso dir should be created");

    // Only provide a prompts override; state-machine and agents come from defaults.
    // Use the real default prompt keys so validation doesn't fail.
    let embedded = load_embedded_template_set().expect("embedded template should load");
    let mut prompts_map = String::from("prompts:\n");
    for (k, v) in &embedded.prompts.prompts {
        let escaped = v.replace('\n', "\\n");
        prompts_map.push_str(&format!("  {k}: |\n    {escaped}\n"));
    }

    fs::write(dot_calypso.join("prompts.yml"), &prompts_map)
        .expect("prompts override should write");

    let template = TemplateSet::load_from_directory(&temp_dir)
        .expect("load_from_directory with prompts override should succeed");

    assert!(!template.prompts.prompts.is_empty());

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn load_from_directory_silently_ignores_non_mapping_override() {
    // If the override YAML is not a mapping (e.g. a list), merge_yaml_strings skips the merge
    // and returns the base unchanged. The resulting template must still validate.
    let temp_dir = temp_template_dir();
    let dot_calypso = temp_dir.join(".calypso");
    fs::create_dir_all(&dot_calypso).expect(".calypso dir should be created");

    // A YAML list is valid YAML but not a mapping — the merge is a no-op.
    fs::write(dot_calypso.join("state-machine.yml"), "- item1\n- item2\n")
        .expect("list override should write");

    // The merge no-op falls through to the base (default), which is valid.
    let template = TemplateSet::load_from_directory(&temp_dir)
        .expect("non-mapping override should fall through to defaults and succeed");

    assert_eq!(template.state_machine.initial_state, "new");

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn load_from_directory_returns_yaml_error_for_malformed_sm_override() {
    let temp_dir = temp_template_dir();
    let dot_calypso = temp_dir.join(".calypso");
    fs::create_dir_all(&dot_calypso).expect(".calypso dir should be created");

    fs::write(dot_calypso.join("state-machine.yml"), ": bad yaml {{{")
        .expect("malformed sm override should write");

    let error =
        TemplateSet::load_from_directory(&temp_dir).expect_err("malformed YAML should fail");

    assert!(
        matches!(error, TemplateError::Yaml(_)),
        "expected Yaml error, got: {error}"
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn load_from_directory_returns_io_error_for_unreadable_agents_file() {
    let temp_dir = temp_template_dir();
    let dot_calypso = temp_dir.join(".calypso");
    fs::create_dir_all(&dot_calypso).expect(".calypso dir should be created");

    // Create a directory where agents.yml should be — causes IsADirectory I/O error
    fs::create_dir(dot_calypso.join("agents.yml")).expect("agents.yml directory should be created");

    let error = TemplateSet::load_from_directory(&temp_dir)
        .expect_err("unreadable agents file should fail");

    assert!(
        matches!(error, TemplateError::Io(_)),
        "expected Io error, got: {error}"
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

#[test]
fn load_from_directory_returns_io_error_for_unreadable_prompts_file() {
    let temp_dir = temp_template_dir();
    let dot_calypso = temp_dir.join(".calypso");
    fs::create_dir_all(&dot_calypso).expect(".calypso dir should be created");

    // Create a directory where prompts.yml should be — causes IsADirectory I/O error
    fs::create_dir(dot_calypso.join("prompts.yml"))
        .expect("prompts.yml directory should be created");

    let error = TemplateSet::load_from_directory(&temp_dir)
        .expect_err("unreadable prompts file should fail");

    assert!(
        matches!(error, TemplateError::Io(_)),
        "expected Io error, got: {error}"
    );

    fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}

// ── validate_coherence coverage ───────────────────────────────────────────────

fn make_coherence_template() -> TemplateSet {
    TemplateSet {
        state_machine: StateMachineTemplate {
            initial_state: "new".to_string(),
            states: vec![
                StateDefinition::Simple("new".to_string()),
                StateDefinition::Simple("implementation".to_string()),
            ],
            gate_groups: vec![GateGroupTemplate {
                id: "coordination".to_string(),
                label: "Coordination".to_string(),
                gates: vec![GateTemplate {
                    id: "some-gate".to_string(),
                    label: "Some gate".to_string(),
                    task: "human-approval".to_string(),
                    timeout_policy: None,
                    waiver_policy: None,
                    auto_open_task_on_fail: None,
                    pr_checklist_label: None,
                    allow_parallel_with: None,
                    blocking_scope: None,
                    applies_to: None,
                }],
            }],
            policy_gates: vec![],
            transitions: vec![],
            feature_unit: None,
            artifact_policies: None,
            on: None,
        },
        agents: AgentCatalog {
            tasks: vec![AgentTask {
                name: "human-approval".to_string(),
                kind: AgentTaskKind::Human,
                role: None,
                builtin: None,
            }],
            doctor_checks: vec![],
        },
        prompts: PromptCatalog {
            prompts: BTreeMap::new(),
        },
    }
}

#[test]
fn validate_coherence_reports_gate_applies_to_with_nonexistent_state() {
    let mut template = make_coherence_template();
    template.state_machine.gate_groups[0].gates[0].applies_to =
        Some(vec!["nonexistent-state".to_string()]);

    let errors = template.validate_coherence();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("nonexistent-state") && e.contains("applies_to")),
        "expected applies_to error; got: {errors:?}"
    );
}

#[test]
fn validate_coherence_reports_gate_blocking_scope_with_nonexistent_state() {
    let mut template = make_coherence_template();
    template.state_machine.gate_groups[0].gates[0].blocking_scope = Some("ghost-state".to_string());

    let errors = template.validate_coherence();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("ghost-state") && e.contains("blocking_scope")),
        "expected blocking_scope error; got: {errors:?}"
    );
}

#[test]
fn validate_coherence_reports_unrecognized_builtin_keyword() {
    let mut template = make_coherence_template();
    template.agents.tasks.push(AgentTask {
        name: "mystery-check".to_string(),
        kind: AgentTaskKind::Builtin,
        role: None,
        builtin: Some("builtin.mystery.unknown".to_string()),
    });
    // Add a gate referencing it so the task is valid in gate_groups context
    template.state_machine.gate_groups[0]
        .gates
        .push(GateTemplate {
            id: "mystery-gate".to_string(),
            label: "Mystery gate".to_string(),
            task: "mystery-check".to_string(),
            timeout_policy: None,
            waiver_policy: None,
            auto_open_task_on_fail: None,
            pr_checklist_label: None,
            allow_parallel_with: None,
            blocking_scope: None,
            applies_to: None,
        });

    let errors = template.validate_coherence();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("mystery-check") && e.contains("unrecognized builtin")),
        "expected unrecognized builtin error; got: {errors:?}"
    );
}

#[test]
fn validate_coherence_reports_transition_from_nonexistent_state() {
    let mut template = make_coherence_template();
    template.state_machine.transitions.push(TransitionTemplate {
        from: "ghost".to_string(),
        to: "new".to_string(),
    });

    let errors = template.validate_coherence();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("ghost") && e.contains("from")),
        "expected transition-from error; got: {errors:?}"
    );
}

#[test]
fn validate_coherence_reports_transition_to_nonexistent_state() {
    let mut template = make_coherence_template();
    template.state_machine.transitions.push(TransitionTemplate {
        from: "new".to_string(),
        to: "ghost".to_string(),
    });

    let errors = template.validate_coherence();
    assert!(
        errors
            .iter()
            .any(|e| e.contains("ghost") && e.contains("to")),
        "expected transition-to error; got: {errors:?}"
    );
}

// ── StateDefinition serde tests ───────────────────────────────────────────────

#[test]
fn state_definition_simple_parses_as_string() {
    let yaml = "states:\n  - new\n  - prd-review\n";
    #[derive(serde::Deserialize)]
    struct Wrapper {
        states: Vec<StateDefinition>,
    }
    let w: Wrapper = serde_yaml::from_str(yaml).expect("should parse");
    assert_eq!(w.states.len(), 2);
    assert_eq!(w.states[0].name(), "new");
    assert_eq!(w.states[1].name(), "prd-review");
    assert_eq!(
        w.states[0].step_type(),
        calypso_cli::template::StepType::Agent
    );
}

#[test]
fn state_definition_detailed_parses_with_function_type() {
    let yaml = "states:\n  - name: git-init\n    type: function\n    function: git_init\n";
    #[derive(serde::Deserialize)]
    struct Wrapper {
        states: Vec<StateDefinition>,
    }
    let w: Wrapper = serde_yaml::from_str(yaml).expect("should parse");
    assert_eq!(w.states.len(), 1);
    assert_eq!(w.states[0].name(), "git-init");
    assert_eq!(
        w.states[0].step_type(),
        calypso_cli::template::StepType::Function
    );
}

#[test]
fn state_definition_detailed_defaults_step_type_to_agent() {
    let yaml = "states:\n  - name: my-state\n";
    #[derive(serde::Deserialize)]
    struct Wrapper {
        states: Vec<StateDefinition>,
    }
    let w: Wrapper = serde_yaml::from_str(yaml).expect("should parse");
    assert_eq!(
        w.states[0].step_type(),
        calypso_cli::template::StepType::Agent
    );
}

#[test]
fn state_definition_mixed_simple_and_detailed_parse_together() {
    let yaml = "states:\n  - new\n  - name: setup\n    type: function\n    function: do_setup\n  - implementation\n";
    #[derive(serde::Deserialize)]
    struct Wrapper {
        states: Vec<StateDefinition>,
    }
    let w: Wrapper = serde_yaml::from_str(yaml).expect("should parse");
    assert_eq!(w.states.len(), 3);
    assert_eq!(w.states[0].name(), "new");
    assert_eq!(
        w.states[0].step_type(),
        calypso_cli::template::StepType::Agent
    );
    assert_eq!(w.states[1].name(), "setup");
    assert_eq!(
        w.states[1].step_type(),
        calypso_cli::template::StepType::Function
    );
    assert_eq!(w.states[2].name(), "implementation");
}
