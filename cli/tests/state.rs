use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::state::{
    AgentSession, AgentSessionStatus, BuiltinEvidence, EvidenceStatus, FeatureState, FeatureType,
    Gate, GateEvaluationError, GateGroup, GateGroupStatus, GateInitializationError, GateStatus,
    PullRequestRef, RepositoryIdentity, RepositoryState, SchedulingMeta, SessionOutput,
    SessionOutputStream, StateError, TransitionError, TransitionFacts, WorkflowState,
};
use calypso_cli::template::{TemplateSet, load_embedded_template_set};

fn sample_state() -> RepositoryState {
    RepositoryState {
        version: 1,
        schema_version: 1,
        repo_id: "acme-api".to_string(),
        identity: RepositoryIdentity::default(),
        providers: Vec::new(),
        releases: Vec::new(),
        deployments: Vec::new(),
        current_feature: FeatureState {
            feature_id: "feat-auth-refresh".to_string(),
            branch: "feat/123-token-refresh".to_string(),
            worktree_path: "/worktrees/feat-123-token-refresh".to_string(),
            pull_request: PullRequestRef {
                number: 231,
                url: "https://github.com/org/repo/pull/231".to_string(),
            },
            github_snapshot: None,
            github_error: None,
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
            feature_type: FeatureType::Feat,
            roles: Vec::new(),
            scheduling: SchedulingMeta::default(),
            artifact_refs: Vec::new(),
            transcript_refs: Vec::new(),
            clarification_history: Vec::new(),
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
    // Deprecated aliases must report their canonical kebab-case string values
    assert_eq!(WorkflowState::WaitingForHuman.as_str(), "implementation");
    assert_eq!(WorkflowState::ReadyForReview.as_str(), "release-ready");
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
    assert_eq!(feature.gate_groups[0].gates[0].task, "gh-installed");
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
        ("prd-review", WorkflowState::PrdReview),
        ("architecture-plan", WorkflowState::ArchitecturePlan),
        ("scaffold-tdd", WorkflowState::ScaffoldTdd),
        ("architecture-review", WorkflowState::ArchitectureReview),
        ("implementation", WorkflowState::Implementation),
        ("qa-validation", WorkflowState::QaValidation),
        ("release-ready", WorkflowState::ReleaseReady),
        ("done", WorkflowState::Done),
        ("blocked", WorkflowState::Blocked),
        ("aborted", WorkflowState::Aborted),
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
        .with_result("builtin.github.pr_ready_for_review", true)
        .with_result("builtin.github.pr_checks_green", true)
        .with_status("builtin.github.pr_review_approved", EvidenceStatus::Manual)
        .with_result("builtin.github.pr_mergeable", true);

    feature
        .evaluate_gates(&template, &evidence)
        .expect("gate evaluation should succeed");

    let blocking = feature.blocking_gate_ids();
    // Known blocking gates given the evidence provided
    assert!(blocking.contains(&"pr-canonicalized".to_string()));
    assert!(blocking.contains(&"blueprint-policy-clean".to_string()));
    assert!(blocking.contains(&"feature-pr-reviewed".to_string()));
    assert!(blocking.contains(&"merge-drift-reviewed".to_string()));
    // Evidence-provided gates must not be blocking
    assert!(!blocking.contains(&"feature-pr-exists".to_string()));
    assert!(!blocking.contains(&"rust-quality-green".to_string()));
    assert!(!blocking.contains(&"pr-mergeable".to_string()));
}

#[test]
fn feature_state_maps_pending_builtin_evidence_to_pending_gate_status() {
    let template = TemplateSet::from_yaml_strings(
        r#"
initial_state: new
states:
  - new
gate_groups:
  - id: validation
    label: Validation
    gates:
      - id: review-gate
        label: Review gate
        task: review-check
"#,
        r#"
tasks:
  - name: review-check
    kind: builtin
    builtin: builtin.github.pr_review_approved
"#,
        "prompts: {}\n",
    )
    .expect("template should parse");

    let mut feature = FeatureState::from_template(
        "feat-pending-evidence",
        "feat/pending",
        "/worktrees/feat-pending",
        PullRequestRef {
            number: 1,
            url: "https://github.com/org/repo/pull/1".to_string(),
        },
        &template,
    )
    .expect("feature should initialize from template");

    feature
        .evaluate_gates(
            &template,
            &BuiltinEvidence::new()
                .with_status("builtin.github.pr_review_approved", EvidenceStatus::Pending),
        )
        .expect("gate evaluation should succeed");

    let gate = feature.gate_groups[0]
        .gates
        .first()
        .expect("gate should exist");
    assert_eq!(gate.status, GateStatus::Pending);
}

#[test]
fn feature_state_maps_human_task_to_manual_status() {
    let state_machine_yaml = "\
initial_state: new
states:
  - new
gate_groups:
  - id: approval
    label: Approval
    gates:
      - id: human-sign-off
        label: Human sign-off
        task: human-reviewer
";
    let agents_yaml = "\
tasks:
  - name: human-reviewer
    kind: human
";
    let prompts_yaml = "prompts: {}";

    let template = TemplateSet::from_yaml_strings(state_machine_yaml, agents_yaml, prompts_yaml)
        .expect("custom template should parse");

    let mut feature = FeatureState::from_template(
        "feat-approval",
        "feat/approval",
        "/worktrees/feat-approval",
        PullRequestRef {
            number: 1,
            url: "https://github.com/org/repo/pull/1".to_string(),
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
        .find(|gate| gate.id == "human-sign-off")
        .expect("human-sign-off gate should exist");

    assert_eq!(gate.status, GateStatus::Manual);
}

#[test]
fn feature_state_maps_human_task_to_manual_gate_status() {
    let template = TemplateSet::from_yaml_strings(
        r#"
initial_state: new
states:
  - new
gate_groups:
  - id: approval
    label: Approval
    gates:
      - id: human-sign-off
        label: Human sign-off
        task: human-approver
"#,
        r#"
tasks:
  - name: human-approver
    kind: human
"#,
        "prompts: {}\n",
    )
    .expect("template should parse");

    let mut feature = FeatureState::from_template(
        "feat-human-task",
        "feat/human",
        "/worktrees/feat-human",
        PullRequestRef {
            number: 2,
            url: "https://github.com/org/repo/pull/2".to_string(),
        },
        &template,
    )
    .expect("feature should initialize from template");

    feature
        .evaluate_gates(&template, &BuiltinEvidence::new())
        .expect("gate evaluation should succeed");

    let gate = feature.gate_groups[0]
        .gates
        .first()
        .expect("gate should exist");
    assert_eq!(gate.status, GateStatus::Manual);
}

#[test]
fn feature_state_maps_manual_builtin_evidence_to_manual_gate_status() {
    let template = load_embedded_template_set().expect("embedded template should load");
    let mut feature = FeatureState::from_template(
        "feat-auth-refresh",
        "feat/123-token-refresh",
        "/worktrees/feat-123-token-refresh",
        PullRequestRef {
            number: 1,
            url: "https://github.com/org/repo/pull/1".to_string(),
        },
        &template,
    )
    .expect("feature should initialize from template");

    feature
        .evaluate_gates(
            &template,
            &BuiltinEvidence::new()
                .with_status("builtin.github.pr_review_approved", EvidenceStatus::Manual),
        )
        .expect("gate evaluation should succeed");

    let review_gate = feature
        .gate_groups
        .iter()
        .flat_map(|group| group.gates.iter())
        .find(|gate| gate.id == "feature-pr-reviewed")
        .expect("review gate should exist");

    assert_eq!(review_gate.status, GateStatus::Manual);
}

// --- WorkflowState::as_str ---

#[test]
fn workflow_state_as_str_returns_expected_slugs() {
    assert_eq!(WorkflowState::New.as_str(), "new");
    assert_eq!(WorkflowState::PrdReview.as_str(), "prd-review");
    assert_eq!(
        WorkflowState::ArchitecturePlan.as_str(),
        "architecture-plan"
    );
    assert_eq!(WorkflowState::ScaffoldTdd.as_str(), "scaffold-tdd");
    assert_eq!(
        WorkflowState::ArchitectureReview.as_str(),
        "architecture-review"
    );
    assert_eq!(WorkflowState::Implementation.as_str(), "implementation");
    assert_eq!(WorkflowState::QaValidation.as_str(), "qa-validation");
    assert_eq!(WorkflowState::ReleaseReady.as_str(), "release-ready");
    assert_eq!(WorkflowState::Done.as_str(), "done");
    assert_eq!(WorkflowState::Blocked.as_str(), "blocked");
    assert_eq!(WorkflowState::Aborted.as_str(), "aborted");
    // Deprecated aliases map to their canonical equivalents
    assert_eq!(WorkflowState::WaitingForHuman.as_str(), "implementation");
    assert_eq!(WorkflowState::ReadyForReview.as_str(), "release-ready");
}

// --- WorkflowState::available_transitions ---

#[test]
fn workflow_state_new_transitions_to_prd_review_when_binding_complete() {
    let facts = TransitionFacts {
        feature_binding_complete: true,
        ..TransitionFacts::default()
    };
    assert_eq!(
        WorkflowState::New.available_transitions(&facts),
        vec![WorkflowState::PrdReview]
    );
}

#[test]
fn workflow_state_new_has_no_transitions_when_binding_incomplete() {
    let facts = TransitionFacts::default();
    assert!(WorkflowState::New.available_transitions(&facts).is_empty());
}

#[test]
fn workflow_state_implementation_transitions_to_all_valid_targets() {
    let facts = TransitionFacts {
        blocking_issue_present: true,
        ready_for_review: true,
        aborted: true,
        ..TransitionFacts::default()
    };
    let transitions = WorkflowState::Implementation.available_transitions(&facts);
    assert!(transitions.contains(&WorkflowState::QaValidation));
    assert!(transitions.contains(&WorkflowState::Blocked));
    assert!(transitions.contains(&WorkflowState::Aborted));
}

#[test]
fn workflow_state_waiting_for_human_transitions_based_on_facts() {
    let facts = TransitionFacts {
        blocking_issue_present: true,
        human_response_ready: true,
        ..TransitionFacts::default()
    };
    let transitions = WorkflowState::WaitingForHuman.available_transitions(&facts);
    assert!(transitions.contains(&WorkflowState::Blocked));
    assert!(transitions.contains(&WorkflowState::Implementation));
}

#[test]
fn workflow_state_ready_for_review_transitions_based_on_facts() {
    let facts = TransitionFacts {
        blocking_issue_present: true,
        review_rework_required: true,
        ..TransitionFacts::default()
    };
    let transitions = WorkflowState::ReadyForReview.available_transitions(&facts);
    assert!(transitions.contains(&WorkflowState::Blocked));
    assert!(transitions.contains(&WorkflowState::Implementation));
}

#[test]
fn workflow_state_blocked_transitions_to_all_active_states_when_blocker_resolved() {
    let facts = TransitionFacts {
        blocker_resolved: true,
        ..TransitionFacts::default()
    };
    let transitions = WorkflowState::Blocked.available_transitions(&facts);
    // All 8 non-terminal active states should be offered
    assert!(transitions.contains(&WorkflowState::New));
    assert!(transitions.contains(&WorkflowState::PrdReview));
    assert!(transitions.contains(&WorkflowState::ArchitecturePlan));
    assert!(transitions.contains(&WorkflowState::ScaffoldTdd));
    assert!(transitions.contains(&WorkflowState::ArchitectureReview));
    assert!(transitions.contains(&WorkflowState::Implementation));
    assert!(transitions.contains(&WorkflowState::QaValidation));
    assert!(transitions.contains(&WorkflowState::ReleaseReady));
    assert!(!transitions.contains(&WorkflowState::Done));
    assert!(!transitions.contains(&WorkflowState::Aborted));
}

// --- WorkflowState::validate_transition ---

#[test]
fn workflow_state_validate_transition_succeeds_for_valid_transitions() {
    let facts = TransitionFacts {
        feature_binding_complete: true,
        ..TransitionFacts::default()
    };
    assert!(
        WorkflowState::New
            .validate_transition(WorkflowState::PrdReview, &facts)
            .is_ok()
    );
}

#[test]
fn workflow_state_validate_transition_rejects_unsupported_transition() {
    let facts = TransitionFacts::default();
    let error = WorkflowState::New
        .validate_transition(WorkflowState::Blocked, &facts)
        .expect_err("unsupported transition should fail");
    assert!(matches!(error, TransitionError::Rejected { .. }));
    assert!(error.to_string().contains("cannot transition from"));
    assert!(error.to_string().contains("'new'"));
}

#[test]
fn workflow_state_validate_transition_rejects_all_invalid_pairs() {
    let facts = TransitionFacts::default();

    let invalid_pairs = [
        // New can only go to PrdReview (with binding complete) — not directly to others
        (WorkflowState::New, WorkflowState::PrdReview),
        (WorkflowState::New, WorkflowState::Implementation),
        (WorkflowState::New, WorkflowState::Blocked),
        (WorkflowState::New, WorkflowState::Aborted),
        // Stage forward transitions without stage_complete
        (WorkflowState::PrdReview, WorkflowState::ArchitecturePlan),
        (WorkflowState::PrdReview, WorkflowState::Blocked),
        (WorkflowState::ArchitecturePlan, WorkflowState::ScaffoldTdd),
        (
            WorkflowState::ScaffoldTdd,
            WorkflowState::ArchitectureReview,
        ),
        (
            WorkflowState::ArchitectureReview,
            WorkflowState::Implementation,
        ),
        // Implementation → QaValidation requires ready_for_review
        (WorkflowState::Implementation, WorkflowState::QaValidation),
        (WorkflowState::Implementation, WorkflowState::Blocked),
        // QaValidation transitions require respective facts
        (WorkflowState::QaValidation, WorkflowState::ReleaseReady),
        (WorkflowState::QaValidation, WorkflowState::Implementation),
        (WorkflowState::QaValidation, WorkflowState::Blocked),
        // ReleaseReady → Done requires stage_complete
        (WorkflowState::ReleaseReady, WorkflowState::Done),
        (WorkflowState::ReleaseReady, WorkflowState::Blocked),
        // Terminal states have no transitions
        (WorkflowState::Done, WorkflowState::New),
        (WorkflowState::Done, WorkflowState::Implementation),
        (WorkflowState::Aborted, WorkflowState::New),
        (WorkflowState::Aborted, WorkflowState::Implementation),
        // Blocked without blocker_resolved
        (WorkflowState::Blocked, WorkflowState::Implementation),
        (WorkflowState::Blocked, WorkflowState::New),
    ];

    for (from, to) in invalid_pairs {
        let result = from.clone().validate_transition(to.clone(), &facts);
        assert!(
            result.is_err(),
            "expected {from} -> {to} to be rejected with empty facts"
        );
    }
}

#[test]
fn workflow_state_missing_transition_reason_formats_for_all_pairs() {
    // Exercises the wildcard arm of missing_transition_reason
    let facts = TransitionFacts::default();
    let error = WorkflowState::New
        .validate_transition(WorkflowState::QaValidation, &facts)
        .expect_err("unsupported transition should fail");
    assert!(error.to_string().contains("not supported"));
}

// --- GateGroup::rollup and rollup_status ---

#[test]
fn gate_group_rollup_status_is_blocked_when_any_gate_is_failing() {
    let group = GateGroup {
        id: "g".to_string(),
        label: "G".to_string(),
        gates: vec![
            Gate {
                id: "a".to_string(),
                label: "A".to_string(),
                task: "t".to_string(),
                status: GateStatus::Passing,
            },
            Gate {
                id: "b".to_string(),
                label: "B".to_string(),
                task: "t".to_string(),
                status: GateStatus::Failing,
            },
        ],
    };
    assert_eq!(group.rollup_status(), GateGroupStatus::Blocked);
}

#[test]
fn gate_group_rollup_status_is_pending_when_no_failing_but_some_pending() {
    let group = GateGroup {
        id: "g".to_string(),
        label: "G".to_string(),
        gates: vec![
            Gate {
                id: "a".to_string(),
                label: "A".to_string(),
                task: "t".to_string(),
                status: GateStatus::Passing,
            },
            Gate {
                id: "b".to_string(),
                label: "B".to_string(),
                task: "t".to_string(),
                status: GateStatus::Pending,
            },
        ],
    };
    assert_eq!(group.rollup_status(), GateGroupStatus::Pending);
}

#[test]
fn gate_group_rollup_status_is_manual_when_only_manual_and_passing() {
    let group = GateGroup {
        id: "g".to_string(),
        label: "G".to_string(),
        gates: vec![
            Gate {
                id: "a".to_string(),
                label: "A".to_string(),
                task: "t".to_string(),
                status: GateStatus::Passing,
            },
            Gate {
                id: "b".to_string(),
                label: "B".to_string(),
                task: "t".to_string(),
                status: GateStatus::Manual,
            },
        ],
    };
    assert_eq!(group.rollup_status(), GateGroupStatus::Manual);
}

#[test]
fn gate_group_rollup_status_is_passing_when_all_gates_pass() {
    let group = GateGroup {
        id: "g".to_string(),
        label: "G".to_string(),
        gates: vec![Gate {
            id: "a".to_string(),
            label: "A".to_string(),
            task: "t".to_string(),
            status: GateStatus::Passing,
        }],
    };
    assert_eq!(group.rollup_status(), GateGroupStatus::Passing);
}

#[test]
fn gate_group_rollup_captures_blocking_gate_ids() {
    let group = GateGroup {
        id: "validation".to_string(),
        label: "Validation".to_string(),
        gates: vec![
            Gate {
                id: "gate-pass".to_string(),
                label: "Pass".to_string(),
                task: "t".to_string(),
                status: GateStatus::Passing,
            },
            Gate {
                id: "gate-fail".to_string(),
                label: "Fail".to_string(),
                task: "t".to_string(),
                status: GateStatus::Failing,
            },
        ],
    };
    let rollup = group.rollup();
    assert_eq!(rollup.id, "validation");
    assert_eq!(rollup.status, GateGroupStatus::Blocked);
    assert_eq!(rollup.blocking_gate_ids, vec!["gate-fail".to_string()]);
}

// --- FeatureState transition helpers ---

#[test]
fn feature_state_gate_group_rollups_returns_one_rollup_per_group() {
    let template = load_embedded_template_set().expect("embedded template should load");
    let feature = FeatureState::from_template(
        "feat-rollup",
        "feat/rollup",
        "/worktrees/feat-rollup",
        PullRequestRef {
            number: 1,
            url: "https://github.com/org/repo/pull/1".to_string(),
        },
        &template,
    )
    .expect("feature should initialize from template");

    let rollups = feature.gate_group_rollups();
    assert_eq!(rollups.len(), feature.gate_groups.len());
}

#[test]
fn feature_state_transition_to_succeeds_for_valid_transition() {
    let template = load_embedded_template_set().expect("embedded template should load");
    let mut feature = FeatureState::from_template(
        "feat-transition-valid",
        "feat/transition",
        "/worktrees/feat-transition",
        PullRequestRef {
            number: 3,
            url: "https://github.com/org/repo/pull/3".to_string(),
        },
        &template,
    )
    .expect("feature should initialize from template");

    let facts = TransitionFacts {
        feature_binding_complete: true,
        ..TransitionFacts::default()
    };
    feature
        .transition_to(WorkflowState::PrdReview, &facts)
        .expect("valid transition should succeed");

    assert_eq!(feature.workflow_state, WorkflowState::PrdReview);
}

#[test]
fn feature_state_transition_to_rejects_invalid_transition() {
    let template = load_embedded_template_set().expect("embedded template should load");
    let mut feature = FeatureState::from_template(
        "feat-transition-invalid",
        "feat/transition",
        "/worktrees/feat-transition",
        PullRequestRef {
            number: 4,
            url: "https://github.com/org/repo/pull/4".to_string(),
        },
        &template,
    )
    .expect("feature should initialize from template");

    let facts = TransitionFacts::default();
    let error = feature
        .transition_to(WorkflowState::Blocked, &facts)
        .expect_err("invalid transition should fail");

    assert!(matches!(error, TransitionError::Rejected { .. }));
}

// --- 11-state lifecycle: new tests ---

#[test]
fn workflow_state_every_valid_forward_transition_succeeds() {
    let forward_pairs = [
        (
            WorkflowState::New,
            WorkflowState::PrdReview,
            TransitionFacts {
                feature_binding_complete: true,
                ..TransitionFacts::default()
            },
        ),
        (
            WorkflowState::PrdReview,
            WorkflowState::ArchitecturePlan,
            TransitionFacts {
                stage_complete: true,
                ..TransitionFacts::default()
            },
        ),
        (
            WorkflowState::ArchitecturePlan,
            WorkflowState::ScaffoldTdd,
            TransitionFacts {
                stage_complete: true,
                ..TransitionFacts::default()
            },
        ),
        (
            WorkflowState::ScaffoldTdd,
            WorkflowState::ArchitectureReview,
            TransitionFacts {
                stage_complete: true,
                ..TransitionFacts::default()
            },
        ),
        (
            WorkflowState::ArchitectureReview,
            WorkflowState::Implementation,
            TransitionFacts {
                stage_complete: true,
                ..TransitionFacts::default()
            },
        ),
        (
            WorkflowState::Implementation,
            WorkflowState::QaValidation,
            TransitionFacts {
                ready_for_review: true,
                ..TransitionFacts::default()
            },
        ),
        (
            WorkflowState::QaValidation,
            WorkflowState::ReleaseReady,
            TransitionFacts {
                stage_complete: true,
                ..TransitionFacts::default()
            },
        ),
        (
            WorkflowState::QaValidation,
            WorkflowState::Implementation,
            TransitionFacts {
                review_rework_required: true,
                ..TransitionFacts::default()
            },
        ),
        (
            WorkflowState::ReleaseReady,
            WorkflowState::Done,
            TransitionFacts {
                stage_complete: true,
                ..TransitionFacts::default()
            },
        ),
    ];

    for (from, to, facts) in forward_pairs {
        assert!(
            from.clone().validate_transition(to.clone(), &facts).is_ok(),
            "expected {from} -> {to} to succeed"
        );
    }
}

#[test]
fn workflow_state_any_non_terminal_can_transition_to_blocked_and_aborted() {
    let active_states = [
        WorkflowState::New,
        WorkflowState::PrdReview,
        WorkflowState::ArchitecturePlan,
        WorkflowState::ScaffoldTdd,
        WorkflowState::ArchitectureReview,
        WorkflowState::Implementation,
        WorkflowState::QaValidation,
        WorkflowState::ReleaseReady,
    ];

    let block_facts = TransitionFacts {
        blocking_issue_present: true,
        ..TransitionFacts::default()
    };
    let abort_facts = TransitionFacts {
        aborted: true,
        ..TransitionFacts::default()
    };

    for state in &active_states {
        assert!(
            state
                .clone()
                .validate_transition(WorkflowState::Blocked, &block_facts)
                .is_ok(),
            "expected {state} -> Blocked to succeed"
        );
        assert!(
            state
                .clone()
                .validate_transition(WorkflowState::Aborted, &abort_facts)
                .is_ok(),
            "expected {state} -> Aborted to succeed"
        );
    }
}

#[test]
fn workflow_state_done_and_aborted_are_terminal_with_no_transitions() {
    let facts = TransitionFacts {
        feature_binding_complete: true,
        blocking_issue_present: true,
        stage_complete: true,
        ready_for_review: true,
        blocker_resolved: true,
        aborted: true,
        ..TransitionFacts::default()
    };

    assert!(
        WorkflowState::Done.available_transitions(&facts).is_empty(),
        "Done should have no transitions"
    );
    assert!(
        WorkflowState::Aborted
            .available_transitions(&facts)
            .is_empty(),
        "Aborted should have no transitions"
    );

    let error = WorkflowState::Done
        .validate_transition(WorkflowState::New, &facts)
        .expect_err("Done -> New should be rejected");
    assert!(error.to_string().contains("terminal"));

    let error = WorkflowState::Aborted
        .validate_transition(WorkflowState::New, &facts)
        .expect_err("Aborted -> New should be rejected");
    assert!(error.to_string().contains("terminal"));
}

#[test]
fn workflow_state_blocked_can_unblock_to_every_non_terminal_state() {
    let active_states = [
        WorkflowState::New,
        WorkflowState::PrdReview,
        WorkflowState::ArchitecturePlan,
        WorkflowState::ScaffoldTdd,
        WorkflowState::ArchitectureReview,
        WorkflowState::Implementation,
        WorkflowState::QaValidation,
        WorkflowState::ReleaseReady,
    ];

    for target in &active_states {
        let facts = TransitionFacts {
            blocker_resolved: true,
            target_unblock_state: Some(target.clone()),
            ..TransitionFacts::default()
        };
        assert!(
            WorkflowState::Blocked
                .validate_transition(target.clone(), &facts)
                .is_ok(),
            "Blocked -> {target} should succeed when blocker resolved"
        );
    }

    // Terminal states must not be offered
    let facts = TransitionFacts {
        blocker_resolved: true,
        ..TransitionFacts::default()
    };
    let transitions = WorkflowState::Blocked.available_transitions(&facts);
    assert!(!transitions.contains(&WorkflowState::Done));
    assert!(!transitions.contains(&WorkflowState::Aborted));
}

#[test]
fn old_kebab_case_waiting_for_human_deserializes_without_error() {
    let json = "\"waiting-for-human\"";
    let state: WorkflowState = serde_json::from_str(json)
        .expect("waiting-for-human should deserialize as deprecated alias");
    assert_eq!(state, WorkflowState::WaitingForHuman);
    // as_str returns the canonical equivalent
    assert_eq!(state.as_str(), "implementation");
}

#[test]
fn old_kebab_case_ready_for_review_deserializes_without_error() {
    let json = "\"ready-for-review\"";
    let state: WorkflowState = serde_json::from_str(json)
        .expect("ready-for-review should deserialize as deprecated alias");
    assert_eq!(state, WorkflowState::ReadyForReview);
    // as_str returns the canonical equivalent
    assert_eq!(state.as_str(), "release-ready");
}

#[test]
fn old_state_file_with_waiting_for_human_loads_without_panic() {
    let json = r#"{
        "version": 1,
        "repo_id": "legacy",
        "current_feature": {
            "feature_id": "feat-legacy",
            "branch": "feat/legacy",
            "worktree_path": "/worktrees/legacy",
            "pull_request": {"number": 1, "url": "https://github.com/o/r/pull/1"},
            "workflow_state": "waiting-for-human",
            "gate_groups": [],
            "active_sessions": []
        }
    }"#;
    let state =
        RepositoryState::from_json(json).expect("old state with waiting-for-human should load");
    assert_eq!(
        state.current_feature.workflow_state,
        WorkflowState::WaitingForHuman
    );
}

#[test]
fn old_state_file_with_ready_for_review_loads_without_panic() {
    let json = r#"{
        "version": 1,
        "repo_id": "legacy",
        "current_feature": {
            "feature_id": "feat-legacy",
            "branch": "feat/legacy",
            "worktree_path": "/worktrees/legacy",
            "pull_request": {"number": 1, "url": "https://github.com/o/r/pull/1"},
            "workflow_state": "ready-for-review",
            "gate_groups": [],
            "active_sessions": []
        }
    }"#;
    let state =
        RepositoryState::from_json(json).expect("old state with ready-for-review should load");
    assert_eq!(
        state.current_feature.workflow_state,
        WorkflowState::ReadyForReview
    );
}

#[test]
fn embedded_template_parses_and_references_all_11_states() {
    let template = load_embedded_template_set().expect("embedded template should load");

    let expected_states = [
        "new",
        "prd-review",
        "architecture-plan",
        "scaffold-tdd",
        "architecture-review",
        "implementation",
        "qa-validation",
        "release-ready",
        "done",
        "blocked",
        "aborted",
    ];

    for state in expected_states {
        assert!(
            template
                .state_machine
                .states
                .iter()
                .any(|s| s.name() == state),
            "embedded template should include state '{state}'"
        );
    }

    assert_eq!(template.state_machine.initial_state, "new");
}

#[test]
fn workflow_state_valid_next_states_covers_full_transition_matrix() {
    assert_eq!(
        WorkflowState::New.valid_next_states(),
        vec![
            WorkflowState::PrdReview,
            WorkflowState::Blocked,
            WorkflowState::Aborted
        ]
    );
    assert_eq!(WorkflowState::Done.valid_next_states(), vec![]);
    assert_eq!(WorkflowState::Aborted.valid_next_states(), vec![]);
    // Blocked offers all 8 active states
    assert_eq!(WorkflowState::Blocked.valid_next_states().len(), 8);
}

#[test]
fn workflow_state_every_advertised_transition_is_reachable() {
    // For every (source, target) pair advertised by valid_next_states(), construct
    // the minimal TransitionFacts that satisfy validate_transition and assert Ok(()).
    // If a developer removes a target from valid_next_states() or tightens
    // validate_transition so a previously-reachable edge becomes unreachable, this
    // test catches it — unlike the old mirror test which only verified the list itself.

    let sufficient_facts = |source: &WorkflowState, target: &WorkflowState| -> TransitionFacts {
        let mut facts = TransitionFacts::default();
        match (source, target) {
            // New
            (WorkflowState::New, WorkflowState::PrdReview) => {
                facts.feature_binding_complete = true;
            }
            (_, WorkflowState::Blocked) => {
                facts.blocking_issue_present = true;
            }
            (_, WorkflowState::Aborted) => {
                facts.aborted = true;
            }
            // Linear forward edges that use stage_complete
            (WorkflowState::PrdReview, WorkflowState::ArchitecturePlan)
            | (WorkflowState::ArchitecturePlan, WorkflowState::ScaffoldTdd)
            | (WorkflowState::ScaffoldTdd, WorkflowState::ArchitectureReview)
            | (WorkflowState::ArchitectureReview, WorkflowState::Implementation)
            | (WorkflowState::QaValidation, WorkflowState::ReleaseReady)
            | (WorkflowState::ReleaseReady, WorkflowState::Done) => {
                facts.stage_complete = true;
            }
            // Implementation -> QaValidation
            (WorkflowState::Implementation, WorkflowState::QaValidation) => {
                facts.ready_for_review = true;
            }
            // QaValidation -> Implementation (rework)
            (WorkflowState::QaValidation, WorkflowState::Implementation) => {
                facts.review_rework_required = true;
            }
            // Blocked -> any active state
            (WorkflowState::Blocked, target_state) => {
                facts.blocker_resolved = true;
                facts.target_unblock_state = Some(target_state.clone());
            }
            // Deprecated: WaitingForHuman -> QaValidation (maps to Implementation path)
            (WorkflowState::WaitingForHuman, WorkflowState::QaValidation) => {
                facts.human_response_ready = true;
            }
            // Deprecated: ReadyForReview -> Done
            (WorkflowState::ReadyForReview, WorkflowState::Done) => {
                facts.stage_complete = true;
            }
            _ => {}
        }
        facts
    };

    // Deprecated alias variants (WaitingForHuman, ReadyForReview) have intentional
    // inconsistencies between valid_next_states() and available_transitions() and are
    // excluded from this round-trip test; they are covered by the backward-compat tests.
    let all_sources = [
        WorkflowState::New,
        WorkflowState::PrdReview,
        WorkflowState::ArchitecturePlan,
        WorkflowState::ScaffoldTdd,
        WorkflowState::ArchitectureReview,
        WorkflowState::Implementation,
        WorkflowState::QaValidation,
        WorkflowState::ReleaseReady,
        WorkflowState::Done,
        WorkflowState::Aborted,
        WorkflowState::Blocked,
    ];

    for source in &all_sources {
        for target in source.valid_next_states() {
            let facts = sufficient_facts(source, &target);
            let result = source.validate_transition(target.clone(), &facts);
            assert!(
                result.is_ok(),
                "expected {:?} -> {:?} to succeed with facts {:?}, got {:?}",
                source,
                target,
                facts,
                result.unwrap_err()
            );
        }
    }
}

#[test]
fn workflow_state_missing_transition_reason_covers_all_reject_arms() {
    let facts = TransitionFacts::default();

    // PrdReview -> Aborted (abort flag not set)
    let err = WorkflowState::PrdReview
        .validate_transition(WorkflowState::Aborted, &facts)
        .unwrap_err();
    assert!(err.to_string().contains("abort flag is not set"), "{err}");

    // ArchitecturePlan -> Blocked / Aborted
    let err = WorkflowState::ArchitecturePlan
        .validate_transition(WorkflowState::Blocked, &facts)
        .unwrap_err();
    assert!(
        err.to_string().contains("no blocking issue is present"),
        "{err}"
    );

    let err = WorkflowState::ArchitecturePlan
        .validate_transition(WorkflowState::Aborted, &facts)
        .unwrap_err();
    assert!(err.to_string().contains("abort flag is not set"), "{err}");

    // ScaffoldTdd -> Blocked / Aborted
    let err = WorkflowState::ScaffoldTdd
        .validate_transition(WorkflowState::Blocked, &facts)
        .unwrap_err();
    assert!(
        err.to_string().contains("no blocking issue is present"),
        "{err}"
    );

    let err = WorkflowState::ScaffoldTdd
        .validate_transition(WorkflowState::Aborted, &facts)
        .unwrap_err();
    assert!(err.to_string().contains("abort flag is not set"), "{err}");

    // ArchitectureReview -> Blocked / Aborted
    let err = WorkflowState::ArchitectureReview
        .validate_transition(WorkflowState::Blocked, &facts)
        .unwrap_err();
    assert!(
        err.to_string().contains("no blocking issue is present"),
        "{err}"
    );

    let err = WorkflowState::ArchitectureReview
        .validate_transition(WorkflowState::Aborted, &facts)
        .unwrap_err();
    assert!(err.to_string().contains("abort flag is not set"), "{err}");

    // Implementation -> Aborted
    let err = WorkflowState::Implementation
        .validate_transition(WorkflowState::Aborted, &facts)
        .unwrap_err();
    assert!(err.to_string().contains("abort flag is not set"), "{err}");

    // QaValidation -> Aborted
    let err = WorkflowState::QaValidation
        .validate_transition(WorkflowState::Aborted, &facts)
        .unwrap_err();
    assert!(err.to_string().contains("abort flag is not set"), "{err}");

    // ReleaseReady -> Aborted
    let err = WorkflowState::ReleaseReady
        .validate_transition(WorkflowState::Aborted, &facts)
        .unwrap_err();
    assert!(err.to_string().contains("abort flag is not set"), "{err}");

    // WaitingForHuman -> Blocked (deprecated)
    let err = WorkflowState::WaitingForHuman
        .validate_transition(WorkflowState::Blocked, &facts)
        .unwrap_err();
    assert!(
        err.to_string().contains("no blocking issue is present"),
        "{err}"
    );

    // WaitingForHuman -> Implementation (deprecated)
    let err = WorkflowState::WaitingForHuman
        .validate_transition(WorkflowState::Implementation, &facts)
        .unwrap_err();
    assert!(
        err.to_string().contains("no human response is available"),
        "{err}"
    );

    // ReadyForReview -> Blocked (deprecated)
    let err = WorkflowState::ReadyForReview
        .validate_transition(WorkflowState::Blocked, &facts)
        .unwrap_err();
    assert!(
        err.to_string().contains("no blocking issue is present"),
        "{err}"
    );

    // ReadyForReview -> Implementation (deprecated)
    let err = WorkflowState::ReadyForReview
        .validate_transition(WorkflowState::Implementation, &facts)
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("no follow-up implementation request is present"),
        "{err}"
    );
}
