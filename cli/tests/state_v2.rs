use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::state::{
    ArtifactRef, ClarificationEntry, FeatureState, FeatureType, Gate, GateGroup, GateStatus,
    PullRequestRef, RepositoryIdentity, RepositoryState, RoleSession, SchedulingMeta,
    WorkflowState,
};

fn temp_path(suffix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("calypso-state-v2-{unique}-{suffix}"))
}

fn minimal_feature_state(id: &str) -> FeatureState {
    FeatureState {
        feature_id: id.to_string(),
        branch: format!("feat/{id}"),
        worktree_path: format!("/worktrees/{id}"),
        pull_request: PullRequestRef {
            number: 1,
            url: "https://github.com/org/repo/pull/1".to_string(),
        },
        github_snapshot: None,
        github_error: None,
        workflow_state: WorkflowState::Implementation,
        gate_groups: vec![GateGroup {
            id: "validation".to_string(),
            label: "Validation".to_string(),
            gates: vec![Gate {
                id: "test-gate".to_string(),
                label: "Test gate".to_string(),
                task: "test-task".to_string(),
                status: GateStatus::Pending,
            }],
        }],
        active_sessions: Vec::new(),
        feature_type: FeatureType::Feat,
        roles: Vec::new(),
        scheduling: SchedulingMeta::default(),
        artifact_refs: Vec::new(),
        transcript_refs: Vec::new(),
        clarification_history: Vec::new(),
    }
}

fn full_repository_state() -> RepositoryState {
    RepositoryState {
        version: 1,
        schema_version: 1,
        repo_id: "test-repo".to_string(),
        identity: RepositoryIdentity {
            name: "my-org/test-repo".to_string(),
            github_remote_url: "https://github.com/my-org/test-repo.git".to_string(),
            default_branch: "main".to_string(),
        },
        providers: vec!["openai".to_string(), "anthropic".to_string()],
        releases: Vec::new(),
        deployments: Vec::new(),
        current_feature: minimal_feature_state("feat-auth"),
    }
}

#[test]
fn repository_state_with_all_new_fields_round_trips_through_json() {
    let state = full_repository_state();
    let json = state.to_json_pretty().expect("state should serialize");
    let restored = RepositoryState::from_json(&json).expect("state should deserialize");
    assert_eq!(restored, state);
}

#[test]
fn feature_state_with_roles_artifacts_and_clarification_history_round_trips() {
    let mut feature = minimal_feature_state("feat-complex");
    feature.feature_type = FeatureType::Fix;
    feature.roles = vec![
        RoleSession {
            role: "engineer".to_string(),
            session_id: Some("sess-001".to_string()),
            last_outcome: Some("ok".to_string()),
        },
        RoleSession {
            role: "reviewer".to_string(),
            session_id: None,
            last_outcome: None,
        },
    ];
    feature.scheduling = SchedulingMeta {
        created_at: "2024-01-01T00:00:00Z".to_string(),
        last_advanced_at: Some("2024-01-02T00:00:00Z".to_string()),
        last_agent_run_at: Some("2024-01-03T00:00:00Z".to_string()),
    };
    feature.artifact_refs = vec![ArtifactRef {
        kind: "screenshot".to_string(),
        path: "/tmp/screenshot.png".to_string(),
        session_id: Some("sess-001".to_string()),
    }];
    feature.transcript_refs = vec!["/tmp/transcripts/sess-001.jsonl".to_string()];
    feature.clarification_history = vec![ClarificationEntry {
        session_id: "sess-001".to_string(),
        question: "Should we use async?".to_string(),
        answer: Some("Yes, please use tokio.".to_string()),
        timestamp: "2024-01-01T01:00:00Z".to_string(),
    }];

    let json = serde_json::to_string_pretty(&feature).expect("feature should serialize");
    let restored: FeatureState = serde_json::from_str(&json).expect("feature should deserialize");
    assert_eq!(restored, feature);
}

#[test]
fn save_to_path_uses_atomic_rename_no_tmp_file_remains() {
    let path = temp_path("atomic.json");
    let tmp_path = path.with_extension("tmp");
    let state = full_repository_state();

    state.save_to_path(&path).expect("state should save");

    assert!(path.exists(), "final state file should exist");
    assert!(!tmp_path.exists(), ".tmp file should not remain after save");

    fs::remove_file(&path).expect("temp state file should be removed");
}

#[test]
fn loading_state_file_with_unknown_field_succeeds() {
    let path = temp_path("unknown-field.json");
    // Write JSON with an extra unknown field that didn't exist before
    let json = r#"{
        "version": 1,
        "repo_id": "test",
        "schema_version": 1,
        "future_unknown_field": "some value",
        "current_feature": {
            "feature_id": "feat-x",
            "branch": "feat/x",
            "worktree_path": "/worktrees/x",
            "pull_request": { "number": 1, "url": "https://github.com/o/r/pull/1" },
            "workflow_state": "new",
            "gate_groups": [],
            "active_sessions": []
        }
    }"#;
    fs::write(&path, json).expect("fixture should write");

    let result = RepositoryState::load_from_path(&path);
    assert!(
        result.is_ok(),
        "loading state with unknown fields should succeed, got: {:?}",
        result.err()
    );

    fs::remove_file(&path).expect("temp file should be removed");
}

#[test]
fn schema_version_defaults_to_one_when_absent_from_old_state_file() {
    let json = r#"{
        "version": 1,
        "repo_id": "legacy-repo",
        "current_feature": {
            "feature_id": "feat-legacy",
            "branch": "feat/legacy",
            "worktree_path": "/worktrees/legacy",
            "pull_request": { "number": 10, "url": "https://github.com/o/r/pull/10" },
            "workflow_state": "implementation",
            "gate_groups": [],
            "active_sessions": []
        }
    }"#;

    let state = RepositoryState::from_json(json).expect("old state should deserialize");
    assert_eq!(
        state.schema_version, 1,
        "schema_version should default to 1 when absent"
    );
}

#[test]
fn feature_type_enum_serializes_with_kebab_case() {
    assert_eq!(
        serde_json::to_string(&FeatureType::Feat).expect("should serialize"),
        "\"feat\""
    );
    assert_eq!(
        serde_json::to_string(&FeatureType::Fix).expect("should serialize"),
        "\"fix\""
    );
    assert_eq!(
        serde_json::to_string(&FeatureType::Chore).expect("should serialize"),
        "\"chore\""
    );
}

#[test]
fn feature_state_feature_type_defaults_to_feat_when_absent() {
    let json = r#"{
        "feature_id": "feat-default",
        "branch": "feat/default",
        "worktree_path": "/worktrees/default",
        "pull_request": { "number": 1, "url": "https://github.com/o/r/pull/1" },
        "workflow_state": "new",
        "gate_groups": [],
        "active_sessions": []
    }"#;

    let feature: FeatureState = serde_json::from_str(json).expect("feature should deserialize");
    assert_eq!(feature.feature_type, FeatureType::Feat);
}
