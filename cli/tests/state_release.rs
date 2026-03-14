use calypso_cli::state::{
    DeploymentRecord, DeploymentState, DeploymentTransitionError, FeatureState, FeatureType,
    GateGroup, PullRequestRef, ReleaseRecord, ReleaseState, ReleaseTransitionError,
    RepositoryIdentity, RepositoryState, SchedulingMeta, WorkflowState,
};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn minimal_feature() -> FeatureState {
    FeatureState {
        feature_id: "feat-x".to_string(),
        branch: "feat/x".to_string(),
        worktree_path: "/worktrees/x".to_string(),
        pull_request: PullRequestRef {
            number: 1,
            url: "https://github.com/org/repo/pull/1".to_string(),
        },
        github_snapshot: None,
        github_error: None,
        workflow_state: WorkflowState::New,
        gate_groups: Vec::new(),
        active_sessions: Vec::new(),
        feature_type: FeatureType::Feat,
        roles: Vec::new(),
        scheduling: SchedulingMeta::default(),
        artifact_refs: Vec::new(),
        transcript_refs: Vec::new(),
        clarification_history: Vec::new(),
    }
}

fn minimal_repo_state() -> RepositoryState {
    RepositoryState {
        version: 1,
        schema_version: 1,
        repo_id: "test-repo".to_string(),
        identity: RepositoryIdentity::default(),
        providers: Vec::new(),
        github_auth_ref: None,
        secure_key_refs: Vec::new(),
        active_features: Vec::new(),
        known_worktrees: Vec::new(),
        releases: Vec::new(),
        deployments: Vec::new(),
        current_feature: minimal_feature(),
    }
}

fn sample_release(state: ReleaseState) -> ReleaseRecord {
    ReleaseRecord {
        release_id: "rel-001".to_string(),
        candidate_version: "1.2.0".to_string(),
        state,
        validation_ref: None,
        approval_ref: None,
        deployment_ref: None,
        rollback_state: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

fn sample_deployment(env: &str, state: DeploymentState) -> DeploymentRecord {
    DeploymentRecord {
        deployment_id: format!("dep-{env}-001"),
        environment: env.to_string(),
        desired_code_version: "1.2.0".to_string(),
        deployed_code_version: None,
        desired_migration_version: None,
        deployed_migration_version: None,
        state,
        last_result: None,
        rollback_target: None,
        actor: None,
        created_at: "2026-01-01T00:00:00Z".to_string(),
        updated_at: "2026-01-01T00:00:00Z".to_string(),
    }
}

// ---------------------------------------------------------------------------
// ReleaseState — valid transitions
// ---------------------------------------------------------------------------

#[test]
fn release_state_planned_transitions_to_in_progress() {
    assert!(
        ReleaseState::Planned
            .validate_transition(&ReleaseState::InProgress)
            .is_ok()
    );
}

#[test]
fn release_state_planned_transitions_to_aborted() {
    assert!(
        ReleaseState::Planned
            .validate_transition(&ReleaseState::Aborted)
            .is_ok()
    );
}

#[test]
fn release_state_in_progress_transitions_to_candidate() {
    assert!(
        ReleaseState::InProgress
            .validate_transition(&ReleaseState::Candidate)
            .is_ok()
    );
}

#[test]
fn release_state_candidate_transitions_to_validated() {
    assert!(
        ReleaseState::Candidate
            .validate_transition(&ReleaseState::Validated)
            .is_ok()
    );
}

#[test]
fn release_state_candidate_transitions_back_to_in_progress() {
    assert!(
        ReleaseState::Candidate
            .validate_transition(&ReleaseState::InProgress)
            .is_ok()
    );
}

#[test]
fn release_state_validated_transitions_to_approved() {
    assert!(
        ReleaseState::Validated
            .validate_transition(&ReleaseState::Approved)
            .is_ok()
    );
}

#[test]
fn release_state_validated_transitions_back_to_candidate() {
    assert!(
        ReleaseState::Validated
            .validate_transition(&ReleaseState::Candidate)
            .is_ok()
    );
}

#[test]
fn release_state_approved_transitions_to_deployed() {
    assert!(
        ReleaseState::Approved
            .validate_transition(&ReleaseState::Deployed)
            .is_ok()
    );
}

#[test]
fn release_state_deployed_transitions_to_rolled_back() {
    assert!(
        ReleaseState::Deployed
            .validate_transition(&ReleaseState::RolledBack)
            .is_ok()
    );
}

// ---------------------------------------------------------------------------
// ReleaseState — invalid transitions
// ---------------------------------------------------------------------------

#[test]
fn release_state_planned_rejects_direct_jump_to_deployed() {
    let err = ReleaseState::Planned
        .validate_transition(&ReleaseState::Deployed)
        .expect_err("should be rejected");
    assert!(matches!(err, ReleaseTransitionError::Rejected { .. }));
    assert!(err.to_string().contains("cannot transition release from"));
    assert!(err.to_string().contains("planned"));
    assert!(err.to_string().contains("deployed"));
}

#[test]
fn release_state_in_progress_rejects_jump_to_approved() {
    let err = ReleaseState::InProgress
        .validate_transition(&ReleaseState::Approved)
        .expect_err("should be rejected");
    assert!(matches!(err, ReleaseTransitionError::Rejected { .. }));
}

#[test]
fn release_state_approved_rejects_back_transition_to_validated() {
    let err = ReleaseState::Approved
        .validate_transition(&ReleaseState::Validated)
        .expect_err("should be rejected");
    assert!(matches!(err, ReleaseTransitionError::Rejected { .. }));
}

// ---------------------------------------------------------------------------
// Terminal states — RolledBack and Aborted
// ---------------------------------------------------------------------------

#[test]
fn release_state_rolled_back_is_terminal() {
    assert!(ReleaseState::RolledBack.is_terminal());
    assert!(ReleaseState::RolledBack.valid_next_states().is_empty());
}

#[test]
fn release_state_aborted_is_terminal() {
    assert!(ReleaseState::Aborted.is_terminal());
    assert!(ReleaseState::Aborted.valid_next_states().is_empty());
}

#[test]
fn release_state_rolled_back_rejects_any_transition() {
    let err = ReleaseState::RolledBack
        .validate_transition(&ReleaseState::Planned)
        .expect_err("terminal state should reject all transitions");
    assert!(err.to_string().contains("terminal"));
}

#[test]
fn release_state_aborted_rejects_any_transition() {
    let err = ReleaseState::Aborted
        .validate_transition(&ReleaseState::InProgress)
        .expect_err("terminal state should reject all transitions");
    assert!(err.to_string().contains("terminal"));
}

#[test]
fn release_state_non_terminal_states_are_not_terminal() {
    for state in [
        ReleaseState::Planned,
        ReleaseState::InProgress,
        ReleaseState::Candidate,
        ReleaseState::Validated,
        ReleaseState::Approved,
        ReleaseState::Deployed,
    ] {
        assert!(!state.is_terminal(), "{state} should not be terminal");
    }
}

// ---------------------------------------------------------------------------
// DeploymentState — valid transitions
// ---------------------------------------------------------------------------

#[test]
fn deployment_state_idle_transitions_to_pending() {
    assert!(
        DeploymentState::Idle
            .validate_transition(&DeploymentState::Pending)
            .is_ok()
    );
}

#[test]
fn deployment_state_pending_transitions_to_deploying() {
    assert!(
        DeploymentState::Pending
            .validate_transition(&DeploymentState::Deploying)
            .is_ok()
    );
}

#[test]
fn deployment_state_pending_transitions_to_idle_on_cancel() {
    assert!(
        DeploymentState::Pending
            .validate_transition(&DeploymentState::Idle)
            .is_ok()
    );
}

#[test]
fn deployment_state_deploying_transitions_to_deployed() {
    assert!(
        DeploymentState::Deploying
            .validate_transition(&DeploymentState::Deployed)
            .is_ok()
    );
}

#[test]
fn deployment_state_deploying_transitions_to_failed() {
    assert!(
        DeploymentState::Deploying
            .validate_transition(&DeploymentState::Failed)
            .is_ok()
    );
}

#[test]
fn deployment_state_deployed_transitions_to_rolling_back() {
    assert!(
        DeploymentState::Deployed
            .validate_transition(&DeploymentState::RollingBack)
            .is_ok()
    );
}

#[test]
fn deployment_state_deployed_transitions_to_idle_for_new_deploy() {
    assert!(
        DeploymentState::Deployed
            .validate_transition(&DeploymentState::Idle)
            .is_ok()
    );
}

#[test]
fn deployment_state_failed_transitions_to_rolling_back() {
    assert!(
        DeploymentState::Failed
            .validate_transition(&DeploymentState::RollingBack)
            .is_ok()
    );
}

#[test]
fn deployment_state_failed_transitions_to_idle_for_retry() {
    assert!(
        DeploymentState::Failed
            .validate_transition(&DeploymentState::Idle)
            .is_ok()
    );
}

#[test]
fn deployment_state_rolling_back_transitions_to_rolled_back() {
    assert!(
        DeploymentState::RollingBack
            .validate_transition(&DeploymentState::RolledBack)
            .is_ok()
    );
}

#[test]
fn deployment_state_rolling_back_transitions_to_failed() {
    assert!(
        DeploymentState::RollingBack
            .validate_transition(&DeploymentState::Failed)
            .is_ok()
    );
}

#[test]
fn deployment_state_rolled_back_transitions_to_idle() {
    assert!(
        DeploymentState::RolledBack
            .validate_transition(&DeploymentState::Idle)
            .is_ok()
    );
}

// ---------------------------------------------------------------------------
// DeploymentState — invalid transitions
// ---------------------------------------------------------------------------

#[test]
fn deployment_state_idle_rejects_jump_to_deployed() {
    let err = DeploymentState::Idle
        .validate_transition(&DeploymentState::Deployed)
        .expect_err("should be rejected");
    assert!(matches!(err, DeploymentTransitionError::Rejected { .. }));
    assert!(
        err.to_string()
            .contains("cannot transition deployment from")
    );
    assert!(err.to_string().contains("idle"));
    assert!(err.to_string().contains("deployed"));
}

#[test]
fn deployment_state_deploying_rejects_back_to_idle() {
    let err = DeploymentState::Deploying
        .validate_transition(&DeploymentState::Idle)
        .expect_err("should be rejected");
    assert!(matches!(err, DeploymentTransitionError::Rejected { .. }));
}

#[test]
fn deployment_state_rolled_back_rejects_jump_to_deployed() {
    let err = DeploymentState::RolledBack
        .validate_transition(&DeploymentState::Deployed)
        .expect_err("should be rejected");
    assert!(matches!(err, DeploymentTransitionError::Rejected { .. }));
}

// ---------------------------------------------------------------------------
// JSON round-trips
// ---------------------------------------------------------------------------

#[test]
fn release_record_round_trips_through_json() {
    let record = ReleaseRecord {
        release_id: "rel-123".to_string(),
        candidate_version: "2.0.0-rc1".to_string(),
        state: ReleaseState::Candidate,
        validation_ref: Some("session-abc".to_string()),
        approval_ref: None,
        deployment_ref: None,
        rollback_state: None,
        created_at: "2026-03-01T00:00:00Z".to_string(),
        updated_at: "2026-03-02T00:00:00Z".to_string(),
    };

    let json = serde_json::to_string_pretty(&record).expect("should serialize");
    let restored: ReleaseRecord = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(restored, record);
}

#[test]
fn deployment_record_round_trips_through_json() {
    let record = DeploymentRecord {
        deployment_id: "dep-prod-001".to_string(),
        environment: "prod".to_string(),
        desired_code_version: "2.0.0".to_string(),
        deployed_code_version: Some("2.0.0".to_string()),
        desired_migration_version: Some("20260301".to_string()),
        deployed_migration_version: Some("20260301".to_string()),
        state: DeploymentState::Deployed,
        last_result: Some("ok".to_string()),
        rollback_target: None,
        actor: Some("alice".to_string()),
        created_at: "2026-03-01T00:00:00Z".to_string(),
        updated_at: "2026-03-01T01:00:00Z".to_string(),
    };

    let json = serde_json::to_string_pretty(&record).expect("should serialize");
    let restored: DeploymentRecord = serde_json::from_str(&json).expect("should deserialize");
    assert_eq!(restored, record);
}

// ---------------------------------------------------------------------------
// ReleaseState serde — kebab-case variants
// ---------------------------------------------------------------------------

#[test]
fn release_state_serializes_with_kebab_case_variants() {
    assert_eq!(
        serde_json::to_string(&ReleaseState::Planned).unwrap(),
        "\"planned\""
    );
    assert_eq!(
        serde_json::to_string(&ReleaseState::InProgress).unwrap(),
        "\"in-progress\""
    );
    assert_eq!(
        serde_json::to_string(&ReleaseState::Candidate).unwrap(),
        "\"candidate\""
    );
    assert_eq!(
        serde_json::to_string(&ReleaseState::Validated).unwrap(),
        "\"validated\""
    );
    assert_eq!(
        serde_json::to_string(&ReleaseState::Approved).unwrap(),
        "\"approved\""
    );
    assert_eq!(
        serde_json::to_string(&ReleaseState::Deployed).unwrap(),
        "\"deployed\""
    );
    assert_eq!(
        serde_json::to_string(&ReleaseState::RolledBack).unwrap(),
        "\"rolled-back\""
    );
    assert_eq!(
        serde_json::to_string(&ReleaseState::Aborted).unwrap(),
        "\"aborted\""
    );
}

#[test]
fn deployment_state_serializes_with_kebab_case_variants() {
    assert_eq!(
        serde_json::to_string(&DeploymentState::Idle).unwrap(),
        "\"idle\""
    );
    assert_eq!(
        serde_json::to_string(&DeploymentState::Pending).unwrap(),
        "\"pending\""
    );
    assert_eq!(
        serde_json::to_string(&DeploymentState::Deploying).unwrap(),
        "\"deploying\""
    );
    assert_eq!(
        serde_json::to_string(&DeploymentState::Deployed).unwrap(),
        "\"deployed\""
    );
    assert_eq!(
        serde_json::to_string(&DeploymentState::Failed).unwrap(),
        "\"failed\""
    );
    assert_eq!(
        serde_json::to_string(&DeploymentState::RollingBack).unwrap(),
        "\"rolling-back\""
    );
    assert_eq!(
        serde_json::to_string(&DeploymentState::RolledBack).unwrap(),
        "\"rolled-back\""
    );
}

// ---------------------------------------------------------------------------
// Multiple deployments per environment in RepositoryState
// ---------------------------------------------------------------------------

#[test]
fn repository_state_holds_multiple_deployment_records_for_different_environments() {
    let mut state = minimal_repo_state();
    state
        .deployments
        .push(sample_deployment("prod", DeploymentState::Deployed));
    state
        .deployments
        .push(sample_deployment("staging", DeploymentState::Idle));
    state
        .deployments
        .push(sample_deployment("demo", DeploymentState::Pending));

    assert_eq!(state.deployments.len(), 3);
    assert_eq!(state.deployments[0].environment, "prod");
    assert_eq!(state.deployments[1].environment, "staging");
    assert_eq!(state.deployments[2].environment, "demo");

    // Round-trip through JSON to confirm multi-environment coexistence serializes correctly
    let json = state.to_json_pretty().expect("should serialize");
    let restored = RepositoryState::from_json(&json).expect("should deserialize");
    assert_eq!(restored.deployments.len(), 3);
}

#[test]
fn repository_state_holds_release_records() {
    let mut state = minimal_repo_state();
    state
        .releases
        .push(sample_release(ReleaseState::InProgress));
    state.releases.push(sample_release(ReleaseState::Deployed));

    let json = state.to_json_pretty().expect("should serialize");
    let restored = RepositoryState::from_json(&json).expect("should deserialize");
    assert_eq!(restored.releases.len(), 2);
    assert_eq!(restored.releases[0].state, ReleaseState::InProgress);
    assert_eq!(restored.releases[1].state, ReleaseState::Deployed);
}

// ---------------------------------------------------------------------------
// Forward-compatibility: unknown fields are ignored
// ---------------------------------------------------------------------------

#[test]
fn loading_state_with_unknown_release_fields_still_succeeds() {
    let json = r#"{
        "release_id": "rel-x",
        "candidate_version": "1.0.0",
        "state": "planned",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "unknown_future_field": "value"
    }"#;

    // serde(deny_unknown_fields) is NOT set, so this should succeed
    let record: ReleaseRecord =
        serde_json::from_str(json).expect("should deserialize with unknown fields");
    assert_eq!(record.state, ReleaseState::Planned);
}

#[test]
fn loading_state_with_unknown_deployment_fields_still_succeeds() {
    let json = r#"{
        "deployment_id": "dep-001",
        "environment": "prod",
        "desired_code_version": "1.0.0",
        "state": "idle",
        "created_at": "2026-01-01T00:00:00Z",
        "updated_at": "2026-01-01T00:00:00Z",
        "unknown_future_field": true
    }"#;

    let record: DeploymentRecord =
        serde_json::from_str(json).expect("should deserialize with unknown fields");
    assert_eq!(record.state, DeploymentState::Idle);
}

// ---------------------------------------------------------------------------
// GateGroup import — ensure minimal_feature compiles with empty gate_groups
// ---------------------------------------------------------------------------

#[test]
fn gate_group_import_is_available_for_future_tests() {
    // This test just ensures GateGroup is importable from state module and can be used
    let _: Vec<GateGroup> = Vec::new();
}
