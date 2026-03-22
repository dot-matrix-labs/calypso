//! Tests for the phony-template fixture and the StateMachineDriver
//! using an injectable PhonyExecutor.
//!
//! The phony template defines a minimal 3-step pipeline using canonical
//! WorkflowState slugs: new -> prd-review -> architecture-plan.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use calypso_cli::driver::{DriverMode, DriverStepResult, SessionExecutor, StateMachineDriver};
use calypso_cli::execution::{ExecutionConfig, ExecutionError, ExecutionOutcome};
use calypso_cli::state::{
    FeatureState, FeatureType, PullRequestRef, RepositoryState, SchedulingMeta, WorkflowState,
};
use calypso_cli::template::TemplateSet;

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Path to the phony template fixture directory (relative to the crate root).
fn phony_template_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phony-template")
}

/// Create a unique temp directory for a test.
fn temp_dir(label: &str) -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let dir = std::env::temp_dir().join(format!("calypso-phony-{label}-{nanos}"));
    std::fs::create_dir_all(&dir).expect("create temp dir");
    dir
}

/// Build a minimal `RepositoryState` at the given `WorkflowState`.
fn minimal_state(workflow_state: WorkflowState) -> RepositoryState {
    RepositoryState {
        version: 1,
        repo_id: "phony-repo".to_string(),
        schema_version: 2,
        current_feature: FeatureState {
            feature_id: "phony-feature".to_string(),
            branch: "feat/phony".to_string(),
            worktree_path: "/tmp".to_string(),
            pull_request: PullRequestRef {
                number: 1,
                url: "https://github.com/example/repo/pull/1".to_string(),
            },
            github_snapshot: None,
            github_error: None,
            workflow_state,
            gate_groups: vec![],
            active_sessions: vec![],
            feature_type: FeatureType::Feat,
            roles: vec![],
            scheduling: SchedulingMeta::default(),
            artifact_refs: vec![],
            transcript_refs: vec![],
            clarification_history: vec![],
        },
        identity: Default::default(),
        providers: vec![],
        releases: vec![],
        deployments: vec![],
        github_auth_ref: None,
        secure_key_refs: vec![],
    }
}

/// Write a `RepositoryState` to a temp directory and return the state file path.
fn write_state(dir: &Path, state: &RepositoryState) -> PathBuf {
    let state_dir = dir.join(".calypso");
    std::fs::create_dir_all(&state_dir).expect("create .calypso dir");
    let path = state_dir.join("repository-state.json");
    state.save_to_path(&path).expect("save state");
    path
}

// ── PhonyExecutor ─────────────────────────────────────────────────────────────

/// A `SessionExecutor` that returns a pre-canned `ExecutionOutcome` without
/// spawning any external process.  Used to drive the state machine in tests
/// without a real Claude installation.
struct PhonyExecutor {
    outcome: ExecutionOutcome,
}

impl PhonyExecutor {
    fn ok_advancing(next: WorkflowState) -> Arc<Self> {
        Arc::new(Self {
            outcome: ExecutionOutcome::Ok {
                summary: "phony ok".to_string(),
                artifact_refs: vec![],
                advanced_to: Some(next),
            },
        })
    }

    fn ok_no_advance() -> Arc<Self> {
        Arc::new(Self {
            outcome: ExecutionOutcome::Ok {
                summary: "phony ok no advance".to_string(),
                artifact_refs: vec![],
                advanced_to: None,
            },
        })
    }

    fn nok(reason: &str) -> Arc<Self> {
        Arc::new(Self {
            outcome: ExecutionOutcome::Nok {
                summary: "phony nok".to_string(),
                reason: reason.to_string(),
            },
        })
    }
}

impl SessionExecutor for PhonyExecutor {
    fn run(
        &self,
        _state_path: &std::path::Path,
        _role: &str,
        _config: &ExecutionConfig,
    ) -> Result<ExecutionOutcome, ExecutionError> {
        Ok(self.outcome.clone())
    }
}

// ── Template unit tests ───────────────────────────────────────────────────────

#[test]
fn phony_template_loads_from_directory() {
    let dir = phony_template_dir();
    let result = TemplateSet::load_from_directory(&dir);
    assert!(
        result.is_ok(),
        "phony template should load without error: {:?}",
        result.err()
    );
}

#[test]
fn phony_template_validates_with_zero_coherence_errors() {
    let dir = phony_template_dir();
    let template = TemplateSet::load_from_directory(&dir).expect("phony template loads");
    let errors = template.validate_coherence();
    assert!(
        errors.is_empty(),
        "phony template should have zero coherence errors; got: {errors:?}"
    );
}

#[test]
fn phony_template_initial_state_is_new() {
    let dir = phony_template_dir();
    let template = TemplateSet::load_from_directory(&dir).expect("phony template loads");
    assert_eq!(
        template.state_machine.initial_state, "new",
        "phony template initial state should be 'new'"
    );
}

#[test]
fn phony_template_defines_task_for_each_gate() {
    let dir = phony_template_dir();
    let template = TemplateSet::load_from_directory(&dir).expect("phony template loads");

    for group in &template.state_machine.gate_groups {
        for gate in &group.gates {
            let task = template.task_by_name(&gate.task);
            assert!(
                task.is_some(),
                "gate '{}' references task '{}' which should exist in agents catalog",
                gate.id,
                gate.task
            );
        }
    }
}

#[test]
fn phony_template_has_three_gate_groups() {
    let dir = phony_template_dir();
    let template = TemplateSet::load_from_directory(&dir).expect("phony template loads");
    assert_eq!(
        template.state_machine.gate_groups.len(),
        3,
        "phony template should define exactly 3 gate groups (alpha, beta, gamma)"
    );
}

// ── Driver integration tests ──────────────────────────────────────────────────

/// Build a `StateMachineDriver` with the phony template and a given executor.
fn phony_driver(state_path: PathBuf, executor: Arc<dyn SessionExecutor>) -> StateMachineDriver {
    let template =
        TemplateSet::load_from_directory(&phony_template_dir()).expect("phony template loads");
    StateMachineDriver {
        mode: DriverMode::Auto,
        state_path,
        template,
        config: ExecutionConfig::default(),
        executor: Some(executor),
    }
}

#[test]
fn driver_advances_from_new_to_prd_review_with_phony_ok() {
    let dir = temp_dir("advance-new");
    let state = minimal_state(WorkflowState::New);
    let state_path = write_state(&dir, &state);

    let executor = PhonyExecutor::ok_advancing(WorkflowState::PrdReview);
    let driver = phony_driver(state_path, executor);

    let result = driver.step();
    assert_eq!(
        result,
        DriverStepResult::Advanced(WorkflowState::PrdReview),
        "agent step on 'new' state should advance to 'prd-review'"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_advances_from_prd_review_to_architecture_plan_with_phony_ok() {
    let dir = temp_dir("advance-prd");
    let state = minimal_state(WorkflowState::PrdReview);
    let state_path = write_state(&dir, &state);

    let executor = PhonyExecutor::ok_advancing(WorkflowState::ArchitecturePlan);
    let driver = phony_driver(state_path, executor);

    let result = driver.step();
    assert_eq!(
        result,
        DriverStepResult::Advanced(WorkflowState::ArchitecturePlan),
        "agent step on 'prd-review' should advance to 'architecture-plan'"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_step_sequence_new_to_prd_review_to_architecture_plan() {
    // Manually advance through the phony pipeline one step at a time,
    // updating the state file between each call.
    let dir = temp_dir("sequence");
    let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

    // Step 1: new -> prd-review
    let executor = PhonyExecutor::ok_advancing(WorkflowState::PrdReview);
    let driver = phony_driver(state_path.clone(), executor);
    let r1 = driver.step();
    assert_eq!(r1, DriverStepResult::Advanced(WorkflowState::PrdReview));

    // Advance state file manually (PhonyExecutor doesn't write to disk).
    write_state(&dir, &minimal_state(WorkflowState::PrdReview));

    // Step 2: prd-review -> architecture-plan
    let executor = PhonyExecutor::ok_advancing(WorkflowState::ArchitecturePlan);
    let driver = phony_driver(state_path.clone(), executor);
    let r2 = driver.step();
    assert_eq!(
        r2,
        DriverStepResult::Advanced(WorkflowState::ArchitecturePlan)
    );

    // Advance state file manually.
    write_state(&dir, &minimal_state(WorkflowState::ArchitecturePlan));

    // Step 3: architecture-plan has no further forward transition in the phony
    // template; PhonyExecutor returns Ok with no advancement -> Unchanged.
    let executor = PhonyExecutor::ok_no_advance();
    let driver = phony_driver(state_path.clone(), executor);
    let r3 = driver.step();
    assert_eq!(
        r3,
        DriverStepResult::Unchanged,
        "architecture-plan with no advance should be Unchanged"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_stays_unchanged_when_executor_returns_ok_with_no_advance() {
    let dir = temp_dir("unchanged");
    let state = minimal_state(WorkflowState::New);
    let state_path = write_state(&dir, &state);

    let executor = PhonyExecutor::ok_no_advance();
    let driver = phony_driver(state_path, executor);

    let result = driver.step();
    assert_eq!(
        result,
        DriverStepResult::Unchanged,
        "ok with no advanced_to should produce Unchanged"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_returns_failed_when_executor_returns_nok() {
    let dir = temp_dir("failed");
    let state = minimal_state(WorkflowState::New);
    let state_path = write_state(&dir, &state);

    let executor = PhonyExecutor::nok("simulated gate failure");
    let driver = phony_driver(state_path, executor);

    let result = driver.step();
    assert!(
        matches!(result, DriverStepResult::Failed { .. }),
        "NOK executor outcome should produce Failed driver result, got: {result:?}"
    );

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn run_auto_stops_on_failed_result() {
    let dir = temp_dir("auto-stop");
    let state = minimal_state(WorkflowState::New);
    let state_path = write_state(&dir, &state);

    let executor = PhonyExecutor::nok("gate blocked");
    let driver = phony_driver(state_path, executor);

    let results = driver.run_auto();
    assert_eq!(
        results.len(),
        1,
        "run_auto should stop after first Failed step"
    );
    assert!(matches!(results[0], DriverStepResult::Failed { .. }));

    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn driver_errors_when_state_file_missing_with_phony_executor() {
    let executor = PhonyExecutor::ok_no_advance();
    let template =
        TemplateSet::load_from_directory(&phony_template_dir()).expect("phony template loads");
    let driver = StateMachineDriver {
        mode: DriverMode::Auto,
        state_path: PathBuf::from("/nonexistent/phony-state.json"),
        template,
        config: ExecutionConfig::default(),
        executor: Some(executor),
    };

    let result = driver.step();
    assert!(
        matches!(result, DriverStepResult::Error(_)),
        "missing state file should produce Error, got: {result:?}"
    );
}
