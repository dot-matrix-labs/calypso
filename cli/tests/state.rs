use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::state::{
    AgentSession, AgentSessionStatus, BuiltinEvidence, FeatureState, Gate, GateEvaluationError,
    GateGroup, GateInitializationError, GateStatus, PullRequestRef, RepositoryState, StateError,
    WorkflowState,
};
use calypso_cli::template::{TemplateSet, load_embedded_template_set};

fn sample_state() -> RepositoryState {
    RepositoryState {
        version: 1,
        repo_id: "acme-api".to_string(),
        current_feature: FeatureState {
            feature_id: "feat-auth-refresh".to_string(),
            branch: "feat/123-token-refresh".to_string(),
            worktree_path: "/worktrees/feat-123-token-refresh".to_string(),
            pull_request: PullRequestRef {
                number: 231,
                url: "https://github.com/org/repo/pull/231".to_string(),
            },
            workflow_state: WorkflowState::Implementation,
            gate_groups: vec![
                GateGroup {
                    id: "specification".to_string(),
                    label: "Specification".to_string(),
                    gates: vec![Gate {
                        id: "pr-canonicalized".to_string(),
                        label: "PR canonicalized".to_string(),
                        task: "pr-editor".to_string(),
                        status: GateStatus::Passing,
                    }],
                },
                GateGroup {
                    id: "validation".to_string(),
                    label: "Validation".to_string(),
                    gates: vec![Gate {
                        id: "rust-quality-green".to_string(),
                        label: "Rust quality green".to_string(),
                        task: "rust-quality".to_string(),
                        status: GateStatus::Pending,
                    }],
                },
            ],
            active_sessions: vec![AgentSession {
                role: "engineer".to_string(),
                session_id: "session_01".to_string(),
                status: AgentSessionStatus::Running,
            }],
        },
    }
}

fn temp_state_path() -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("calypso-state-{unique}.json"))
}

#[test]
fn repository_state_round_trips_through_json() {
    let state = sample_state();

    let json = state.to_json_pretty().expect("state should serialize");
    let restored = RepositoryState::from_json(&json).expect("state should deserialize");

    assert_eq!(restored, state);
}

#[test]
fn repository_state_persists_to_disk_and_loads_back() {
    let state = sample_state();
    let path = temp_state_path();

    state.save_to_path(&path).expect("state should save");
    let restored = RepositoryState::load_from_path(&path).expect("state should load");

    assert_eq!(restored, state);

    fs::remove_file(path).expect("temp state file should be removed");
}

#[test]
fn invalid_json_returns_structured_error() {
    let path = temp_state_path();
    fs::write(&path, "{ not valid json").expect("invalid json fixture should write");

    let error = RepositoryState::load_from_path(&path).expect_err("invalid json should fail");

    assert!(matches!(error, StateError::Json(_)));

    fs::remove_file(path).expect("temp state file should be removed");
}

#[test]
fn state_error_formats_io_and_json_failures() {
    let missing_path = temp_state_path();
    let io_error =
        RepositoryState::load_from_path(&missing_path).expect_err("missing file should fail");
    assert!(matches!(io_error, StateError::Io(_)));
    assert!(io_error.to_string().contains("state I/O error"));

    let json_error = RepositoryState::from_json("{ nope").expect_err("bad json should fail");
    assert!(matches!(json_error, StateError::Json(_)));
    assert!(json_error.to_string().contains("state JSON error"));
}

#[test]
fn state_enums_serialize_with_expected_kebab_case_variants() {
    assert_eq!(
        serde_json::to_string(&WorkflowState::New).expect("workflow state should serialize"),
        "\"new\""
    );
    assert_eq!(
        serde_json::to_string(&WorkflowState::WaitingForHuman)
            .expect("workflow state should serialize"),
        "\"waiting-for-human\""
    );
    assert_eq!(
        serde_json::to_string(&WorkflowState::ReadyForReview)
            .expect("workflow state should serialize"),
        "\"ready-for-review\""
    );
    assert_eq!(
        serde_json::to_string(&WorkflowState::Blocked).expect("workflow state should serialize"),
        "\"blocked\""
    );

    assert_eq!(
        serde_json::to_string(&GateStatus::Failing).expect("gate status should serialize"),
        "\"failing\""
    );
    assert_eq!(
        serde_json::to_string(&GateStatus::Manual).expect("gate status should serialize"),
        "\"manual\""
    );

    assert_eq!(
        serde_json::to_string(&AgentSessionStatus::WaitingForHuman)
            .expect("session status should serialize"),
        "\"waiting-for-human\""
    );
    assert_eq!(
        serde_json::to_string(&AgentSessionStatus::Completed)
            .expect("session status should serialize"),
        "\"completed\""
    );
}

#[test]
fn feature_state_initializes_gate_groups_from_template() {
    let template = load_embedded_template_set().expect("embedded template should load");

    let feature = FeatureState::from_template(
        "feat-auth-refresh",
        "feat/123-token-refresh",
        "/worktrees/feat-123-token-refresh",
        PullRequestRef {
            number: 231,
            url: "https://github.com/org/repo/pull/231".to_string(),
        },
        &template,
    )
    .expect("feature should initialize from template");

    assert_eq!(feature.workflow_state, WorkflowState::New);
    assert_eq!(feature.active_sessions.len(), 0);
    assert_eq!(
        feature.gate_groups.len(),
        template.state_machine.gate_groups.len()
    );
    assert_eq!(feature.gate_groups[0].gates[0].task, "pr-editor");
    assert!(
        feature
            .gate_groups
            .iter()
            .flat_map(|group| group.gates.iter())
            .all(|gate| gate.status == GateStatus::Pending)
    );
}

#[test]
fn feature_state_initialization_rejects_unknown_initial_workflow_state() {
    let invalid_template = TemplateSet::from_yaml_strings(
        r#"
initial_state: made-up
states:
  - made-up
gate_groups:
  - id: validation
    label: Validation
    gates:
      - id: rust-quality-green
        label: Rust quality green
        task: rust-quality
"#,
        r#"
tasks:
  - name: rust-quality
    kind: builtin
    builtin: builtin.ci.rust_quality_green
"#,
        "prompts: {}\n",
    )
    .expect("template shape should still validate");

    let error = FeatureState::from_template(
        "feat-auth-refresh",
        "feat/123-token-refresh",
        "/worktrees/feat-123-token-refresh",
        PullRequestRef {
            number: 231,
            url: "https://github.com/org/repo/pull/231".to_string(),
        },
        &invalid_template,
    )
    .expect_err("unknown workflow state should fail initialization");

    assert!(matches!(
        error,
        GateInitializationError::UnknownWorkflowState(_)
    ));
    assert!(error.to_string().contains("made-up"));
}

#[test]
fn feature_state_initializes_supported_workflow_variants_from_template() {
    for initial_state in [
        ("implementation", WorkflowState::Implementation),
        ("waiting-for-human", WorkflowState::WaitingForHuman),
        ("ready-for-review", WorkflowState::ReadyForReview),
        ("blocked", WorkflowState::Blocked),
    ] {
        let template = TemplateSet::from_yaml_strings(
            &format!(
                r#"
initial_state: {}
states:
  - {}
gate_groups:
  - id: validation
    label: Validation
    gates:
      - id: rust-quality-green
        label: Rust quality green
        task: rust-quality
"#,
                initial_state.0, initial_state.0
            ),
            r#"
tasks:
  - name: rust-quality
    kind: builtin
    builtin: builtin.ci.rust_quality_green
"#,
            "prompts: {}\n",
        )
        .expect("template should parse");

        let feature = FeatureState::from_template(
            "feat-auth-refresh",
            "feat/123-token-refresh",
            "/worktrees/feat-123-token-refresh",
            PullRequestRef {
                number: 231,
                url: "https://github.com/org/repo/pull/231".to_string(),
            },
            &template,
        )
        .expect("feature should initialize from template");

        assert_eq!(feature.workflow_state, initial_state.1);
    }
}

#[test]
fn feature_state_evaluates_builtin_gates_from_template_bindings() {
    let template = load_embedded_template_set().expect("embedded template should load");
    let mut feature = FeatureState::from_template(
        "feat-auth-refresh",
        "feat/123-token-refresh",
        "/worktrees/feat-123-token-refresh",
        PullRequestRef {
            number: 231,
            url: "https://github.com/org/repo/pull/231".to_string(),
        },
        &template,
    )
    .expect("feature should initialize from template");
    let evidence = BuiltinEvidence::new()
        .with_result("builtin.ci.rust_quality_green", true)
        .with_result("builtin.git.is_main_compatible", false);

    feature
        .evaluate_gates(&template, &evidence)
        .expect("gate evaluation should succeed");

    let rust_quality_gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "rust-quality-green")
        .expect("rust quality gate should exist");
    assert_eq!(rust_quality_gate.status, GateStatus::Passing);

    let main_compatibility_gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "merge-drift-reviewed")
        .expect("main compatibility gate should exist");
    assert_eq!(main_compatibility_gate.status, GateStatus::Failing);

    let pr_editor_gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "pr-canonicalized")
        .expect("PR editor gate should exist");
    assert_eq!(pr_editor_gate.status, GateStatus::Pending);
}

#[test]
fn feature_state_reports_blocking_gate_ids_after_evaluation() {
    let template = load_embedded_template_set().expect("embedded template should load");
    let mut feature = FeatureState::from_template(
        "feat-auth-refresh",
        "feat/123-token-refresh",
        "/worktrees/feat-123-token-refresh",
        PullRequestRef {
            number: 231,
            url: "https://github.com/org/repo/pull/231".to_string(),
        },
        &template,
    )
    .expect("feature should initialize from template");
    let evidence = BuiltinEvidence::new()
        .with_result("builtin.ci.rust_quality_green", true)
        .with_result("builtin.git.is_main_compatible", true);

    feature
        .evaluate_gates(&template, &evidence)
        .expect("gate evaluation should succeed");

    let blocking_ids = feature.blocking_gate_ids();

    assert!(blocking_ids.contains(&"pr-canonicalized".to_string()));
    assert!(blocking_ids.contains(&"blueprint-policy-clean".to_string()));
    assert!(!blocking_ids.contains(&"rust-quality-green".to_string()));
    assert!(!blocking_ids.contains(&"merge-drift-reviewed".to_string()));
}

#[test]
fn feature_state_leaves_builtin_gate_pending_without_evidence() {
    let template = TemplateSet::from_yaml_strings(
        r#"
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
"#,
        r#"
tasks:
  - name: rust-quality
    kind: builtin
    builtin: builtin.ci.rust_quality_green
"#,
        "prompts: {}\n",
    )
    .expect("template should parse");
    let mut feature = FeatureState::from_template(
        "feat-auth-refresh",
        "feat/123-token-refresh",
        "/worktrees/feat-123-token-refresh",
        PullRequestRef {
            number: 231,
            url: "https://github.com/org/repo/pull/231".to_string(),
        },
        &template,
    )
    .expect("feature should initialize");

    feature
        .evaluate_gates(&template, &BuiltinEvidence::new())
        .expect("evaluation should succeed");

    assert_eq!(feature.gate_groups[0].gates[0].status, GateStatus::Pending);
}

#[test]
fn feature_state_maps_human_and_hook_tasks_to_manual_and_pending() {
    let template = TemplateSet::from_yaml_strings(
        r#"
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
"#,
        r#"
tasks:
  - name: human-approval
    kind: human
  - name: push-sync
    kind: hook
"#,
        "prompts: {}\n",
    )
    .expect("template should parse");
    let mut feature = FeatureState::from_template(
        "feat-auth-refresh",
        "feat/123-token-refresh",
        "/worktrees/feat-123-token-refresh",
        PullRequestRef {
            number: 231,
            url: "https://github.com/org/repo/pull/231".to_string(),
        },
        &template,
    )
    .expect("feature should initialize");

    feature
        .evaluate_gates(&template, &BuiltinEvidence::new())
        .expect("evaluation should succeed");

    assert_eq!(feature.gate_groups[0].gates[0].status, GateStatus::Manual);
    assert_eq!(feature.gate_groups[0].gates[1].status, GateStatus::Pending);
}

#[test]
fn feature_state_rejects_unknown_task_bindings_during_evaluation() {
    let invalid_template = TemplateSet {
        state_machine: load_embedded_template_set()
            .expect("embedded template should load")
            .state_machine,
        agents: calypso_cli::template::AgentCatalog { tasks: vec![] },
        prompts: calypso_cli::template::PromptCatalog {
            prompts: std::collections::BTreeMap::new(),
        },
    };
    let mut feature = FeatureState {
        feature_id: "feat-auth-refresh".to_string(),
        branch: "feat/123-token-refresh".to_string(),
        worktree_path: "/worktrees/feat-123-token-refresh".to_string(),
        pull_request: PullRequestRef {
            number: 231,
            url: "https://github.com/org/repo/pull/231".to_string(),
        },
        workflow_state: WorkflowState::New,
        gate_groups: vec![GateGroup {
            id: "validation".to_string(),
            label: "Validation".to_string(),
            gates: vec![Gate {
                id: "rust-quality-green".to_string(),
                label: "Rust quality green".to_string(),
                task: "rust-quality".to_string(),
                status: GateStatus::Pending,
            }],
        }],
        active_sessions: vec![],
    };

    let error = feature
        .evaluate_gates(&invalid_template, &BuiltinEvidence::new())
        .expect_err("unknown task binding should fail evaluation");

    assert!(matches!(error, GateEvaluationError::UnknownTask(_)));
    assert!(error.to_string().contains("rust-quality"));
}

#[test]
fn builtin_evidence_merge_prefers_later_results_for_duplicate_keys() {
    let merged = BuiltinEvidence::new()
        .with_result("builtin.doctor.gh_installed", true)
        .merge(
            &BuiltinEvidence::new()
                .with_result("builtin.github.pr_exists", true)
                .with_result("builtin.doctor.gh_installed", false),
        );

    assert_eq!(merged.result_for("builtin.github.pr_exists"), Some(true));
    assert_eq!(
        merged.result_for("builtin.doctor.gh_installed"),
        Some(false)
    );
}

#[test]
fn embedded_template_evaluates_doctor_and_github_builtin_gates() {
    let template = load_embedded_template_set().expect("embedded template should load");
    let mut feature = FeatureState::from_template(
        "feat-auth-refresh",
        "feat/123-token-refresh",
        "/worktrees/feat-123-token-refresh",
        PullRequestRef {
            number: 231,
            url: "https://github.com/org/repo/pull/231".to_string(),
        },
        &template,
    )
    .expect("feature should initialize from template");
    let evidence = BuiltinEvidence::new()
        .with_result("builtin.doctor.gh_installed", true)
        .with_result("builtin.doctor.codex_installed", false)
        .with_result("builtin.github.pr_exists", true)
        .with_result("builtin.github.pr_checks_green", false);

    feature
        .evaluate_gates(&template, &evidence)
        .expect("gate evaluation should succeed");

    let gh_gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "gh-cli-installed")
        .expect("gh installed gate should exist");
    assert_eq!(gh_gate.status, GateStatus::Passing);

    let codex_gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "codex-cli-installed")
        .expect("codex gate should exist");
    assert_eq!(codex_gate.status, GateStatus::Failing);

    let pr_exists_gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "feature-pr-exists")
        .expect("pr exists gate should exist");
    assert_eq!(pr_exists_gate.status, GateStatus::Passing);

    let pr_checks_gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "feature-pr-checks-green")
        .expect("pr checks gate should exist");
    assert_eq!(pr_checks_gate.status, GateStatus::Failing);
}
