//! Headless state-machine driver — executes a [`HeadlessStateMachine`] end-to-end
//! without any TUI or display dependency.
//!
//! # Overview
//!
//! The driver accepts a loaded [`HeadlessStateMachine`], an optional
//! [`ShutdownSignal`], and a pluggable [`StepExecutor`] for agent and builtin
//! steps.  It loops until one of four conditions is met:
//!
//! 1. A `terminal` state is reached — the driver returns [`ExitReason::Terminal`].
//! 2. A shutdown signal is received — the driver returns [`ExitReason::Interrupted`].
//! 3. An agent or builtin step returns a fatal error — the driver returns
//!    [`ExitReason::Error`].
//! 4. The maximum step limit is reached — the driver returns [`ExitReason::StepLimitReached`].
//!
//! All state transitions and exit reasons are logged via the provided [`Logger`].
//!
//! # Step resolution
//!
//! | action     | resolution                                                    |
//! |------------|---------------------------------------------------------------|
//! | `agent`    | delegates to [`StepExecutor::run_agent`]; success → `on_success`, failure → `on_failure` |
//! | `builtin`  | delegates to [`StepExecutor::run_builtin`]; pass → `on_pass`, fail → `on_fail` |
//! | `loop`     | jumps unconditionally to `target`                             |
//! | `terminal` | stops with [`ExitReason::Terminal`]                          |
//!
//! # Default step limit
//!
//! The default maximum number of steps is [`DEFAULT_MAX_STEPS`] (10 000).
//! Override via [`HeadlessSmDriver::with_max_steps`].

use crate::headless_sm::{HeadlessAction, HeadlessStateMachine};
use crate::signal::ShutdownSignal;
use crate::telemetry::{Component, LogEvent, LogLevel, Logger};

// ── Constants ─────────────────────────────────────────────────────────────────

/// Default upper bound on the number of steps the driver will execute.
///
/// A well-formed state machine should terminate naturally via a `terminal`
/// state.  The limit guards against infinite loops caused by authoring errors.
pub const DEFAULT_MAX_STEPS: usize = 10_000;

// ── Step executor trait ───────────────────────────────────────────────────────

/// Outcome of executing a single `agent` step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentOutcome {
    /// The agent completed successfully.
    Success,
    /// The agent completed with a recoverable failure.
    Failure {
        /// Human-readable reason for the failure.
        reason: String,
    },
    /// The agent encountered a fatal error (runtime, provider, etc.).
    Error {
        /// Human-readable description of the error.
        error: String,
    },
}

/// Outcome of evaluating a single `builtin` step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltinOutcome {
    /// The builtin check passed.
    Pass,
    /// The builtin check did not pass (normal, not an error).
    Fail {
        /// Human-readable reason why the check did not pass.
        reason: String,
    },
    /// The builtin evaluation encountered a fatal error.
    Error {
        /// Human-readable description of the error.
        error: String,
    },
}

/// Pluggable execution layer for agent and builtin steps.
///
/// The production implementation will spawn supervised Claude sessions and
/// evaluate real builtin functions.  Tests inject a `PhonyExecutor` that
/// returns pre-canned outcomes without any I/O.
pub trait StepExecutor: Send + Sync {
    /// Run an `agent` action for the given state name.
    fn run_agent(&self, state_name: &str) -> AgentOutcome;

    /// Evaluate a `builtin` function for the given state name and builtin key.
    fn run_builtin(&self, state_name: &str, builtin_key: &str) -> BuiltinOutcome;
}

// ── Run result ────────────────────────────────────────────────────────────────

/// The reason the driver stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExitReason {
    /// A `terminal` state was reached.
    Terminal {
        /// Name of the terminal state.
        state: String,
    },
    /// A shutdown signal (SIGINT / SIGTERM) was received.
    Interrupted {
        /// Signal description (e.g. `"SIGINT"`).
        signal: String,
    },
    /// A fatal error occurred during step execution.
    Error {
        /// State where the error occurred.
        state: String,
        /// Human-readable description of the error.
        message: String,
    },
    /// The step limit was reached before a terminal state.
    StepLimitReached {
        /// Maximum step count that was configured.
        max_steps: usize,
    },
}

// ── Driver ────────────────────────────────────────────────────────────────────

/// Drives a [`HeadlessStateMachine`] to completion.
pub struct HeadlessSmDriver<'sm> {
    sm: &'sm HeadlessStateMachine,
    max_steps: usize,
}

impl<'sm> HeadlessSmDriver<'sm> {
    /// Create a new driver for the given state machine with default settings.
    pub fn new(sm: &'sm HeadlessStateMachine) -> Self {
        Self {
            sm,
            max_steps: DEFAULT_MAX_STEPS,
        }
    }

    /// Override the maximum number of steps (default: [`DEFAULT_MAX_STEPS`]).
    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// Execute the state machine to completion.
    ///
    /// The driver starts from `sm.initial_state` and steps through the graph
    /// until it reaches a terminal state, receives a shutdown signal, or
    /// encounters a fatal error.
    ///
    /// All state transitions are logged via `logger`.  If `shutdown` is
    /// `None` the driver runs without signal awareness (useful in tests).
    pub fn run(
        &self,
        executor: &dyn StepExecutor,
        logger: &Logger,
        shutdown: Option<&ShutdownSignal>,
    ) -> ExitReason {
        let mut current_state = self.sm.initial_state.clone();
        let mut steps_taken: usize = 0;

        logger
            .entry(LogLevel::Info, "headless sm driver starting")
            .component(Component::StateMachine)
            .event(LogEvent::Startup)
            .field("initial_state", &current_state)
            .field("state_count", self.sm.states.len().to_string())
            .emit();

        loop {
            // Guard: step limit
            if steps_taken >= self.max_steps {
                let reason = ExitReason::StepLimitReached {
                    max_steps: self.max_steps,
                };
                logger
                    .entry(LogLevel::Error, "step limit reached")
                    .component(Component::StateMachine)
                    .event(LogEvent::Shutdown)
                    .field("state", &current_state)
                    .field("max_steps", self.max_steps.to_string())
                    .field("exit_reason", "step_limit_reached")
                    .emit();
                return reason;
            }

            // Guard: shutdown signal
            if let Some(sig_handle) = shutdown
                && let Some(signal) = sig_handle.try_recv()
            {
                let signal_str = signal.to_string();
                logger
                    .entry(
                        LogLevel::Warn,
                        &format!("received {signal_str}, shutting down"),
                    )
                    .component(Component::Cli)
                    .event(LogEvent::Shutdown)
                    .field("state", &current_state)
                    .field("signal", &signal_str)
                    .field("exit_reason", "interrupted")
                    .emit();
                return ExitReason::Interrupted { signal: signal_str };
            }

            // Look up current state (must exist — validated on load)
            let state_def = match self.sm.states.get(&current_state) {
                Some(s) => s,
                None => {
                    // Should never happen after validation, but guard defensively.
                    let message = format!("state '{current_state}' not found in state graph");
                    logger
                        .entry(LogLevel::Error, &message)
                        .component(Component::StateMachine)
                        .event(LogEvent::StateTransition)
                        .field("state", &current_state)
                        .field("exit_reason", "error")
                        .emit();
                    return ExitReason::Error {
                        state: current_state,
                        message,
                    };
                }
            };

            steps_taken += 1;

            logger
                .entry(
                    LogLevel::Debug,
                    &format!("entering state '{current_state}'"),
                )
                .component(Component::StateMachine)
                .event(LogEvent::StateTransition)
                .field("state", &current_state)
                .field("action", action_label(&state_def.action))
                .field("step", steps_taken.to_string())
                .emit();

            match &state_def.action {
                HeadlessAction::Terminal => {
                    logger
                        .entry(
                            LogLevel::Info,
                            &format!("reached terminal state '{current_state}'"),
                        )
                        .component(Component::StateMachine)
                        .event(LogEvent::Shutdown)
                        .field("state", &current_state)
                        .field("exit_reason", "terminal")
                        .emit();
                    return ExitReason::Terminal {
                        state: current_state,
                    };
                }

                HeadlessAction::Loop { target } => {
                    let next = target.clone();
                    logger
                        .entry(
                            LogLevel::Debug,
                            &format!("loop: '{current_state}' → '{next}'"),
                        )
                        .component(Component::StateMachine)
                        .event(LogEvent::StateTransition)
                        .field("from_state", &current_state)
                        .field("to_state", &next)
                        .field("transition", "loop")
                        .emit();
                    current_state = next;
                }

                HeadlessAction::Agent {
                    on_success,
                    on_failure,
                } => {
                    let outcome = executor.run_agent(&current_state);
                    match outcome {
                        AgentOutcome::Success => {
                            let next = on_success.clone();
                            logger
                                .entry(
                                    LogLevel::Debug,
                                    &format!("agent '{current_state}' succeeded → '{next}'"),
                                )
                                .component(Component::Agent)
                                .event(LogEvent::AgentCompleted)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("transition", "on_success")
                                .emit();
                            current_state = next;
                        }
                        AgentOutcome::Failure { reason } => {
                            let next = on_failure.clone();
                            logger
                                .entry(
                                    LogLevel::Warn,
                                    &format!("agent '{current_state}' failed → '{next}': {reason}"),
                                )
                                .component(Component::Agent)
                                .event(LogEvent::AgentCompleted)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("transition", "on_failure")
                                .field("reason", &reason)
                                .emit();
                            current_state = next;
                        }
                        AgentOutcome::Error { error } => {
                            logger
                                .entry(
                                    LogLevel::Error,
                                    &format!("agent '{current_state}' fatal error: {error}"),
                                )
                                .component(Component::Agent)
                                .event(LogEvent::AgentCompleted)
                                .field("state", &current_state)
                                .field("exit_reason", "error")
                                .field("error", &error)
                                .emit();
                            return ExitReason::Error {
                                state: current_state,
                                message: error,
                            };
                        }
                    }
                }

                HeadlessAction::Builtin {
                    builtin,
                    on_pass,
                    on_fail,
                } => {
                    let outcome = executor.run_builtin(&current_state, builtin);
                    match outcome {
                        BuiltinOutcome::Pass => {
                            let next = on_pass.clone();
                            logger
                                .entry(
                                    LogLevel::Debug,
                                    &format!("builtin '{builtin}' passed → '{next}'"),
                                )
                                .component(Component::StateMachine)
                                .event(LogEvent::StateTransition)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("builtin", builtin)
                                .field("transition", "on_pass")
                                .emit();
                            current_state = next;
                        }
                        BuiltinOutcome::Fail { reason } => {
                            let next = on_fail.clone();
                            logger
                                .entry(
                                    LogLevel::Debug,
                                    &format!(
                                        "builtin '{builtin}' did not pass → '{next}': {reason}"
                                    ),
                                )
                                .component(Component::StateMachine)
                                .event(LogEvent::StateTransition)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("builtin", builtin)
                                .field("transition", "on_fail")
                                .field("reason", &reason)
                                .emit();
                            current_state = next;
                        }
                        BuiltinOutcome::Error { error } => {
                            logger
                                .entry(
                                    LogLevel::Error,
                                    &format!("builtin '{builtin}' fatal error: {error}"),
                                )
                                .component(Component::StateMachine)
                                .event(LogEvent::StateTransition)
                                .field("state", &current_state)
                                .field("builtin", builtin)
                                .field("exit_reason", "error")
                                .field("error", &error)
                                .emit();
                            return ExitReason::Error {
                                state: current_state,
                                message: error,
                            };
                        }
                    }
                }
            }
        }
    }
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn action_label(action: &HeadlessAction) -> &'static str {
    match action {
        HeadlessAction::Agent { .. } => "agent",
        HeadlessAction::Builtin { .. } => "builtin",
        HeadlessAction::Loop { .. } => "loop",
        HeadlessAction::Terminal => "terminal",
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headless_sm::load_and_validate;
    use crate::telemetry::{LogFormat, Logger};
    use std::sync::{Arc, Mutex};

    // ── Test helpers ─────────────────────────────────────────────────────────

    /// A writer that captures output for test assertions.
    #[derive(Clone)]
    struct CaptureWriter {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    impl CaptureWriter {
        fn new() -> Self {
            Self {
                buffer: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn contents(&self) -> String {
            let buf = self.buffer.lock().unwrap();
            String::from_utf8_lossy(&buf).to_string()
        }
    }

    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.buffer.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn make_logger(writer: CaptureWriter) -> Logger {
        Logger::_with_level_and_writer(LogLevel::Debug, Box::new(writer))
            .with_format(LogFormat::Json)
    }

    /// A `StepExecutor` that returns pre-scripted outcomes.
    ///
    /// Each call to `run_agent` or `run_builtin` pops the next outcome from
    /// the front of the respective queue.  If the queue is empty the executor
    /// panics with a descriptive message (test misconfiguration).
    struct ScriptedExecutor {
        agent_outcomes: Mutex<Vec<AgentOutcome>>,
        builtin_outcomes: Mutex<Vec<BuiltinOutcome>>,
    }

    impl ScriptedExecutor {
        fn new(agent_outcomes: Vec<AgentOutcome>, builtin_outcomes: Vec<BuiltinOutcome>) -> Self {
            Self {
                agent_outcomes: Mutex::new(agent_outcomes),
                builtin_outcomes: Mutex::new(builtin_outcomes),
            }
        }
    }

    impl StepExecutor for ScriptedExecutor {
        fn run_agent(&self, state_name: &str) -> AgentOutcome {
            let mut queue = self.agent_outcomes.lock().unwrap();
            if queue.is_empty() {
                panic!("ScriptedExecutor: no more agent outcomes queued (state: {state_name})");
            }
            queue.remove(0)
        }

        fn run_builtin(&self, state_name: &str, builtin_key: &str) -> BuiltinOutcome {
            let mut queue = self.builtin_outcomes.lock().unwrap();
            if queue.is_empty() {
                panic!(
                    "ScriptedExecutor: no more builtin outcomes queued \
                     (state: {state_name}, builtin: {builtin_key})"
                );
            }
            queue.remove(0)
        }
    }

    // ── Happy path ────────────────────────────────────────────────────────────

    #[test]
    fn minimal_terminal_machine_exits_immediately() {
        let yaml = r#"
initial_state: idle
states:
  - name: idle
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(vec![], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "idle".to_string()
            }
        );

        let output = writer.contents();
        assert!(
            output.contains("terminal"),
            "expected 'terminal' in output: {output}"
        );
        assert!(
            output.contains("idle"),
            "expected state name 'idle' in output: {output}"
        );
    }

    #[test]
    fn two_state_machine_agent_success_path() {
        // work →(success)→ done
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
    on_failure: error
  - name: done
    action: terminal
  - name: error
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(vec![AgentOutcome::Success], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "done".to_string()
            }
        );

        let output = writer.contents();
        assert!(
            output.contains("on_success"),
            "expected on_success transition in output: {output}"
        );
    }

    #[test]
    fn two_state_machine_agent_failure_path() {
        // work →(failure)→ error
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
    on_failure: error
  - name: done
    action: terminal
  - name: error
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(
            vec![AgentOutcome::Failure {
                reason: "agent gave up".to_string(),
            }],
            vec![],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "error".to_string()
            }
        );

        let output = writer.contents();
        assert!(
            output.contains("on_failure"),
            "expected on_failure transition in output: {output}"
        );
        assert!(
            output.contains("agent gave up"),
            "expected failure reason in output: {output}"
        );
    }

    #[test]
    fn builtin_pass_path() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_pass: done
    on_fail: error
  - name: done
    action: terminal
  - name: error
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(vec![], vec![BuiltinOutcome::Pass]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "done".to_string()
            }
        );

        let output = writer.contents();
        assert!(
            output.contains("on_pass"),
            "expected on_pass transition in output: {output}"
        );
    }

    #[test]
    fn builtin_fail_path() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_pass: done
    on_fail: error
  - name: done
    action: terminal
  - name: error
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(
            vec![],
            vec![BuiltinOutcome::Fail {
                reason: "not rebased".to_string(),
            }],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "error".to_string()
            }
        );

        let output = writer.contents();
        assert!(
            output.contains("on_fail"),
            "expected on_fail transition in output: {output}"
        );
        assert!(
            output.contains("not rebased"),
            "expected fail reason in output: {output}"
        );
    }

    /// A looped workflow iterates more than once.
    #[test]
    fn looped_workflow_iterates_multiple_times() {
        // scan →(success)→ done on second attempt; first attempt fails → retry → scan
        let yaml = r#"
initial_state: scan
states:
  - name: scan
    action: agent
    on_success: done
    on_failure: retry
  - name: retry
    action: loop
    target: scan
  - name: done
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        // First call to scan fails, second succeeds.
        let executor = ScriptedExecutor::new(
            vec![
                AgentOutcome::Failure {
                    reason: "first pass failed".to_string(),
                },
                AgentOutcome::Success,
            ],
            vec![],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "done".to_string()
            }
        );

        let output = writer.contents();
        // Should log the loop transition
        assert!(
            output.contains("loop"),
            "expected loop transition in output: {output}"
        );
        // Should log both agent calls
        assert_eq!(
            output.matches("\"transition\":\"on_failure\"").count(),
            1,
            "expected one on_failure in output: {output}"
        );
        assert_eq!(
            output.matches("\"transition\":\"on_success\"").count(),
            1,
            "expected one on_success in output: {output}"
        );
    }

    #[test]
    fn full_recovery_workflow_fixture_runs_to_done_on_success() {
        // Uses the user-recovery-workflow fixture: scan → check(pass) → done
        let yaml = include_str!("../tests/fixtures/user-recovery-workflow.yml");
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(
            vec![AgentOutcome::Success], // scan succeeds
            vec![BuiltinOutcome::Pass],  // check passes
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "done".to_string()
            }
        );
    }

    #[test]
    fn full_recovery_workflow_iterates_on_check_fail() {
        // scan → check(fail) → retry → scan → check(pass) → done
        let yaml = include_str!("../tests/fixtures/user-recovery-workflow.yml");
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(
            vec![AgentOutcome::Success, AgentOutcome::Success],
            vec![
                BuiltinOutcome::Fail {
                    reason: "not ready".to_string(),
                },
                BuiltinOutcome::Pass,
            ],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "done".to_string()
            }
        );

        let output = writer.contents();
        assert!(
            output.contains("loop"),
            "expected loop iteration in output: {output}"
        );
    }

    #[test]
    fn full_recovery_workflow_reaches_error_on_agent_failure() {
        // scan → failure → error (terminal)
        let yaml = include_str!("../tests/fixtures/user-recovery-workflow.yml");
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(
            vec![AgentOutcome::Failure {
                reason: "provider offline".to_string(),
            }],
            vec![],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "error".to_string()
            }
        );
    }

    // ── Error exit reasons ────────────────────────────────────────────────────

    #[test]
    fn agent_fatal_error_exits_with_error_reason() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
    on_failure: done
  - name: done
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(
            vec![AgentOutcome::Error {
                error: "provider crashed".to_string(),
            }],
            vec![],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert!(
            matches!(result, ExitReason::Error { .. }),
            "expected Error exit, got: {result:?}"
        );
        if let ExitReason::Error { state, message } = result {
            assert_eq!(state, "work");
            assert!(
                message.contains("provider crashed"),
                "expected error message to include 'provider crashed', got: {message}"
            );
        }

        let output = writer.contents();
        assert!(
            output.contains("exit_reason"),
            "expected exit_reason in output: {output}"
        );
        assert!(
            output.contains("error"),
            "expected error level in output: {output}"
        );
    }

    #[test]
    fn builtin_fatal_error_exits_with_error_reason() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_pass: done
    on_fail: done
  - name: done
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(
            vec![],
            vec![BuiltinOutcome::Error {
                error: "git binary not found".to_string(),
            }],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert!(
            matches!(result, ExitReason::Error { .. }),
            "expected Error exit, got: {result:?}"
        );
        if let ExitReason::Error { message, .. } = result {
            assert!(
                message.contains("git binary not found"),
                "expected error message in result, got: {message}"
            );
        }
    }

    // ── Step limit ────────────────────────────────────────────────────────────

    #[test]
    fn step_limit_stops_infinite_loop() {
        // A loop that never terminates: work →(failure)→ retry →(loop)→ work
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
    on_failure: retry
  - name: retry
    action: loop
    target: work
  - name: done
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();

        // Always fail so we keep looping
        struct AlwaysFailExecutor;
        impl StepExecutor for AlwaysFailExecutor {
            fn run_agent(&self, _state: &str) -> AgentOutcome {
                AgentOutcome::Failure {
                    reason: "always fails".to_string(),
                }
            }
            fn run_builtin(&self, _state: &str, _key: &str) -> BuiltinOutcome {
                BuiltinOutcome::Pass
            }
        }

        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let result =
            HeadlessSmDriver::new(&sm)
                .with_max_steps(5)
                .run(&AlwaysFailExecutor, &logger, None);

        assert_eq!(
            result,
            ExitReason::StepLimitReached { max_steps: 5 },
            "expected StepLimitReached, got: {result:?}"
        );

        let output = writer.contents();
        assert!(
            output.contains("step limit reached"),
            "expected step limit message in output: {output}"
        );
    }

    // ── Shutdown signal ───────────────────────────────────────────────────────

    #[test]
    fn shutdown_signal_interrupts_before_first_step() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
    on_failure: done
  - name: done
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(vec![], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(crate::signal::SignalKind::Terminate).unwrap();
        let shutdown = crate::signal::ShutdownSignal::from_receiver(rx);

        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, Some(&shutdown));

        assert!(
            matches!(result, ExitReason::Interrupted { .. }),
            "expected Interrupted exit, got: {result:?}"
        );
        if let ExitReason::Interrupted { signal } = result {
            assert_eq!(signal, "SIGTERM");
        }

        let output = writer.contents();
        assert!(
            output.contains("shutting down"),
            "expected shutdown message in output: {output}"
        );
    }

    #[test]
    fn sigint_interrupts_with_correct_signal_name() {
        let yaml = r#"
initial_state: idle
states:
  - name: idle
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(vec![], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(crate::signal::SignalKind::Interrupt).unwrap();
        let shutdown = crate::signal::ShutdownSignal::from_receiver(rx);

        // idle is terminal, but signal check happens before step execution
        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, Some(&shutdown));

        assert!(
            matches!(result, ExitReason::Interrupted { .. }),
            "expected Interrupted exit, got: {result:?}"
        );
        if let ExitReason::Interrupted { signal } = result {
            assert_eq!(signal, "SIGINT");
        }
    }

    // ── Logging content ───────────────────────────────────────────────────────

    #[test]
    fn startup_log_includes_initial_state() {
        let yaml = r#"
initial_state: begin
states:
  - name: begin
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(vec![], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("\"initial_state\":\"begin\""),
            "expected initial_state in startup log: {output}"
        );
    }

    #[test]
    fn exit_reason_logged_on_terminal() {
        let yaml = r#"
initial_state: done
states:
  - name: done
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(vec![], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("\"exit_reason\":\"terminal\""),
            "expected exit_reason:terminal in output: {output}"
        );
    }

    #[test]
    fn exit_reason_logged_on_step_limit() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: loop
    target: work
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(vec![], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let _ = HeadlessSmDriver::new(&sm)
            .with_max_steps(3)
            .run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("\"exit_reason\":\"step_limit_reached\""),
            "expected exit_reason:step_limit_reached in output: {output}"
        );
    }

    #[test]
    fn exit_reason_logged_on_error() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
    on_failure: done
  - name: done
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(
            vec![AgentOutcome::Error {
                error: "fatal".to_string(),
            }],
            vec![],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("\"exit_reason\":\"error\""),
            "expected exit_reason:error in output: {output}"
        );
    }

    #[test]
    fn exit_reason_logged_on_interrupt() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
    on_failure: done
  - name: done
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(vec![], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(crate::signal::SignalKind::Interrupt).unwrap();
        let shutdown = crate::signal::ShutdownSignal::from_receiver(rx);

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, Some(&shutdown));

        let output = writer.contents();
        assert!(
            output.contains("\"exit_reason\":\"interrupted\""),
            "expected exit_reason:interrupted in output: {output}"
        );
    }

    // ── Derived trait smoke tests ─────────────────────────────────────────────

    #[test]
    fn agent_outcome_debug_and_eq() {
        let a = AgentOutcome::Success;
        let b = AgentOutcome::Success;
        assert_eq!(a, b);
        assert_eq!(format!("{a:?}"), "Success");
    }

    #[test]
    fn builtin_outcome_debug_and_eq() {
        let a = BuiltinOutcome::Pass;
        let b = BuiltinOutcome::Pass;
        assert_eq!(a, b);
        assert_eq!(format!("{a:?}"), "Pass");
    }

    #[test]
    fn exit_reason_debug_and_eq() {
        let a = ExitReason::Terminal {
            state: "done".to_string(),
        };
        let b = a.clone();
        assert_eq!(a, b);
        let debug = format!("{a:?}");
        assert!(debug.contains("Terminal"));
        assert!(debug.contains("done"));
    }
}
