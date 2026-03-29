//! Interactive step-mode loop for `nightshift-core`.
//!
//! Extracted from `calypso-cli` so that the full loop can be exercised by
//! tests using an injected [`TerminalBackend`] and [`crate::driver::SessionExecutor`].

use std::io::Write;
use std::path::Path;

use crate::driver::{DriverStepResult, StateMachineDriver};
use crate::pinned_prompt::{
    Confirmation, PinnedPrompt, TerminalBackend, format_initial_prompt, format_transition_prompt,
};
use crate::state::RepositoryState;

/// The outcome of a completed [`run_step_loop`] invocation.
#[derive(Debug, PartialEq, Eq)]
pub enum StepLoopOutcome {
    /// The user cancelled (pressed n/q/ctrl-c) before or between steps.
    UserCancelled,
    /// The state machine reached a terminal state.
    Terminal,
    /// A step returned a clarification request.
    ClarificationRequired(String),
    /// A step returned a failure.
    Failed(String),
    /// An irrecoverable error occurred (I/O, state-load, etc.).
    Error(String),
}

/// Run the interactive step-mode loop.
///
/// Each iteration:
/// 1. Loads current state from `state_path`.
/// 2. Shows the step prompt and waits for Y/n/q.
/// 3. On Y: logs "running: <state>" immediately, then calls `driver.step()`.
/// 4. Repairs the scroll region after the step (subprocess output may have
///    disrupted the ANSI scroll region).
/// 5. Logs the step result and shows the next prompt.
///
/// Returns a [`StepLoopOutcome`] describing how the loop ended.
pub fn run_step_loop<W, B>(
    prompt: &mut PinnedPrompt<W, B>,
    driver: &StateMachineDriver,
    state_path: &Path,
) -> StepLoopOutcome
where
    W: Write,
    B: TerminalBackend,
{
    loop {
        let current = match RepositoryState::load_from_path(state_path) {
            Ok(state) => state.current_feature.workflow_state.as_str().to_string(),
            Err(e) => {
                let _ = prompt.cleanup();
                return StepLoopOutcome::Error(format!("error loading state: {e}"));
            }
        };

        let prompt_text = format_initial_prompt(&current);
        if let Err(e) = prompt.show_prompt(&prompt_text) {
            let _ = prompt.cleanup();
            return StepLoopOutcome::Error(format!("prompt error: {e}"));
        }

        match prompt.read_confirmation() {
            Ok(Confirmation::Yes) => {}
            Ok(Confirmation::No | Confirmation::Quit) => {
                return StepLoopOutcome::UserCancelled;
            }
            Err(e) => {
                let _ = prompt.cleanup();
                return StepLoopOutcome::Error(format!("input error: {e}"));
            }
        }

        // Acknowledge the keypress immediately before the (potentially long)
        // step executes.
        let _ = prompt.log(&format!("running: {current}"));

        let result = driver.step();

        // Repair any scroll-region disruption caused by subprocess stdout/stderr.
        let _ = prompt.repair_scroll_region();

        match result {
            DriverStepResult::Advanced(next_state) => {
                let next = next_state.as_str();
                let _ = prompt.log(&format!("→ advanced to: {next}"));
                let transition_prompt = format_transition_prompt(&current, next);
                let _ = prompt.show_prompt(&transition_prompt);
            }
            DriverStepResult::Terminal => {
                let _ = prompt.log("done");
                return StepLoopOutcome::Terminal;
            }
            DriverStepResult::Unchanged => {
                let _ = prompt.log("step complete (state unchanged)");
            }
            DriverStepResult::ClarificationRequired(q) => {
                let _ = prompt.log(&format!("clarification required: {q}"));
                return StepLoopOutcome::ClarificationRequired(q);
            }
            DriverStepResult::Failed { reason } => {
                let _ = prompt.log(&format!("step failed: {reason}"));
                return StepLoopOutcome::Failed(reason);
            }
            DriverStepResult::Error(e) => {
                let _ = prompt.cleanup();
                return StepLoopOutcome::Error(e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::driver::{DriverMode, SessionExecutor, StateMachineDriver};
    use crate::execution::{ExecutionConfig, ExecutionError, ExecutionOutcome};
    use crate::state::{
        FeatureState, FeatureType, PullRequestRef, RepositoryState, SchedulingMeta, WorkflowState,
    };
    use crate::testing::{MockBackend, make_prompt};
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

    /// Pressing q immediately returns UserCancelled without invoking the executor.
    #[test]
    fn quit_returns_user_cancelled() {
        let dir = temp_dir("quit-cancelled");
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

    /// Pressing Y with an advancing executor then q returns UserCancelled.
    #[test]
    fn y_advance_then_quit_returns_user_cancelled() {
        let dir = temp_dir("advance-quit");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::advancing(WorkflowState::PrdReview);
        let driver = make_driver(state_path.clone(), executor);

        let mut prompt = make_prompt(80, 24, vec![y_key(), q_key()]);
        let outcome = run_step_loop(&mut prompt, &driver, &state_path);
        assert_eq!(outcome, StepLoopOutcome::UserCancelled);
    }

    /// Failed step returns StepLoopOutcome::Failed with the reason.
    #[test]
    fn failed_step_returns_failed_outcome() {
        let dir = temp_dir("failed-outcome");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::failed("phony failure");
        let driver = make_driver(state_path.clone(), executor);

        let mut prompt = make_prompt(80, 24, vec![y_key()]);
        let outcome = run_step_loop(&mut prompt, &driver, &state_path);
        assert_eq!(outcome, StepLoopOutcome::Failed("phony failure".to_string()));
    }

    /// Unchanged result: loop logs "step complete (state unchanged)" and continues.
    #[test]
    fn unchanged_result_logs_message_and_continues() {
        let dir = temp_dir("unchanged-msg");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::unchanged();
        let driver = make_driver(state_path.clone(), executor);

        let mut prompt = PinnedPrompt::with_backend(
            Vec::<u8>::new(),
            MockBackend::new(80, 24, vec![y_key(), q_key()]),
        )
        .expect("prompt init");

        let outcome = run_step_loop(&mut prompt, &driver, &state_path);

        let output = String::from_utf8_lossy(prompt.writer_ref());
        assert!(
            output.contains("step complete (state unchanged)"),
            "expected unchanged message; got: {output}"
        );
        assert_eq!(outcome, StepLoopOutcome::UserCancelled);
    }

    /// Pressing Y writes "running: <state>" BEFORE the "→ advanced to:" line.
    #[test]
    fn y_writes_running_acknowledgment_before_step_result() {
        let dir = temp_dir("running-ack-nc");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::advancing(WorkflowState::PrdReview);
        let driver = make_driver(state_path.clone(), executor);

        let mut prompt = PinnedPrompt::with_backend(
            Vec::<u8>::new(),
            MockBackend::new(80, 24, vec![y_key(), q_key()]),
        )
        .expect("prompt init");

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
            "'running:' must appear before '→ advanced to:'"
        );
    }

    /// After driver.step() returns Advanced, scroll-region ANSI escape is emitted.
    #[test]
    fn repair_scroll_region_is_emitted_after_step() {
        let dir = temp_dir("scroll-repair-nc");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::advancing(WorkflowState::PrdReview);
        let driver = make_driver(state_path.clone(), executor);

        let mut prompt = PinnedPrompt::with_backend(
            Vec::<u8>::new(),
            MockBackend::new(80, 24, vec![y_key(), q_key()]),
        )
        .expect("prompt init");

        prompt.writer_mut().clear();
        run_step_loop(&mut prompt, &driver, &state_path);

        let output = String::from_utf8_lossy(prompt.writer_ref());
        assert!(
            output.contains("\x1b[1;"),
            "expected DECSTBM scroll-region escape after step; got: {output:?}"
        );
    }
}
