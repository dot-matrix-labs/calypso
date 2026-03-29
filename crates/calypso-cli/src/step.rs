//! Step-mode entry point for `calypso --step`.
//!
//! Constructs the driver and prompt, then delegates the interactive loop to
//! [`nightshift_core::step_loop::run_step_loop`].

use std::path::Path;
use std::sync::Arc;

use calypso_templates::TemplateSet;
use nightshift_core::driver::{DriverMode, SessionExecutor, StateMachineDriver};
use nightshift_core::execution::ExecutionConfig;
use nightshift_core::pinned_prompt::PinnedPrompt;
use nightshift_core::step_loop::run_step_loop;

/// Entry point called from `main()`: resolves the template, constructs the
/// driver and prompt, runs the interactive loop, and maps the outcome to an
/// exit code.
pub fn run_state_machine_step(
    state_path: &Path,
    template: TemplateSet,
    executor: Option<Arc<dyn SessionExecutor>>,
) {
    let driver = StateMachineDriver {
        mode: DriverMode::Step,
        state_path: state_path.to_path_buf(),
        template,
        config: ExecutionConfig::default(),
        executor,
    };
    let mut prompt = match PinnedPrompt::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("failed to initialize pinned prompt: {e}");
            std::process::exit(1);
        }
    };
    run_step_loop(&mut prompt, &driver, state_path);
}

#[cfg(test)]
mod tests {
    use nightshift_core::driver::{DriverMode, SessionExecutor, StateMachineDriver};
    use nightshift_core::execution::{ExecutionConfig, ExecutionError, ExecutionOutcome};
    use nightshift_core::pinned_prompt::PinnedPrompt;
    use nightshift_core::state::{
        FeatureState, FeatureType, PullRequestRef, RepositoryState, SchedulingMeta, WorkflowState,
    };
    use nightshift_core::step_loop::{StepLoopOutcome, run_step_loop};
    use nightshift_core::testing::{MockBackend, make_prompt};

    use crossterm::event::{Event, KeyCode, KeyEvent};
    use std::path::PathBuf;
    use std::sync::Arc;

    // ── PhonyExecutor ──────────────────────────────────────────────────────

    struct PhonyExecutor {
        outcome: ExecutionOutcome,
    }

    impl PhonyExecutor {
        fn advancing(next: WorkflowState) -> Arc<Self> {
            Arc::new(Self {
                outcome: ExecutionOutcome::Ok {
                    summary: "phony".to_string(),
                    artifact_refs: vec![],
                    advanced_to: Some(next),
                },
            })
        }

        fn unchanged() -> Arc<Self> {
            Arc::new(Self {
                outcome: ExecutionOutcome::Ok {
                    summary: "phony".to_string(),
                    artifact_refs: vec![],
                    advanced_to: None,
                },
            })
        }

        fn failed(reason: &str) -> Arc<Self> {
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

    // ── Fixtures ──────────────────────────────────────────────────────────

    fn phony_template_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/phony-template")
    }

    fn minimal_state(workflow_state: WorkflowState) -> RepositoryState {
        RepositoryState {
            version: 1,
            repo_id: "step-test".to_string(),
            schema_version: 2,
            current_feature: FeatureState {
                feature_id: "step-feature".to_string(),
                branch: "feat/step".to_string(),
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

    fn write_state(dir: &std::path::Path, state: &RepositoryState) -> PathBuf {
        let state_dir = dir.join(".calypso");
        std::fs::create_dir_all(&state_dir).expect("create .calypso dir");
        let path = state_dir.join("repository-state.json");
        state.save_to_path(&path).expect("save state");
        path
    }

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("calypso-step-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn make_driver(state_path: PathBuf, executor: Arc<dyn SessionExecutor>) -> StateMachineDriver {
        let template = calypso_templates::TemplateSet::load_from_directory(&phony_template_dir())
            .expect("phony template loads");
        StateMachineDriver {
            mode: DriverMode::Step,
            state_path,
            template,
            config: ExecutionConfig::default(),
            executor: Some(executor),
        }
    }

    fn y_key() -> Event {
        Event::Key(KeyEvent::from(KeyCode::Char('y')))
    }

    fn q_key() -> Event {
        Event::Key(KeyEvent::from(KeyCode::Char('q')))
    }

    // ── Tests ──────────────────────────────────────────────────────────────

    /// Pressing Y writes "running: <state>" to the prompt output BEFORE the
    /// step result log line ("→ advanced to:").
    #[test]
    fn y_writes_running_acknowledgment_before_step_result() {
        let dir = temp_dir("running-ack");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::advancing(WorkflowState::PrdReview);
        let driver = make_driver(state_path.clone(), executor);

        let mut prompt = PinnedPrompt::with_backend(
            Vec::<u8>::new(),
            MockBackend::new(80, 24, vec![y_key(), q_key()]),
        )
        .expect("mock prompt init");

        run_step_loop(&mut prompt, &driver, &state_path);

        let output = String::from_utf8_lossy(prompt.writer_ref());
        let running_pos = output
            .find("running:")
            .expect("expected 'running:' in output");
        let advanced_pos = output
            .find("→ advanced to:")
            .expect("expected '→ advanced to:' in output");
        assert!(
            running_pos < advanced_pos,
            "'running:' must appear before '→ advanced to:' in output"
        );
    }

    /// After driver.step() returns Advanced, the output contains both the
    /// running acknowledgment and the transition log line.
    #[test]
    fn step_advanced_output_contains_running_and_transition() {
        let dir = temp_dir("both-lines");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::advancing(WorkflowState::PrdReview);
        let driver = make_driver(state_path.clone(), executor);

        let mut prompt = PinnedPrompt::with_backend(
            Vec::<u8>::new(),
            MockBackend::new(80, 24, vec![y_key(), q_key()]),
        )
        .expect("mock prompt init");

        run_step_loop(&mut prompt, &driver, &state_path);

        let output = String::from_utf8_lossy(prompt.writer_ref());
        assert!(
            output.contains("running: new"),
            "expected 'running: new' in output; got: {output}"
        );
        assert!(
            output.contains("→ advanced to:"),
            "expected '→ advanced to:' in output"
        );
    }

    /// After a step result the scroll region repair ANSI escape is emitted.
    #[test]
    fn repair_scroll_region_is_emitted_after_step() {
        let dir = temp_dir("scroll-repair");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::advancing(WorkflowState::PrdReview);
        let driver = make_driver(state_path.clone(), executor);

        let mut prompt = PinnedPrompt::with_backend(
            Vec::<u8>::new(),
            MockBackend::new(80, 24, vec![y_key(), q_key()]),
        )
        .expect("mock prompt init");

        prompt.writer_mut().clear();
        run_step_loop(&mut prompt, &driver, &state_path);

        let output = String::from_utf8_lossy(prompt.writer_ref());
        assert!(
            output.contains("\x1b[1;"),
            "expected DECSTBM scroll-region escape after step; got: {output:?}"
        );
    }

    /// Pressing q immediately returns UserCancelled without executing a step.
    #[test]
    fn quit_returns_user_cancelled_without_step() {
        let dir = temp_dir("quit-no-step");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        struct PanicExecutor;
        impl SessionExecutor for PanicExecutor {
            fn run(
                &self,
                _: &std::path::Path,
                _: &str,
                _: &ExecutionConfig,
            ) -> Result<ExecutionOutcome, ExecutionError> {
                panic!("executor must not be called when user quits");
            }
        }

        let template = calypso_templates::TemplateSet::load_from_directory(&phony_template_dir())
            .expect("phony template loads");
        let driver = StateMachineDriver {
            mode: DriverMode::Step,
            state_path: state_path.clone(),
            template,
            config: ExecutionConfig::default(),
            executor: Some(Arc::new(PanicExecutor)),
        };

        let mut prompt = make_prompt(80, 24, vec![q_key()]);
        let outcome = run_step_loop(&mut prompt, &driver, &state_path);
        assert_eq!(outcome, StepLoopOutcome::UserCancelled);
    }

    /// In Unchanged result, "step complete (state unchanged)" appears in output.
    #[test]
    fn unchanged_result_logs_unchanged_message() {
        let dir = temp_dir("unchanged");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::unchanged();
        let driver = make_driver(state_path.clone(), executor);

        let mut prompt = make_prompt(80, 24, vec![y_key(), q_key()]);
        run_step_loop(&mut prompt, &driver, &state_path);

        let output = String::from_utf8_lossy(prompt.writer_ref());
        assert!(
            output.contains("step complete (state unchanged)"),
            "expected unchanged message; got: {output}"
        );
    }

    /// Failed result logs the failure reason and returns Failed outcome.
    #[test]
    fn failed_result_logs_reason_and_returns_outcome() {
        let dir = temp_dir("failed");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::failed("phony failure");
        let driver = make_driver(state_path.clone(), executor);

        let mut prompt = make_prompt(80, 24, vec![y_key()]);
        let outcome = run_step_loop(&mut prompt, &driver, &state_path);

        let output = String::from_utf8_lossy(prompt.writer_ref());
        assert!(
            output.contains("step failed: phony failure"),
            "expected failure message; got: {output}"
        );
        assert_eq!(outcome, StepLoopOutcome::Failed("phony failure".to_string()));
    }
}
