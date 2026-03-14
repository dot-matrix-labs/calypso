use calypso_cli::template::{
    AgentTaskKind, TemplateError, TemplateSet, load_embedded_template_set,
    resolve_template_set_for_path,
};
#[allow(unused_imports)]
use calypso_cli::template::{GateStatus, TimeoutPolicy};
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
        "{}\npolicy_gates:\n  - gate_id: missing-gate\n    evaluator: builtin.policy.next_prompt_present\n    kind: hook\n    paths:\n      - docs/plans/next-prompt.md\n",
        VALID_STATE_MACHINE
    );
    let agents = format!(
        "{}  - name: next-prompt-present\n    kind: builtin\n    builtin: builtin.policy.next_prompt_present\n",
        VALID_AGENTS
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
    use calypso_cli::template::{
        AgentCatalog, AgentTask, GateGroupTemplate, GateTemplate, PromptCatalog,
        StateMachineTemplate,
    };
    use std::collections::BTreeMap;

    // Build a TemplateSet directly, bypassing from_yaml_strings strict validation,
    // to test that validate_coherence catches the bad task reference.
    let template = TemplateSet {
        state_machine: StateMachineTemplate {
            initial_state: "new".to_string(),
            states: vec!["new".to_string()],
            gate_groups: vec![GateGroupTemplate {
                id: "validation".to_string(),
                label: "Validation".to_string(),
                gates: vec![GateTemplate {
                    id: "some-gate".to_string(),
                    label: "Some gate".to_string(),
                    task: "nonexistent-task".to_string(),
                    recheck_trigger: None,
                    timeout_policy: None,
                    waiver_policy: None,
                    auto_open_task_on_fail: None,
                    pr_checklist_label: None,
                    allow_parallel_with: None,
                    blocking_scope: None,
                    applies_to: None,
                    status_source: None,
                }],
            }],
            policy_gates: vec![],
            transitions: vec![],
            feature_unit: None,
            artifact_policies: None,
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
    assert!(template.state_machine.states.contains(&"new".to_string()));
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
