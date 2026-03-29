//! Step-mode state machine execution for `calypso --step`.
//!
//! Extracted from `main.rs` so that the loop can be unit-tested with an
//! injected [`TerminalBackend`] and [`SessionExecutor`] without needing a
//! real terminal or Claude installation.

use std::io::Write;
use std::path::Path;
use std::sync::Arc;

use calypso_templates::TemplateSet;
use nightshift_core::driver::{DriverMode, DriverStepResult, SessionExecutor, StateMachineDriver};
use nightshift_core::execution::ExecutionConfig;
use nightshift_core::pinned_prompt::{
    Confirmation, PinnedPrompt, TerminalBackend, format_initial_prompt, format_transition_prompt,
};
use nightshift_core::state::RepositoryState;

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
/// Returns when the user quits, the machine reaches a terminal state, or a
/// driver error occurs.
pub fn run_step_loop<W, B>(
    prompt: &mut PinnedPrompt<W, B>,
    driver: &StateMachineDriver,
    state_path: &Path,
) where
    W: Write,
    B: TerminalBackend,
{
    loop {
        let current = match RepositoryState::load_from_path(state_path) {
            Ok(state) => state.current_feature.workflow_state.as_str().to_string(),
            Err(e) => {
                let _ = prompt.cleanup();
                eprintln!("error loading state: {e}");
                std::process::exit(1);
            }
        };

        let prompt_text = format_initial_prompt(&current);
        if let Err(e) = prompt.show_prompt(&prompt_text) {
            let _ = prompt.cleanup();
            eprintln!("prompt error: {e}");
            std::process::exit(1);
        }

        match prompt.read_confirmation() {
            Ok(Confirmation::Yes) => {}
            Ok(Confirmation::No | Confirmation::Quit) => break,
            Err(e) => {
                let _ = prompt.cleanup();
                eprintln!("input error: {e}");
                std::process::exit(1);
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
                break;
            }
            DriverStepResult::Unchanged => {
                let _ = prompt.log("step complete (state unchanged)");
            }
            DriverStepResult::ClarificationRequired(q) => {
                let _ = prompt.log(&format!("clarification required: {q}"));
            }
            DriverStepResult::Failed { reason } => {
                let _ = prompt.log(&format!("step failed: {reason}"));
            }
            DriverStepResult::Error(e) => {
                let _ = prompt.cleanup();
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

/// Convenience entry point for `main.rs`: resolves the template and delegates
/// to [`run_step_loop`].
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
    use super::*;
    use nightshift_core::execution::{ExecutionConfig, ExecutionError, ExecutionOutcome};
    use nightshift_core::state::{
        FeatureState, FeatureType, PullRequestRef, RepositoryState, SchedulingMeta, WorkflowState,
    };
    use std::collections::VecDeque;
    use std::path::PathBuf;
    use std::sync::Arc;

    use crossterm::event::{Event, KeyCode, KeyEvent};

    // ── MockBackend ────────────────────────────────────────────────────────

    struct MockBackend {
        events: VecDeque<Event>,
        width: u16,
        height: u16,
    }

    impl MockBackend {
        fn new(events: Vec<Event>) -> Self {
            MockBackend {
                events: events.into(),
                width: 80,
                height: 24,
            }
        }
    }

    impl TerminalBackend for MockBackend {
        fn read_event(&mut self) -> std::io::Result<Event> {
            self.events.pop_front().ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::UnexpectedEof, "no more events")
            })
        }

        fn size(&self) -> std::io::Result<(u16, u16)> {
            Ok((self.width, self.height))
        }

        fn enable_raw_mode(&self) -> std::io::Result<()> {
            Ok(())
        }

        fn disable_raw_mode(&self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn key(code: KeyCode) -> Event {
        Event::Key(KeyEvent::from(code))
    }

    fn y_key() -> Event {
        key(KeyCode::Char('y'))
    }

    fn q_key() -> Event {
        key(KeyCode::Char('q'))
    }

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

    // ── Tests ──────────────────────────────────────────────────────────────

    /// Pressing Y writes "running: <state>" to the prompt output BEFORE the
    /// step result log line ("→ advanced to:").
    #[test]
    fn y_writes_running_acknowledgment_before_step_result() {
        let dir = temp_dir("running-ack");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::advancing(WorkflowState::PrdReview);
        let driver = make_driver(state_path.clone(), executor);

        // Y → advance, then q to exit
        let events = vec![y_key(), q_key()];
        let mut prompt = PinnedPrompt::with_backend(Vec::new(), MockBackend::new(events))
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

        // Y → advance, then q to exit
        let events = vec![y_key(), q_key()];
        let mut prompt = PinnedPrompt::with_backend(Vec::new(), MockBackend::new(events))
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

        // Y → advance, then q
        let events = vec![y_key(), q_key()];
        let mut prompt = PinnedPrompt::with_backend(Vec::new(), MockBackend::new(events))
            .expect("mock prompt init");

        // Clear init output so we only see output from the loop
        prompt.writer_mut().clear();
        run_step_loop(&mut prompt, &driver, &state_path);

        let output = String::from_utf8_lossy(prompt.writer_ref());
        // DECSTBM set-scroll-region escape: \x1b[1;<N>r
        assert!(
            output.contains("\x1b[1;"),
            "expected DECSTBM scroll-region escape after step; got: {output:?}"
        );
    }

    /// Pressing q immediately exits without executing a step.
    #[test]
    fn quit_exits_without_step() {
        let dir = temp_dir("quit-no-step");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        // Executor that panics if called — it should never be invoked
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

        let events = vec![q_key()];
        let mut prompt = PinnedPrompt::with_backend(Vec::new(), MockBackend::new(events))
            .expect("mock prompt init");

        run_step_loop(&mut prompt, &driver, &state_path);
        // If we get here without panicking, the test passes.
    }

    /// In Unchanged result, "step complete (state unchanged)" appears in output.
    #[test]
    fn unchanged_result_logs_unchanged_message() {
        let dir = temp_dir("unchanged");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::unchanged();
        let driver = make_driver(state_path.clone(), executor);

        // Y → unchanged, then q
        let events = vec![y_key(), q_key()];
        let mut prompt = PinnedPrompt::with_backend(Vec::new(), MockBackend::new(events))
            .expect("mock prompt init");

        run_step_loop(&mut prompt, &driver, &state_path);

        let output = String::from_utf8_lossy(prompt.writer_ref());
        assert!(
            output.contains("step complete (state unchanged)"),
            "expected unchanged message; got: {output}"
        );
    }

    /// Failed result logs the failure reason.
    #[test]
    fn failed_result_logs_reason() {
        let dir = temp_dir("failed");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));

        let executor = PhonyExecutor::failed("phony failure");
        let driver = make_driver(state_path.clone(), executor);

        // Y → failed, then q
        let events = vec![y_key(), q_key()];
        let mut prompt = PinnedPrompt::with_backend(Vec::new(), MockBackend::new(events))
            .expect("mock prompt init");

        run_step_loop(&mut prompt, &driver, &state_path);

        let output = String::from_utf8_lossy(prompt.writer_ref());
        assert!(
            output.contains("step failed: phony failure"),
            "expected failure message; got: {output}"
        );
    }
}
