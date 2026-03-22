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

use std::path::PathBuf;

use crate::headless_persist::{ExitReasonTag, HeadlessRunState, now_rfc3339};
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
    /// If set, the driver writes a [`HeadlessRunState`] snapshot to this path
    /// on each state entry and on exit.  After a clean `Terminal` exit the
    /// file is deleted.
    persist_path: Option<PathBuf>,
    /// If set, the driver resumes from this state instead of
    /// `sm.initial_state`.  The caller is responsible for loading the
    /// snapshot and verifying that the recorded state still exists in the
    /// current state machine graph.
    resume_state: Option<HeadlessRunState>,
}

impl<'sm> HeadlessSmDriver<'sm> {
    /// Create a new driver for the given state machine with default settings.
    pub fn new(sm: &'sm HeadlessStateMachine) -> Self {
        Self {
            sm,
            max_steps: DEFAULT_MAX_STEPS,
            persist_path: None,
            resume_state: None,
        }
    }

    /// Override the maximum number of steps (default: [`DEFAULT_MAX_STEPS`]).
    pub fn with_max_steps(mut self, max_steps: usize) -> Self {
        self.max_steps = max_steps;
        self
    }

    /// Enable iteration-state persistence.
    ///
    /// On each state entry the driver writes a [`HeadlessRunState`] snapshot
    /// to `path`.  The snapshot records the current state name, iteration
    /// count, and a timestamp.  On exit the driver overwrites the snapshot
    /// with the final exit reason.  After a clean `Terminal` exit the file is
    /// deleted so subsequent runs start fresh.
    pub fn with_persist_path(mut self, path: PathBuf) -> Self {
        self.persist_path = Some(path);
        self
    }

    /// Resume from a previously persisted run state.
    ///
    /// When `resume` is provided the driver starts from `resume.current_state`
    /// and `resume.iteration` instead of the state machine's `initial_state`.
    /// A startup log entry records the resumption so that terminal output
    /// clearly explains what happened.
    ///
    /// The caller must verify that `resume.current_state` exists in the
    /// current state machine graph before calling this method.
    pub fn with_resume(mut self, resume: HeadlessRunState) -> Self {
        self.resume_state = Some(resume);
        self
    }

    /// Execute the state machine to completion.
    ///
    /// The driver starts from `sm.initial_state` (or from the state recorded
    /// in [`Self::with_resume`]) and steps through the graph until it reaches
    /// a terminal state, receives a shutdown signal, or encounters a fatal
    /// error.
    ///
    /// All state transitions are logged via `logger`.  If `shutdown` is
    /// `None` the driver runs without signal awareness (useful in tests).
    ///
    /// When a `persist_path` is configured the driver writes a
    /// [`HeadlessRunState`] snapshot before each state action and on exit.
    /// After a clean `Terminal` exit the snapshot is deleted.
    pub fn run(
        &self,
        executor: &dyn StepExecutor,
        logger: &Logger,
        shutdown: Option<&ShutdownSignal>,
    ) -> ExitReason {
        // Determine starting state and iteration counter.
        let (mut current_state, mut iteration) = match &self.resume_state {
            Some(prev) => {
                logger
                    .entry(
                        LogLevel::Warn,
                        &format!(
                            "resuming from state '{}' (previous run: {}, iteration {})",
                            prev.current_state, prev.exit_reason, prev.iteration,
                        ),
                    )
                    .component(Component::StateMachine)
                    .event(LogEvent::Startup)
                    .field("resumed_from_state", &prev.current_state)
                    .field("previous_exit_reason", prev.exit_reason.as_str())
                    .field("previous_iteration", prev.iteration.to_string())
                    .field("state_count", self.sm.states.len().to_string())
                    .emit();
                (prev.current_state.clone(), prev.iteration)
            }
            None => {
                let initial = self.sm.initial_state.clone();
                logger
                    .entry(LogLevel::Info, "headless sm driver starting")
                    .component(Component::StateMachine)
                    .event(LogEvent::Startup)
                    .field("initial_state", &initial)
                    .field("state_count", self.sm.states.len().to_string())
                    .emit();
                (initial, 0)
            }
        };
        let mut steps_taken: usize = 0;

        loop {
            iteration += 1;

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
                    .field("step", steps_taken.to_string())
                    .field("iteration", iteration.to_string())
                    .field("max_steps", self.max_steps.to_string())
                    .field("exit_reason", "step_limit_reached")
                    .emit();
                self.persist_exit(
                    &current_state,
                    iteration,
                    ExitReasonTag::StepLimitReached,
                    Some(format!("max_steps={}", self.max_steps)),
                    logger,
                );
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
                    .field("step", steps_taken.to_string())
                    .field("iteration", iteration.to_string())
                    .field("signal", &signal_str)
                    .field("exit_reason", "interrupted")
                    .emit();
                self.persist_exit(
                    &current_state,
                    iteration,
                    ExitReasonTag::Interrupted,
                    Some(signal_str.clone()),
                    logger,
                );
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
                        .event(LogEvent::StateEntered)
                        .field("state", &current_state)
                        .field("step", steps_taken.to_string())
                        .field("iteration", iteration.to_string())
                        .field("exit_reason", "error")
                        .emit();
                    self.persist_exit(
                        &current_state,
                        iteration,
                        ExitReasonTag::Error,
                        Some(message.clone()),
                        logger,
                    );
                    return ExitReason::Error {
                        state: current_state,
                        message,
                    };
                }
            };

            steps_taken += 1;

            // Persist current position before executing the action so that if
            // the process is killed mid-step, the snapshot still records where
            // execution was.
            self.persist_iteration(&current_state, iteration, logger);

            logger
                .entry(LogLevel::Info, &format!("entering state '{current_state}'"))
                .component(Component::StateMachine)
                .event(LogEvent::StateEntered)
                .field("state", &current_state)
                .field("action", action_label(&state_def.action))
                .field("step", steps_taken.to_string())
                .field("iteration", iteration.to_string())
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
                        .field("step", steps_taken.to_string())
                        .field("iteration", iteration.to_string())
                        .field("exit_reason", "terminal")
                        .emit();
                    // Clean exit — delete the snapshot so the next run starts fresh.
                    self.clear_persist(logger);
                    return ExitReason::Terminal {
                        state: current_state,
                    };
                }

                HeadlessAction::Loop { target } => {
                    let next = target.clone();
                    logger
                        .entry(
                            LogLevel::Info,
                            &format!("loop: '{current_state}' → '{next}'"),
                        )
                        .component(Component::StateMachine)
                        .event(LogEvent::TransitionSelected)
                        .field("state", &current_state)
                        .field("from_state", &current_state)
                        .field("to_state", &next)
                        .field("transition", "loop")
                        .field("step", steps_taken.to_string())
                        .field("iteration", iteration.to_string())
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
                                    LogLevel::Info,
                                    &format!("agent '{current_state}' succeeded → '{next}'"),
                                )
                                .component(Component::Agent)
                                .event(LogEvent::StepExecuted)
                                .field("state", &current_state)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("transition", "on_success")
                                .field("step", steps_taken.to_string())
                                .field("iteration", iteration.to_string())
                                .emit();
                            logger
                                .entry(
                                    LogLevel::Info,
                                    &format!("transition: '{current_state}' →on_success→ '{next}'"),
                                )
                                .component(Component::StateMachine)
                                .event(LogEvent::TransitionSelected)
                                .field("state", &current_state)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("transition", "on_success")
                                .field("step", steps_taken.to_string())
                                .field("iteration", iteration.to_string())
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
                                .event(LogEvent::StepExecuted)
                                .field("state", &current_state)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("transition", "on_failure")
                                .field("reason", &reason)
                                .field("step", steps_taken.to_string())
                                .field("iteration", iteration.to_string())
                                .emit();
                            logger
                                .entry(
                                    LogLevel::Warn,
                                    &format!("transition: '{current_state}' →on_failure→ '{next}'"),
                                )
                                .component(Component::StateMachine)
                                .event(LogEvent::TransitionSelected)
                                .field("state", &current_state)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("transition", "on_failure")
                                .field("reason", &reason)
                                .field("step", steps_taken.to_string())
                                .field("iteration", iteration.to_string())
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
                                .event(LogEvent::StepExecuted)
                                .field("state", &current_state)
                                .field("step", steps_taken.to_string())
                                .field("iteration", iteration.to_string())
                                .field("exit_reason", "error")
                                .field("error", &error)
                                .emit();
                            self.persist_exit(
                                &current_state,
                                iteration,
                                ExitReasonTag::Error,
                                Some(error.clone()),
                                logger,
                            );
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
                                    LogLevel::Info,
                                    &format!("builtin '{builtin}' passed → '{next}'"),
                                )
                                .component(Component::StateMachine)
                                .event(LogEvent::StepExecuted)
                                .field("state", &current_state)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("builtin", builtin)
                                .field("transition", "on_pass")
                                .field("step", steps_taken.to_string())
                                .field("iteration", iteration.to_string())
                                .emit();
                            logger
                                .entry(
                                    LogLevel::Info,
                                    &format!("transition: '{current_state}' →on_pass→ '{next}'"),
                                )
                                .component(Component::StateMachine)
                                .event(LogEvent::TransitionSelected)
                                .field("state", &current_state)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("builtin", builtin)
                                .field("transition", "on_pass")
                                .field("step", steps_taken.to_string())
                                .field("iteration", iteration.to_string())
                                .emit();
                            current_state = next;
                        }
                        BuiltinOutcome::Fail { reason } => {
                            let next = on_fail.clone();
                            logger
                                .entry(
                                    LogLevel::Warn,
                                    &format!(
                                        "builtin '{builtin}' did not pass → '{next}': {reason}"
                                    ),
                                )
                                .component(Component::StateMachine)
                                .event(LogEvent::StepExecuted)
                                .field("state", &current_state)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("builtin", builtin)
                                .field("transition", "on_fail")
                                .field("reason", &reason)
                                .field("step", steps_taken.to_string())
                                .field("iteration", iteration.to_string())
                                .emit();
                            logger
                                .entry(
                                    LogLevel::Warn,
                                    &format!("transition: '{current_state}' →on_fail→ '{next}'"),
                                )
                                .component(Component::StateMachine)
                                .event(LogEvent::TransitionSelected)
                                .field("state", &current_state)
                                .field("from_state", &current_state)
                                .field("to_state", &next)
                                .field("builtin", builtin)
                                .field("transition", "on_fail")
                                .field("reason", &reason)
                                .field("step", steps_taken.to_string())
                                .field("iteration", iteration.to_string())
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
                                .event(LogEvent::StepExecuted)
                                .field("state", &current_state)
                                .field("builtin", builtin)
                                .field("step", steps_taken.to_string())
                                .field("iteration", iteration.to_string())
                                .field("exit_reason", "error")
                                .field("error", &error)
                                .emit();
                            self.persist_exit(
                                &current_state,
                                iteration,
                                ExitReasonTag::Error,
                                Some(error.clone()),
                                logger,
                            );
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

    /// Write a mid-run snapshot recording the current state and iteration.
    ///
    /// Does nothing if no `persist_path` is configured.  Logs a warning if
    /// the write fails but does not abort the run.
    fn persist_iteration(&self, state: &str, iteration: usize, logger: &Logger) {
        let Some(path) = &self.persist_path else {
            return;
        };
        let snapshot = HeadlessRunState {
            current_state: state.to_string(),
            iteration,
            exit_reason: ExitReasonTag::Interrupted,
            exit_detail: None,
            timestamp: now_rfc3339(),
        };
        if let Err(e) = snapshot.save(path) {
            logger
                .entry(LogLevel::Warn, "failed to write iteration snapshot")
                .component(Component::StateMachine)
                .field("path", path.display().to_string())
                .field("error", e.to_string())
                .emit();
        }
    }

    /// Write a final snapshot on exit with the actual exit reason and detail.
    ///
    /// Does nothing if no `persist_path` is configured.
    fn persist_exit(
        &self,
        state: &str,
        iteration: usize,
        reason: ExitReasonTag,
        detail: Option<String>,
        logger: &Logger,
    ) {
        let Some(path) = &self.persist_path else {
            return;
        };
        let snapshot = HeadlessRunState {
            current_state: state.to_string(),
            iteration,
            exit_reason: reason,
            exit_detail: detail,
            timestamp: now_rfc3339(),
        };
        if let Err(e) = snapshot.save(path) {
            logger
                .entry(LogLevel::Warn, "failed to write exit snapshot")
                .component(Component::StateMachine)
                .field("path", path.display().to_string())
                .field("error", e.to_string())
                .emit();
        }
    }

    /// Delete the snapshot file after a clean terminal exit.
    ///
    /// Does nothing if no `persist_path` is configured.
    fn clear_persist(&self, logger: &Logger) {
        let Some(path) = &self.persist_path else {
            return;
        };
        if let Err(e) = HeadlessRunState::clear(path) {
            logger
                .entry(LogLevel::Warn, "failed to clear run snapshot")
                .component(Component::StateMachine)
                .field("path", path.display().to_string())
                .field("error", e.to_string())
                .emit();
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
        // Each agent outcome emits step_executed + transition_selected, so each
        // transition name appears exactly twice (once per event type).
        assert_eq!(
            output.matches("\"transition\":\"on_failure\"").count(),
            2,
            "expected two on_failure entries (step_executed + transition_selected) in output: {output}"
        );
        assert_eq!(
            output.matches("\"transition\":\"on_success\"").count(),
            2,
            "expected two on_success entries (step_executed + transition_selected) in output: {output}"
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

    // ── Structured log event names ────────────────────────────────────────────

    /// State entry emits the `state_entered` event with state, action, step, and
    /// iteration fields.
    #[test]
    fn state_entry_emits_state_entered_event() {
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

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("\"event\":\"state_entered\""),
            "expected state_entered event in output: {output}"
        );
        assert!(
            output.contains("\"state\":\"idle\""),
            "expected state field in output: {output}"
        );
        assert!(
            output.contains("\"action\":\"terminal\""),
            "expected action field in output: {output}"
        );
        assert!(
            output.contains("\"step\":\"1\""),
            "expected step field in output: {output}"
        );
        assert!(
            output.contains("\"iteration\":\"1\""),
            "expected iteration field in output: {output}"
        );
    }

    /// Agent success emits `step_executed` and `transition_selected` events with
    /// full context fields.
    #[test]
    fn agent_success_emits_step_executed_and_transition_selected() {
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
        let executor = ScriptedExecutor::new(vec![AgentOutcome::Success], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("\"event\":\"step_executed\""),
            "expected step_executed event in output: {output}"
        );
        assert!(
            output.contains("\"event\":\"transition_selected\""),
            "expected transition_selected event in output: {output}"
        );
        assert!(
            output.contains("\"transition\":\"on_success\""),
            "expected on_success transition in output: {output}"
        );
        assert!(
            output.contains("\"from_state\":\"work\""),
            "expected from_state in output: {output}"
        );
        assert!(
            output.contains("\"to_state\":\"done\""),
            "expected to_state in output: {output}"
        );
        assert!(
            output.contains("\"iteration\":\"1\""),
            "expected iteration counter in output: {output}"
        );
    }

    /// Agent failure emits `step_executed` with the failure reason and
    /// `transition_selected` with the on_failure target.
    #[test]
    fn agent_failure_emits_step_executed_with_reason_and_transition_selected() {
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
                reason: "provider offline".to_string(),
            }],
            vec![],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("\"event\":\"step_executed\""),
            "expected step_executed event in output: {output}"
        );
        assert!(
            output.contains("\"event\":\"transition_selected\""),
            "expected transition_selected event in output: {output}"
        );
        assert!(
            output.contains("\"transition\":\"on_failure\""),
            "expected on_failure transition in output: {output}"
        );
        assert!(
            output.contains("\"reason\":\"provider offline\""),
            "expected reason in output: {output}"
        );
    }

    /// Builtin pass emits `step_executed` and `transition_selected` events.
    #[test]
    fn builtin_pass_emits_step_executed_and_transition_selected() {
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

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("\"event\":\"step_executed\""),
            "expected step_executed event in output: {output}"
        );
        assert!(
            output.contains("\"event\":\"transition_selected\""),
            "expected transition_selected event in output: {output}"
        );
        assert!(
            output.contains("\"transition\":\"on_pass\""),
            "expected on_pass transition in output: {output}"
        );
    }

    /// Builtin fail emits `step_executed` with reason and `transition_selected`.
    #[test]
    fn builtin_fail_emits_step_executed_with_reason_and_transition_selected() {
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

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("\"event\":\"step_executed\""),
            "expected step_executed event in output: {output}"
        );
        assert!(
            output.contains("\"event\":\"transition_selected\""),
            "expected transition_selected event in output: {output}"
        );
        assert!(
            output.contains("\"transition\":\"on_fail\""),
            "expected on_fail transition in output: {output}"
        );
        assert!(
            output.contains("\"reason\":\"not rebased\""),
            "expected reason in output: {output}"
        );
    }

    /// Loop transition emits `transition_selected` event.
    #[test]
    fn loop_transition_emits_transition_selected_event() {
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
        let executor = ScriptedExecutor::new(
            vec![
                AgentOutcome::Failure {
                    reason: "first try".to_string(),
                },
                AgentOutcome::Success,
            ],
            vec![],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("\"event\":\"transition_selected\""),
            "expected transition_selected event in output: {output}"
        );
        // Verify the loop transition is logged
        assert!(
            output.contains("\"transition\":\"loop\""),
            "expected loop transition in output: {output}"
        );
    }

    /// Iteration counter increases across loop cycles — verifies two distinct
    /// iteration values appear in the log.
    #[test]
    fn iteration_counter_increments_across_loop_cycles() {
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
        // First scan fails (goes to retry), then succeeds (goes to done).
        // Steps: [scan(fail), retry(loop), scan(success), done(terminal)]
        // Iterations: 1,2,3,4
        let executor = ScriptedExecutor::new(
            vec![
                AgentOutcome::Failure {
                    reason: "not yet".to_string(),
                },
                AgentOutcome::Success,
            ],
            vec![],
        );
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        let output = writer.contents();
        // The first iteration of scan is iteration 1; the second is iteration 3
        // (after the retry loop step at iteration 2). Both must appear.
        assert!(
            output.contains("\"iteration\":\"1\""),
            "expected iteration 1 in output: {output}"
        );
        assert!(
            output.contains("\"iteration\":\"3\""),
            "expected iteration 3 (second scan entry) in output: {output}"
        );
    }

    /// Step counter and iteration counter appear in the step_limit_reached shutdown
    /// log so a human can see where the loop stalled.
    #[test]
    fn step_limit_log_includes_step_and_iteration() {
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
            output.contains("\"step\":\"3\""),
            "expected step count in step_limit log: {output}"
        );
        assert!(
            output.contains("\"iteration\""),
            "expected iteration in step_limit log: {output}"
        );
    }

    /// Shutdown signal log includes the shutdown cause (signal name), step, and
    /// iteration fields.
    #[test]
    fn shutdown_log_includes_signal_step_and_iteration() {
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

        let _ = HeadlessSmDriver::new(&sm).run(&executor, &logger, Some(&shutdown));

        let output = writer.contents();
        assert!(
            output.contains("\"signal\":\"SIGTERM\""),
            "expected signal name in shutdown log: {output}"
        );
        assert!(
            output.contains("\"step\""),
            "expected step field in shutdown log: {output}"
        );
        assert!(
            output.contains("\"iteration\""),
            "expected iteration field in shutdown log: {output}"
        );
    }

    // ── Persistence: snapshot written on interrupt ─────────────────────────────

    fn tmp_persist_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir()
            .join(format!("calypso-driver-persist-test-{name}"))
            .join("headless-state.json")
    }

    /// When a shutdown signal is received the driver writes a snapshot
    /// recording the interrupted state, iteration, and signal name.
    #[test]
    fn persist_path_snapshot_written_on_interrupt() {
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

        let path = tmp_persist_path("interrupt");
        let _ = crate::headless_persist::HeadlessRunState::clear(&path);

        let (tx, rx) = std::sync::mpsc::channel();
        tx.send(crate::signal::SignalKind::Interrupt).unwrap();
        let shutdown = crate::signal::ShutdownSignal::from_receiver(rx);

        let _ = HeadlessSmDriver::new(&sm)
            .with_persist_path(path.clone())
            .run(&executor, &logger, Some(&shutdown));

        let snapshot = crate::headless_persist::HeadlessRunState::load(&path)
            .expect("load should succeed")
            .expect("snapshot should exist after interrupt");

        assert_eq!(snapshot.current_state, "work");
        assert_eq!(
            snapshot.exit_reason,
            crate::headless_persist::ExitReasonTag::Interrupted
        );
        assert_eq!(snapshot.exit_detail.as_deref(), Some("SIGINT"));

        let _ = crate::headless_persist::HeadlessRunState::clear(&path);
    }

    /// After a clean Terminal exit the snapshot file is deleted.
    #[test]
    fn persist_path_snapshot_cleared_on_terminal_exit() {
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

        let path = tmp_persist_path("terminal-clear");

        let _ = HeadlessSmDriver::new(&sm)
            .with_persist_path(path.clone())
            .run(&executor, &logger, None);

        assert!(
            !path.exists(),
            "snapshot should be deleted after terminal exit"
        );
    }

    /// When a step limit is reached the snapshot is written with
    /// `StepLimitReached` as the exit reason.
    #[test]
    fn persist_path_snapshot_written_on_step_limit() {
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

        let path = tmp_persist_path("step-limit");
        let _ = crate::headless_persist::HeadlessRunState::clear(&path);

        let _ = HeadlessSmDriver::new(&sm)
            .with_persist_path(path.clone())
            .with_max_steps(3)
            .run(&executor, &logger, None);

        let snapshot = crate::headless_persist::HeadlessRunState::load(&path)
            .expect("load should succeed")
            .expect("snapshot should exist after step limit");

        assert_eq!(
            snapshot.exit_reason,
            crate::headless_persist::ExitReasonTag::StepLimitReached
        );
        assert_eq!(snapshot.current_state, "work");

        let _ = crate::headless_persist::HeadlessRunState::clear(&path);
    }

    /// When an agent fatal error occurs the snapshot is written with `Error`.
    #[test]
    fn persist_path_snapshot_written_on_agent_error() {
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

        let path = tmp_persist_path("agent-error");
        let _ = crate::headless_persist::HeadlessRunState::clear(&path);

        let _ = HeadlessSmDriver::new(&sm)
            .with_persist_path(path.clone())
            .run(&executor, &logger, None);

        let snapshot = crate::headless_persist::HeadlessRunState::load(&path)
            .expect("load should succeed")
            .expect("snapshot should exist after agent error");

        assert_eq!(
            snapshot.exit_reason,
            crate::headless_persist::ExitReasonTag::Error
        );
        assert_eq!(snapshot.current_state, "work");
        assert!(
            snapshot
                .exit_detail
                .as_deref()
                .unwrap_or("")
                .contains("provider crashed"),
            "expected error detail in snapshot"
        );

        let _ = crate::headless_persist::HeadlessRunState::clear(&path);
    }

    // ── Persistence: resume from previous snapshot ─────────────────────────────

    /// Resuming from a persisted state starts from the recorded state, not
    /// `initial_state`, and logs a resumption notice.
    #[test]
    fn resume_starts_from_recorded_state_not_initial() {
        // Two-state machine: start → done. If resumed from "done", should
        // exit immediately without ever visiting "start".
        let yaml = r#"
initial_state: start
states:
  - name: start
    action: agent
    on_success: done
    on_failure: done
  - name: done
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        // No agent outcomes queued — if "start" were entered this would panic.
        let executor = ScriptedExecutor::new(vec![], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let resume = crate::headless_persist::HeadlessRunState {
            current_state: "done".to_string(),
            iteration: 5,
            exit_reason: crate::headless_persist::ExitReasonTag::Interrupted,
            exit_detail: Some("SIGINT".to_string()),
            timestamp: "2026-03-22T06:00:00Z".to_string(),
        };

        let result = HeadlessSmDriver::new(&sm)
            .with_resume(resume)
            .run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "done".to_string()
            },
            "expected terminal at 'done' when resuming"
        );

        let output = writer.contents();
        assert!(
            output.contains("resuming from state"),
            "expected resumption notice in output: {output}"
        );
        assert!(
            output.contains("\"resumed_from_state\":\"done\""),
            "expected resumed_from_state field in output: {output}"
        );
    }

    /// The iteration counter continues from the resumed snapshot's iteration
    /// value, not from zero.
    #[test]
    fn resume_continues_iteration_counter_from_snapshot() {
        let yaml = r#"
initial_state: scan
states:
  - name: scan
    action: agent
    on_success: done
    on_failure: done
  - name: done
    action: terminal
"#;
        let sm = load_and_validate(yaml, "<test>").unwrap();
        let executor = ScriptedExecutor::new(vec![AgentOutcome::Success], vec![]);
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let resume = crate::headless_persist::HeadlessRunState {
            current_state: "scan".to_string(),
            iteration: 10,
            exit_reason: crate::headless_persist::ExitReasonTag::Interrupted,
            exit_detail: Some("SIGINT".to_string()),
            timestamp: "2026-03-22T06:00:00Z".to_string(),
        };

        let _ = HeadlessSmDriver::new(&sm)
            .with_resume(resume)
            .run(&executor, &logger, None);

        let output = writer.contents();
        // Iteration 10 + 1 = 11 on the first loop after resume
        assert!(
            output.contains("\"iteration\":\"11\""),
            "expected resumed iteration counter (11) in output: {output}"
        );
    }

    /// Resume logs the previous exit reason from the snapshot so it is
    /// visible in terminal output.
    #[test]
    fn resume_logs_previous_exit_reason() {
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

        let resume = crate::headless_persist::HeadlessRunState {
            current_state: "done".to_string(),
            iteration: 3,
            exit_reason: crate::headless_persist::ExitReasonTag::Interrupted,
            exit_detail: Some("SIGTERM".to_string()),
            timestamp: "2026-03-22T06:00:00Z".to_string(),
        };

        let _ = HeadlessSmDriver::new(&sm)
            .with_resume(resume)
            .run(&executor, &logger, None);

        let output = writer.contents();
        assert!(
            output.contains("previous_exit_reason"),
            "expected previous_exit_reason in startup log: {output}"
        );
        assert!(
            output.contains("interrupted"),
            "expected previous exit reason value in startup log: {output}"
        );
    }

    /// Running without a persist_path does not error or panic even with
    /// all exit paths exercised.
    #[test]
    fn no_persist_path_runs_cleanly() {
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

        // Should complete without panicking or writing any files.
        let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

        assert_eq!(
            result,
            ExitReason::Terminal {
                state: "done".to_string()
            }
        );
    }
}
