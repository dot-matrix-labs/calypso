use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::state::{
    AgentSession, AgentSessionStatus, BuiltinEvidence, EvidenceStatus, FeatureState, FeatureType,
    Gate, GateEvaluationError, GateGroup, GateGroupStatus, GateStatus, PullRequestRef,
    RepositoryIdentity, RepositoryState, SchedulingMeta, SessionOutput, SessionOutputStream,
    StateError,
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
            workflow_state: "implementation".to_string(),
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
    );

    assert_eq!(feature.workflow_state, "new");
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
    );
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
    );

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
    );

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
    );

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
    );
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
    );

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
    );

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
    );

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
    );

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
    );

    let rollups = feature.gate_group_rollups();
    assert_eq!(rollups.len(), feature.gate_groups.len());
}
