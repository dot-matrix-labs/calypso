use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::state::{
    AgentSession, AgentSessionStatus, FeatureState, Gate, GateGroup, GateStatus, PullRequestRef,
    RepositoryState, StateError, WorkflowState,
};

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
                        status: GateStatus::Passing,
                    }],
                },
                GateGroup {
                    id: "validation".to_string(),
                    label: "Validation".to_string(),
                    gates: vec![Gate {
                        id: "rust-quality-green".to_string(),
                        label: "Rust quality green".to_string(),
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
