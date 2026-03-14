use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::state::{
    AgentSession, AgentSessionStatus, AgentTerminalOutcome, BuiltinEvidence, FeatureState, Gate,
    GateEvaluationError, GateGroup, GateGroupRollup, GateGroupStatus, GateInitializationError,
    GateStatus, PullRequestRef, RepositoryState, SessionOutput, SessionOutputStream, StateError,
    TransitionError, TransitionFacts, WorkflowState,
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
                provider_session_id: Some("codex_01".to_string()),
                status: AgentSessionStatus::Running,
                output: vec![SessionOutput {
                    stream: SessionOutputStream::Stdout,
                    text: "streamed chunk".to_string(),
                }],
                pending_follow_ups: vec!["Please include the diff".to_string()],
                terminal_outcome: None,
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

fn sample_feature(workflow_state: WorkflowState) -> FeatureState {
    let mut feature = sample_state().current_feature;
    feature.workflow_state = workflow_state;
    feature
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
    assert_eq!(
        serde_json::to_string(&AgentSessionStatus::Failed)
            .expect("session status should serialize"),
        "\"failed\""
    );
    assert_eq!(
        serde_json::to_string(&AgentSessionStatus::Aborted)
            .expect("session status should serialize"),
        "\"aborted\""
    );
    assert_eq!(
        serde_json::to_string(&SessionOutputStream::Stderr)
            .expect("session output stream should serialize"),
        "\"stderr\""
    );
    assert_eq!(
        serde_json::to_string(&AgentTerminalOutcome::Nok)
            .expect("session terminal outcome should serialize"),
        "\"nok\""
    );
}

#[test]
fn agent_session_defaults_optional_runtime_fields_when_missing_from_json() {
    let session: AgentSession =
        serde_json::from_str(r#"{"role":"engineer","session_id":"session_01","status":"running"}"#)
            .expect("agent session should deserialize");

    assert!(session.provider_session_id.is_none());
    assert!(session.output.is_empty());
    assert!(session.pending_follow_ups.is_empty());
    assert!(session.terminal_outcome.is_none());
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
        .expect("pr editor gate should exist");
    assert_eq!(pr_editor_gate.status, GateStatus::Pending);
}

#[test]
fn feature_state_leaves_builtin_gate_pending_without_evidence() {
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

    feature
        .evaluate_gates(&template, &BuiltinEvidence::new())
        .expect("gate evaluation should succeed");

    assert!(
        feature
            .gate_groups
            .iter()
            .flat_map(|group| group.gates.iter())
            .filter(|gate| gate.id == "rust-quality-green" || gate.id == "merge-drift-reviewed")
            .all(|gate| gate.status == GateStatus::Pending)
    );
}

#[test]
fn feature_state_maps_agent_and_builtin_tasks_to_pending_and_failing_states() {
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

    feature
        .evaluate_gates(&template, &BuiltinEvidence::new())
        .expect("gate evaluation should succeed");

    let pr_gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "pr-canonicalized")
        .expect("pr gate should exist");
    assert_eq!(pr_gate.status, GateStatus::Pending);

    let blueprint_gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "blueprint-policy-clean")
        .expect("blueprint review gate should exist");
    assert_eq!(blueprint_gate.status, GateStatus::Pending);
}

#[test]
fn feature_state_rejects_unknown_task_bindings_during_evaluation() {
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

    feature.gate_groups.push(GateGroup {
        id: "custom".to_string(),
        label: "Custom".to_string(),
        gates: vec![Gate {
            id: "unknown".to_string(),
            label: "Unknown".to_string(),
            task: "does-not-exist".to_string(),
            status: GateStatus::Pending,
        }],
    });

    let error = feature
        .evaluate_gates(&template, &BuiltinEvidence::new())
        .expect_err("unknown task should fail evaluation");

    assert!(matches!(error, GateEvaluationError::UnknownTask(_)));
    assert_eq!(
        error.to_string(),
        "gate evaluation references unknown task 'does-not-exist'"
    );
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
        .with_result("builtin.git.is_main_compatible", false)
        .with_result("builtin.doctor.gh_installed", true)
        .with_result("builtin.doctor.codex_installed", true)
        .with_result("builtin.doctor.gh_authenticated", true)
        .with_result("builtin.doctor.github_remote_configured", true)
        .with_result("builtin.doctor.required_workflows_present", true)
        .with_result("builtin.policy.implementation_plan_present", true)
        .with_result("builtin.policy.implementation_plan_fresh", true)
        .with_result("builtin.policy.next_prompt_present", true)
        .with_result("builtin.policy.required_workflows_present", true)
        .with_result("builtin.github.pr_exists", true)
        .with_result("builtin.github.pr_checks_green", true)
        .with_result("builtin.github.pr_merged", false);

    feature
        .evaluate_gates(&template, &evidence)
        .expect("gate evaluation should succeed");

    assert_eq!(
        feature.blocking_gate_ids(),
        vec![
            "pr-canonicalized".to_string(),
            "blueprint-policy-clean".to_string(),
            "merge-drift-reviewed".to_string(),
        ]
    );
}

#[test]
fn feature_state_reports_gate_group_rollups_for_mixed_statuses() {
    let feature = FeatureState {
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
                gates: vec![
                    Gate {
                        id: "rust-quality-green".to_string(),
                        label: "Rust quality green".to_string(),
                        task: "rust-quality".to_string(),
                        status: GateStatus::Passing,
                    },
                    Gate {
                        id: "qa-signoff".to_string(),
                        label: "QA signoff".to_string(),
                        task: "qa-review".to_string(),
                        status: GateStatus::Manual,
                    },
                ],
            },
            GateGroup {
                id: "merge-readiness".to_string(),
                label: "Merge Readiness".to_string(),
                gates: vec![
                    Gate {
                        id: "merge-drift-reviewed".to_string(),
                        label: "Merge drift reviewed".to_string(),
                        task: "main-compatible".to_string(),
                        status: GateStatus::Failing,
                    },
                    Gate {
                        id: "pr-green".to_string(),
                        label: "PR green".to_string(),
                        task: "pr-green".to_string(),
                        status: GateStatus::Pending,
                    },
                ],
            },
        ],
        ..sample_feature(WorkflowState::Implementation)
    };

    assert_eq!(
        feature.gate_group_rollups(),
        vec![
            GateGroupRollup {
                id: "specification".to_string(),
                label: "Specification".to_string(),
                status: GateGroupStatus::Passing,
                blocking_gate_ids: vec![],
            },
            GateGroupRollup {
                id: "validation".to_string(),
                label: "Validation".to_string(),
                status: GateGroupStatus::Manual,
                blocking_gate_ids: vec!["qa-signoff".to_string()],
            },
            GateGroupRollup {
                id: "merge-readiness".to_string(),
                label: "Merge Readiness".to_string(),
                status: GateGroupStatus::Blocked,
                blocking_gate_ids: vec!["merge-drift-reviewed".to_string(), "pr-green".to_string(),],
            },
        ]
    );
}

#[test]
fn feature_state_reports_available_transitions_for_each_workflow_state() {
    assert_eq!(
        sample_feature(WorkflowState::New).available_transitions(&TransitionFacts {
            feature_binding_complete: true,
            ..TransitionFacts::default()
        }),
        vec![WorkflowState::Implementation]
    );

    assert_eq!(
        sample_feature(WorkflowState::Implementation).available_transitions(&TransitionFacts {
            waiting_for_human_input: true,
            ..TransitionFacts::default()
        }),
        vec![WorkflowState::WaitingForHuman]
    );

    assert_eq!(
        sample_feature(WorkflowState::Implementation).available_transitions(&TransitionFacts {
            ready_for_review: true,
            ..TransitionFacts::default()
        }),
        vec![WorkflowState::ReadyForReview]
    );

    assert_eq!(
        sample_feature(WorkflowState::Implementation).available_transitions(&TransitionFacts {
            blocking_issue_present: true,
            ..TransitionFacts::default()
        }),
        vec![WorkflowState::Blocked]
    );

    assert_eq!(
        sample_feature(WorkflowState::WaitingForHuman).available_transitions(&TransitionFacts {
            human_response_ready: true,
            ..TransitionFacts::default()
        }),
        vec![WorkflowState::Implementation]
    );

    assert_eq!(
        sample_feature(WorkflowState::ReadyForReview).available_transitions(&TransitionFacts {
            review_rework_required: true,
            ..TransitionFacts::default()
        }),
        vec![WorkflowState::Implementation]
    );

    assert_eq!(
        sample_feature(WorkflowState::Blocked).available_transitions(&TransitionFacts {
            blocker_resolved: true,
            ..TransitionFacts::default()
        }),
        vec![WorkflowState::Implementation]
    );

    assert_eq!(
        sample_feature(WorkflowState::WaitingForHuman).available_transitions(&TransitionFacts {
            blocking_issue_present: true,
            ..TransitionFacts::default()
        }),
        vec![WorkflowState::Blocked]
    );

    assert_eq!(
        sample_feature(WorkflowState::ReadyForReview).available_transitions(&TransitionFacts {
            blocking_issue_present: true,
            ..TransitionFacts::default()
        }),
        vec![WorkflowState::Blocked]
    );
}

#[test]
fn feature_state_rejects_transitions_without_required_facts() {
    let mut feature = sample_feature(WorkflowState::Implementation);

    let error = feature
        .transition_to(WorkflowState::ReadyForReview, &TransitionFacts::default())
        .expect_err("missing readiness facts should reject the transition");

    assert_eq!(
        error,
        TransitionError::Rejected {
            from: WorkflowState::Implementation,
            to: WorkflowState::ReadyForReview,
            reason: "feature is not ready for review".to_string(),
        }
    );
    assert_eq!(
        error.to_string(),
        "cannot transition from 'implementation' to 'ready-for-review': feature is not ready for review"
    );
}

#[test]
fn feature_state_transitions_when_required_facts_are_present() {
    let mut feature = sample_feature(WorkflowState::WaitingForHuman);

    feature
        .transition_to(
            WorkflowState::Implementation,
            &TransitionFacts {
                human_response_ready: true,
                ..TransitionFacts::default()
            },
        )
        .expect("human response should resume implementation");

    assert_eq!(feature.workflow_state, WorkflowState::Implementation);
}

#[test]
fn workflow_state_as_str_covers_all_variants() {
    assert_eq!(WorkflowState::New.as_str(), "new");
    assert_eq!(WorkflowState::Implementation.as_str(), "implementation");
    assert_eq!(WorkflowState::WaitingForHuman.as_str(), "waiting-for-human");
    assert_eq!(WorkflowState::ReadyForReview.as_str(), "ready-for-review");
    assert_eq!(WorkflowState::Blocked.as_str(), "blocked");
}

#[test]
fn feature_state_rejects_all_missing_fact_transitions() {
    // New -> Implementation without binding
    let error = sample_feature(WorkflowState::New)
        .transition_to(WorkflowState::Implementation, &TransitionFacts::default())
        .expect_err("missing binding should reject");
    assert!(error.to_string().contains("feature binding is incomplete"));

    // Implementation -> WaitingForHuman without waiting flag
    let error = sample_feature(WorkflowState::Implementation)
        .transition_to(WorkflowState::WaitingForHuman, &TransitionFacts::default())
        .expect_err("missing waiting flag should reject");
    assert!(
        error
            .to_string()
            .contains("no agent session is waiting for human input")
    );

    // Implementation -> Blocked without blocking issue
    let error = sample_feature(WorkflowState::Implementation)
        .transition_to(WorkflowState::Blocked, &TransitionFacts::default())
        .expect_err("missing blocking issue should reject");
    assert!(error.to_string().contains("no blocking issue is present"));

    // WaitingForHuman -> Implementation without human response
    let error = sample_feature(WorkflowState::WaitingForHuman)
        .transition_to(WorkflowState::Implementation, &TransitionFacts::default())
        .expect_err("missing human response should reject");
    assert!(error.to_string().contains("no human response is available"));

    // WaitingForHuman -> Blocked without blocking issue
    let error = sample_feature(WorkflowState::WaitingForHuman)
        .transition_to(WorkflowState::Blocked, &TransitionFacts::default())
        .expect_err("missing blocking issue should reject");
    assert!(error.to_string().contains("no blocking issue is present"));

    // ReadyForReview -> Implementation without rework flag
    let error = sample_feature(WorkflowState::ReadyForReview)
        .transition_to(WorkflowState::Implementation, &TransitionFacts::default())
        .expect_err("missing rework flag should reject");
    assert!(
        error
            .to_string()
            .contains("no follow-up implementation request is present")
    );

    // ReadyForReview -> Blocked without blocking issue
    let error = sample_feature(WorkflowState::ReadyForReview)
        .transition_to(WorkflowState::Blocked, &TransitionFacts::default())
        .expect_err("missing blocking issue should reject");
    assert!(error.to_string().contains("no blocking issue is present"));

    // Blocked -> Implementation without blocker resolved
    let error = sample_feature(WorkflowState::Blocked)
        .transition_to(WorkflowState::Implementation, &TransitionFacts::default())
        .expect_err("unresolved blocker should reject");
    assert!(
        error
            .to_string()
            .contains("blocking issue is still present")
    );

    // Unsupported transition (New -> Blocked)
    let error = sample_feature(WorkflowState::New)
        .transition_to(WorkflowState::Blocked, &TransitionFacts::default())
        .expect_err("unsupported transition should reject");
    assert!(
        error
            .to_string()
            .contains("transition is not supported by the prototype workflow")
    );
}

#[test]
fn gate_group_rollup_status_is_pending_when_all_gates_are_pending() {
    let feature = FeatureState {
        gate_groups: vec![GateGroup {
            id: "validation".to_string(),
            label: "Validation".to_string(),
            gates: vec![
                Gate {
                    id: "gate-a".to_string(),
                    label: "Gate A".to_string(),
                    task: "task-a".to_string(),
                    status: GateStatus::Pending,
                },
                Gate {
                    id: "gate-b".to_string(),
                    label: "Gate B".to_string(),
                    task: "task-b".to_string(),
                    status: GateStatus::Pending,
                },
            ],
        }],
        ..sample_feature(WorkflowState::Implementation)
    };

    let rollups = feature.gate_group_rollups();
    assert_eq!(rollups[0].status, GateGroupStatus::Pending);
}

#[test]
fn feature_state_evaluate_gates_maps_human_task_to_manual_status() {
    let template = TemplateSet::from_yaml_strings(
        r#"
initial_state: new
states:
  - new
gate_groups:
  - id: review
    label: Review
    gates:
      - id: human-signoff
        label: Human signoff
        task: human-reviewer
"#,
        r#"
tasks:
  - name: human-reviewer
    kind: human
"#,
        "prompts: {}\n",
    )
    .expect("template with human task should be valid");

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

    feature
        .evaluate_gates(&template, &BuiltinEvidence::new())
        .expect("gate evaluation should succeed");

    let gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "human-signoff")
        .expect("human signoff gate should exist");
    assert_eq!(gate.status, GateStatus::Manual);
}

#[test]
fn feature_state_sets_human_task_gate_to_manual_status() {
    use calypso_cli::template::TemplateSet;

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
"#,
        r#"
tasks:
  - name: human-approval
    kind: human
"#,
        "prompts: {}\n",
    )
    .expect("template should validate");

    let mut feature = FeatureState::from_template(
        "feat-human",
        "feat/human-test",
        "/worktrees/feat-human",
        PullRequestRef {
            number: 1,
            url: "https://github.com/org/repo/pull/1".to_string(),
        },
        &template,
    )
    .expect("feature should initialize");

    feature
        .evaluate_gates(&template, &BuiltinEvidence::new())
        .expect("gate evaluation should succeed");

    let gate = feature.gate_groups[0].gates[0].clone();
    assert_eq!(gate.status, GateStatus::Manual);
}
