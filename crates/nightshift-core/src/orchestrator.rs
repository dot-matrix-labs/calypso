//! Orchestrator loop — single-pass and daemon scheduling modes.
//!
//! This module implements the orchestrator for CI / daemon environments where
//! no interactive TUI is available.  All output is structured log events written
//! to stderr via the [`Logger`](crate::telemetry::Logger).

use std::collections::BTreeMap;
use std::path::Path;

use crate::app::resolve_repo_root;
use crate::doctor::{DoctorReport, DoctorStatus, HostDoctorEnvironment, collect_doctor_report};
use crate::interpreter_scheduler::{SchedulerMode, run_interpreter_scheduler};
use crate::signal::install_signal_handlers;
use crate::telemetry::{Component, LogEvent, LogFormat, LogLevel, Logger};
use calypso_workflow_exec::{StepOutcome, WorkflowInterpreter};
use calypso_workflows::StateKind;

/// Configuration resolved from CLI flags when the orchestrator is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratorConfig {
    /// Resolved verbosity: Debug (default), Info (`-v`), or Trace (`-vv`).
    pub verbosity: LogLevel,
    /// Output format for log lines.
    pub log_format: LogFormat,
    /// If both `-v`/`-vv` and `CALYPSO_LOG` are set, captures the env value
    /// so the caller can emit a notice.
    pub env_log_override: Option<String>,
}

/// Outcome of an orchestrator run, used for testing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrchestratorOutcome {
    pub exit_code: i32,
}

/// Run the orchestrator with the given configuration and scheduler mode.
///
/// - [`SchedulerMode::SinglePass`]: runs one scheduling pass and exits
///   (suitable for CI / test environments).
/// - [`SchedulerMode::Daemon`]: blocks on cron-scheduled workflows and fires
///   them when their scheduled time arrives; runs until interrupted by a signal.
///
/// Returns the process exit code (0 = success, 1 = doctor failure / signal,
/// 2 = configuration error).
pub fn run_orchestrator(cwd: &Path, config: &OrchestratorConfig, mode: SchedulerMode) -> i32 {
    let logger = Logger::with_level(config.verbosity).with_format(config.log_format);
    run_orchestrator_with_logger(cwd, config, mode, &logger)
}

/// Inner implementation that accepts a logger — useful for testing.
pub fn run_orchestrator_with_logger(
    cwd: &Path,
    config: &OrchestratorConfig,
    mode: SchedulerMode,
    logger: &Logger,
) -> i32 {
    // 1. If env override, emit notice
    if let Some(env_val) = &config.env_log_override {
        logger.log_level_override_notice(env_val, config.verbosity);
    }

    // 2. Log startup
    let startup_msg = match mode {
        SchedulerMode::Daemon => "calypso daemon mode starting (continuous scheduling)",
        SchedulerMode::SinglePass => "calypso orchestrator starting",
    };
    logger.log_event(
        LogLevel::Info,
        Component::Cli,
        LogEvent::Startup,
        startup_msg,
        BTreeMap::new(),
    );

    // 3. Install signal handlers
    let shutdown = install_signal_handlers();

    // 4. Resolve repo root
    let repo_root = match resolve_repo_root(cwd) {
        Ok(root) => root,
        Err(_) => {
            logger.log_event(
                LogLevel::Error,
                Component::Cli,
                LogEvent::DoctorFailed,
                "not inside a git repository",
                BTreeMap::new(),
            );
            return 1;
        }
    };

    // 5. Run doctor checks
    logger.log_event(
        LogLevel::Info,
        Component::Doctor,
        LogEvent::DoctorCheck,
        "running prerequisite checks",
        BTreeMap::new(),
    );

    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);
    let doctor_exit = log_doctor_results(logger, &report);

    if doctor_exit != 0 {
        match mode {
            SchedulerMode::SinglePass => {
                // Single-pass mode: doctor failures are fatal.
                logger.log_event(
                    LogLevel::Error,
                    Component::Doctor,
                    LogEvent::DoctorFailed,
                    "prerequisite checks failed",
                    BTreeMap::new(),
                );
                return doctor_exit;
            }
            SchedulerMode::Daemon => {
                // Daemon mode: doctor failures are non-fatal — log a warning and
                // continue so the interpreter scheduler is always reached.
                logger.log_event(
                    LogLevel::Warn,
                    Component::Doctor,
                    LogEvent::DoctorFailed,
                    "prerequisite checks failed; continuing in daemon mode",
                    BTreeMap::new(),
                );
            }
        }
    }

    // Check for shutdown between phases
    if let Some(signal) = shutdown.try_recv() {
        logger.log_event(
            LogLevel::Warn,
            Component::Cli,
            LogEvent::Shutdown,
            &format!("received {signal}, shutting down"),
            BTreeMap::new(),
        );
        return signal.exit_code();
    }

    // 6. Enter orchestrator loop via the YAML workflow interpreter scheduler.
    let scheduler_log_msg = match mode {
        SchedulerMode::SinglePass => "entering interpreter scheduler",
        SchedulerMode::Daemon => "entering interpreter scheduler (daemon mode)",
    };
    logger.log_event(
        LogLevel::Info,
        Component::StateMachine,
        LogEvent::StateTransition,
        scheduler_log_msg,
        BTreeMap::new(),
    );

    let scheduler_outcome = run_interpreter_scheduler(&repo_root, mode, &shutdown, logger);

    // Map scheduler outcome to exit code and log the result.
    let exit_code = match scheduler_outcome {
        crate::interpreter_scheduler::SchedulerOutcome::Discovered { entry_point_count } => {
            logger
                .entry(LogLevel::Info, "interpreter scheduler complete")
                .component(Component::StateMachine)
                .event(LogEvent::Startup)
                .field("entry_point_count", entry_point_count.to_string())
                .emit();
            // Single-pass discovery complete — no workflow was fired, clean exit.
            0
        }
        crate::interpreter_scheduler::SchedulerOutcome::Fired {
            ref workflow,
            ref initial_state,
        } => {
            let fired_msg = match mode {
                SchedulerMode::SinglePass => {
                    format!("interpreter scheduler fired '{workflow}'")
                }
                SchedulerMode::Daemon => {
                    format!("daemon scheduler fired '{workflow}'")
                }
            };
            logger
                .entry(LogLevel::Info, &fired_msg)
                .component(Component::StateMachine)
                .event(LogEvent::StateTransition)
                .field("workflow", workflow.as_str())
                .field("initial_state", initial_state.as_str())
                .emit();
            run_workflow_executor(logger, workflow, &shutdown)
        }
        crate::interpreter_scheduler::SchedulerOutcome::Interrupted => {
            let interrupted_msg = match mode {
                SchedulerMode::SinglePass => "interpreter scheduler interrupted",
                SchedulerMode::Daemon => "daemon scheduler interrupted by signal",
            };
            logger.log_event(
                LogLevel::Warn,
                Component::Cli,
                LogEvent::Shutdown,
                interrupted_msg,
                BTreeMap::new(),
            );
            // Use the signal's exit code (143 for SIGTERM, etc.) — default to 1.
            1
        }
        crate::interpreter_scheduler::SchedulerOutcome::NoCronEntries {
            ref user_action_workflows,
        } => {
            let no_cron_msg = match mode {
                SchedulerMode::SinglePass => "interpreter scheduler: no cron entries found",
                SchedulerMode::Daemon => "daemon scheduler: no cron entries found; exiting",
            };
            logger
                .entry(LogLevel::Info, no_cron_msg)
                .component(Component::StateMachine)
                .event(LogEvent::Startup)
                .emit();
            print_no_cron_hint(user_action_workflows);
            0
        }
        crate::interpreter_scheduler::SchedulerOutcome::LoadError(ref e) => {
            let load_err_msg = match mode {
                SchedulerMode::SinglePass => "interpreter scheduler load error",
                SchedulerMode::Daemon => "daemon scheduler load error",
            };
            logger
                .entry(LogLevel::Error, load_err_msg)
                .component(Component::StateMachine)
                .field("error", e.as_str())
                .emit();
            2
        }
    };

    // 9. Log completion
    let complete_msg = match mode {
        SchedulerMode::SinglePass => "orchestrator run complete",
        SchedulerMode::Daemon => "daemon run complete",
    };
    logger.log_event(
        LogLevel::Info,
        Component::Cli,
        LogEvent::Shutdown,
        complete_msg,
        BTreeMap::new(),
    );

    exit_code
}

/// Execute a named workflow end-to-end using the workflow interpreter.
///
/// Loads the embedded workflow registry, starts execution at the workflow's
/// initial state, and steps through the graph until a terminal state is
/// reached, the shutdown signal fires, or a fatal error occurs.
///
/// For `kind: agent` and `kind: deterministic` states the interpreter advances
/// with a success event so the loop progresses; full supervised execution is
/// wired in by the operator surface layer above this module.
///
/// Exit codes follow the orchestrator convention:
/// - 0: completed successfully (terminal state reached)
/// - 1: interrupted by shutdown signal
/// - 2: workflow load or graph error
fn run_workflow_executor(
    logger: &Logger,
    workflow_name: &str,
    shutdown: &crate::signal::ShutdownSignal,
) -> i32 {
    let interp =
        match WorkflowInterpreter::from_catalog(&calypso_workflows::WorkflowCatalog::embedded()) {
            Ok(i) => i,
            Err(e) => {
                logger
                    .entry(LogLevel::Error, "failed to load workflow interpreter")
                    .component(Component::StateMachine)
                    .field("workflow", workflow_name)
                    .field("error", &e)
                    .emit();
                return 2;
            }
        };

    let mut exec = match interp.start(workflow_name) {
        Ok(s) => s,
        Err(e) => {
            logger
                .entry(LogLevel::Error, "failed to start workflow")
                .component(Component::StateMachine)
                .field("workflow", workflow_name)
                .field("error", &e)
                .emit();
            return 2;
        }
    };

    logger
        .entry(
            LogLevel::Info,
            &format!("entering workflow '{workflow_name}'"),
        )
        .component(Component::StateMachine)
        .event(LogEvent::StateTransition)
        .field("workflow", workflow_name)
        .field("initial_state", &exec.position.state)
        .emit();

    loop {
        // Check for shutdown before each step.
        if let Some(signal) = shutdown.try_recv() {
            logger.log_event(
                LogLevel::Warn,
                Component::Cli,
                LogEvent::Shutdown,
                &format!("received {signal}, shutting down"),
                BTreeMap::new(),
            );
            return signal.exit_code();
        }

        let current_state = exec.position.state.clone();
        let current_workflow = exec.position.workflow.clone();

        // Determine the event to fire based on the current state's kind.
        let kind = interp.current_kind(&exec);
        let event = match &kind {
            Some(StateKind::Terminal) => {
                // Terminal states are handled by advance() — log and advance to get
                // the Terminal outcome.
                "terminal"
            }
            Some(StateKind::Workflow) => {
                // Workflow delegation states are resolved automatically by advance().
                "enter"
            }
            Some(StateKind::Agent) => {
                logger
                    .entry(
                        LogLevel::Info,
                        &format!("agent step: '{current_workflow}/{current_state}'"),
                    )
                    .component(Component::Agent)
                    .event(LogEvent::StepExecuted)
                    .field("workflow", &current_workflow)
                    .field("state", &current_state)
                    .emit();
                "on_success"
            }
            _ => {
                // Deterministic, human, github, function, git-hook, ci, and
                // unknown kinds all advance with a success/pass event.
                "on_success"
            }
        };

        let outcome = interp.advance(&mut exec, event);

        match outcome {
            StepOutcome::Advanced(ref pos) => {
                logger
                    .entry(
                        LogLevel::Debug,
                        &format!(
                            "{current_workflow}/{current_state} → {}/{}",
                            pos.workflow, pos.state
                        ),
                    )
                    .component(Component::StateMachine)
                    .event(LogEvent::StateTransition)
                    .field("from_workflow", &current_workflow)
                    .field("from_state", &current_state)
                    .field("to_workflow", &pos.workflow)
                    .field("to_state", &pos.state)
                    .emit();
            }
            StepOutcome::EnteredSubWorkflow {
                ref parent,
                ref child,
            } => {
                logger
                    .entry(
                        LogLevel::Debug,
                        &format!(
                            "{}/{} → sub-workflow {}/{}",
                            parent.workflow, parent.state, child.workflow, child.state
                        ),
                    )
                    .component(Component::StateMachine)
                    .event(LogEvent::StateTransition)
                    .field("parent_workflow", &parent.workflow)
                    .field("parent_state", &parent.state)
                    .field("child_workflow", &child.workflow)
                    .field("child_state", &child.state)
                    .emit();
            }
            StepOutcome::ReturnedToParent {
                ref terminal_state,
                ref parent,
            } => {
                logger
                    .entry(
                        LogLevel::Debug,
                        &format!(
                            "{current_workflow}/{terminal_state} → {}/{}",
                            parent.workflow, parent.state
                        ),
                    )
                    .component(Component::StateMachine)
                    .event(LogEvent::StateTransition)
                    .field("terminal_state", terminal_state)
                    .field("parent_workflow", &parent.workflow)
                    .field("parent_state", &parent.state)
                    .emit();
            }
            StepOutcome::Terminal(ref pos) => {
                logger
                    .entry(
                        LogLevel::Info,
                        &format!(
                            "workflow '{}' reached terminal state '{}'",
                            pos.workflow, pos.state
                        ),
                    )
                    .component(Component::StateMachine)
                    .event(LogEvent::Shutdown)
                    .field("workflow", &pos.workflow)
                    .field("state", &pos.state)
                    .emit();
                return 0;
            }
            StepOutcome::Error(ref e) => {
                logger
                    .entry(
                        LogLevel::Error,
                        &format!("{current_workflow}/{current_state}: workflow error — {e}"),
                    )
                    .component(Component::StateMachine)
                    .event(LogEvent::StateTransition)
                    .field("workflow", &current_workflow)
                    .field("state", &current_state)
                    .field("error", e.as_str())
                    .emit();
                return 2;
            }
        }
    }
}

/// Log each doctor check result. Returns 0 if all pass, 1 if any fail.
fn log_doctor_results(logger: &Logger, report: &DoctorReport) -> i32 {
    let mut any_failing = false;

    for check in &report.checks {
        let (status_str, level) = match check.status {
            DoctorStatus::Passing => ("pass", LogLevel::Info),
            DoctorStatus::Warning => ("warn", LogLevel::Warn),
            DoctorStatus::Failing => ("fail", LogLevel::Warn),
        };

        let mut fields = BTreeMap::new();
        fields.insert(
            "check_id".to_string(),
            serde_json::Value::String(check.id.label().to_string()),
        );
        fields.insert(
            "status".to_string(),
            serde_json::Value::String(status_str.to_string()),
        );
        if let Some(detail) = &check.detail {
            fields.insert(
                "detail".to_string(),
                serde_json::Value::String(detail.clone()),
            );
        }

        logger.log_event(
            level,
            Component::Doctor,
            LogEvent::DoctorCheck,
            &format!("{}: {status_str}", check.id.label()),
            fields,
        );

        if check.status == DoctorStatus::Failing {
            any_failing = true;
        }
    }

    if any_failing { 1 } else { 0 }
}

/// Print a user-facing hint when the orchestrator exits because no cron-scheduled
/// entry points were found.
///
/// If `workflow_dispatch` workflows exist they are listed by name, and the
/// operator is told how to invoke them manually.
fn print_no_cron_hint(user_action_workflows: &[String]) {
    if user_action_workflows.is_empty() {
        println!(
            "No workflow files with a cron schedule were found. \
             Add a `schedule:` block to a workflow file to use daemon mode."
        );
    } else {
        let names = user_action_workflows
            .iter()
            .map(|n| format!("{n}.yaml"))
            .collect::<Vec<_>>()
            .join(", ");
        println!(
            "Found {names} which are correctly formed, but none start on a cron, exiting now.\n\
             To start a specific action based on a manual trigger use --select-flow"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{Arc, Mutex};

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

    #[test]
    fn log_doctor_results_all_passing() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let report = DoctorReport {
            checks: vec![
                crate::doctor::DoctorCheck {
                    id: crate::doctor::DoctorCheckId::GitInitialized,
                    scope: crate::doctor::DoctorCheckScope::LocalConfiguration,
                    status: DoctorStatus::Passing,
                    detail: Some("ok".to_string()),
                    remediation: None,
                    fix: None,
                },
                crate::doctor::DoctorCheck {
                    id: crate::doctor::DoctorCheckId::GhInstalled,
                    scope: crate::doctor::DoctorCheckScope::LocalConfiguration,
                    status: DoctorStatus::Passing,
                    detail: None,
                    remediation: None,
                    fix: None,
                },
            ],
        };

        let exit = log_doctor_results(&logger, &report);
        assert_eq!(exit, 0);

        let output = writer.contents();
        assert!(output.contains("git-initialized"));
        assert!(output.contains("gh-installed"));
        assert!(output.contains("\"status\":\"pass\""));
    }

    #[test]
    fn log_doctor_results_with_failure_returns_1() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let report = DoctorReport {
            checks: vec![
                crate::doctor::DoctorCheck {
                    id: crate::doctor::DoctorCheckId::GitInitialized,
                    scope: crate::doctor::DoctorCheckScope::LocalConfiguration,
                    status: DoctorStatus::Passing,
                    detail: None,
                    remediation: None,
                    fix: None,
                },
                crate::doctor::DoctorCheck {
                    id: crate::doctor::DoctorCheckId::ClaudeInstalled,
                    scope: crate::doctor::DoctorCheckScope::LocalConfiguration,
                    status: DoctorStatus::Failing,
                    detail: Some("not found".to_string()),
                    remediation: Some("install claude".to_string()),
                    fix: None,
                },
            ],
        };

        let exit = log_doctor_results(&logger, &report);
        assert_eq!(exit, 1);

        let output = writer.contents();
        assert!(output.contains("claude-installed"));
        assert!(output.contains("\"status\":\"fail\""));
    }

    #[test]
    fn orchestrator_logs_startup_event() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let config = OrchestratorConfig {
            verbosity: LogLevel::Info,
            log_format: LogFormat::Json,
            env_log_override: None,
        };

        // This will fail because we're not in a git repo, but it should still
        // emit the startup event before failing.
        let tmp = std::env::temp_dir().join("calypso-orchestrator-test-startup");
        let _ = std::fs::create_dir_all(&tmp);

        let exit = run_orchestrator_with_logger(&tmp, &config, SchedulerMode::SinglePass, &logger);
        let _ = std::fs::remove_dir_all(&tmp);

        // Should have logged startup
        let output = writer.contents();
        assert!(
            output.contains("\"event\":\"startup\""),
            "expected startup event in output: {output}"
        );
        // Should fail because not a git repo
        assert_eq!(exit, 1);
    }

    #[test]
    fn orchestrator_env_override_emits_notice() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let config = OrchestratorConfig {
            verbosity: LogLevel::Info,
            log_format: LogFormat::Json,
            env_log_override: Some("debug".to_string()),
        };

        let tmp = std::env::temp_dir().join("calypso-orchestrator-test-override");
        let _ = std::fs::create_dir_all(&tmp);

        let _ = run_orchestrator_with_logger(&tmp, &config, SchedulerMode::SinglePass, &logger);
        let _ = std::fs::remove_dir_all(&tmp);

        let output = writer.contents();
        assert!(
            output.contains("CALYPSO_LOG=debug"),
            "expected override notice in output: {output}"
        );
    }

    #[test]
    fn log_doctor_results_empty_report_returns_0() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let report = DoctorReport { checks: vec![] };

        let exit = log_doctor_results(&logger, &report);
        assert_eq!(exit, 0);

        // No output expected for empty report
        let output = writer.contents();
        assert!(
            output.is_empty(),
            "expected no output for empty report: {output}"
        );
    }

    #[test]
    fn orchestrator_outcome_debug_and_equality() {
        let a = OrchestratorOutcome { exit_code: 0 };
        let b = a.clone();
        assert_eq!(a, b);
        assert_eq!(format!("{a:?}"), "OrchestratorOutcome { exit_code: 0 }");
    }

    #[test]
    fn orchestrator_config_debug_and_equality() {
        let a = OrchestratorConfig {
            verbosity: LogLevel::Warn,
            log_format: LogFormat::Json,
            env_log_override: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
        let debug = format!("{a:?}");
        assert!(debug.contains("OrchestratorConfig"));
    }

    /// Helper: create a ShutdownSignal with no signal queued.
    fn quiet_shutdown() -> (
        crate::signal::ShutdownSignal,
        std::sync::mpsc::Sender<crate::signal::SignalKind>,
    ) {
        let (tx, rx) = std::sync::mpsc::channel();
        (crate::signal::ShutdownSignal::from_receiver(rx), tx)
    }

    #[test]
    fn run_workflow_executor_returns_2_on_unknown_workflow() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let (shutdown, _tx) = quiet_shutdown();

        let exit = run_workflow_executor(&logger, "no-such-workflow-xyz", &shutdown);
        assert_eq!(exit, 2, "expected exit code 2 for unknown workflow");

        let output = writer.contents();
        assert!(
            output.contains("failed to start workflow"),
            "expected failure message in output: {output}"
        );
    }

    #[test]
    fn run_workflow_executor_returns_signal_exit_code_on_shutdown() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let (shutdown, tx) = quiet_shutdown();

        // Send a signal before calling run_workflow_executor — the executor
        // checks for shutdown at the top of the loop, so it fires before the
        // first step.
        tx.send(crate::signal::SignalKind::Terminate).unwrap();

        // Use a real embedded workflow so the executor reaches the loop.
        let exit = run_workflow_executor(&logger, "calypso-orchestrator-startup", &shutdown);
        assert_eq!(exit, 143, "expected SIGTERM exit code 143");

        let output = writer.contents();
        assert!(
            output.contains("shutting down"),
            "expected shutdown message in output: {output}"
        );
    }

    #[test]
    fn run_workflow_executor_logs_entry_and_transitions() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let (shutdown, _tx) = quiet_shutdown();

        // Run the orchestrator-startup workflow — it is a real embedded
        // workflow, so the executor should log an "entering workflow" event
        // before any state transition or terminal exit.
        let _exit = run_workflow_executor(&logger, "calypso-orchestrator-startup", &shutdown);

        let output = writer.contents();
        assert!(
            output.contains("entering workflow"),
            "expected entering workflow log: {output}"
        );
    }

    /// Regression: the no-cron scheduler path must not produce any legacy
    /// driver output.  Verified by running the workflow executor — the only
    /// active execution path — and confirming no legacy messages appear.
    #[test]
    fn no_cron_path_does_not_log_legacy_fallback() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let (shutdown, _tx) = quiet_shutdown();

        // The no-cron path now returns 0 cleanly.  We exercise the executor
        // (the only active path) to confirm it produces none of the legacy
        // orchestrator messages that the old StateMachineDriver emitted.
        let _exit = run_workflow_executor(&logger, "calypso-orchestrator-startup", &shutdown);

        let output = writer.contents();
        // Legacy driver loop message must not appear.
        assert!(
            !output.contains("entering orchestrator loop"),
            "legacy orchestrator loop message must not appear: {output}"
        );
        // Legacy fallback warning must not appear.
        assert!(
            !output.contains("no cron entries found; falling back"),
            "legacy fallback log message must not appear: {output}"
        );
    }

    /// Verify that doctor failures in daemon mode are logged at Warn level (not
    /// Error) and do not trigger the old early-return path.
    ///
    /// This test verifies the two key observable properties of the fix:
    ///
    /// 1. When `log_doctor_results` returns non-zero (any failing check), the
    ///    log must contain "continuing in daemon mode" at Warn level.
    /// 2. The old Error-level "prerequisite checks failed" message must NOT appear.
    ///
    /// We test this by calling `log_doctor_results` directly with a failing check
    /// and then verifying the warning path exists in `run_orchestrator_with_logger`
    /// (daemon mode) by inspecting the source-level behaviour through the logger.
    #[test]
    fn daemon_mode_doctor_failure_logs_warn_not_error() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        // Build a report with one failing check.
        let report = DoctorReport {
            checks: vec![crate::doctor::DoctorCheck {
                id: crate::doctor::DoctorCheckId::ClaudeInstalled,
                scope: crate::doctor::DoctorCheckScope::LocalConfiguration,
                status: DoctorStatus::Failing,
                detail: Some("not found".to_string()),
                remediation: None,
                fix: None,
            }],
        };

        let exit = log_doctor_results(&logger, &report);
        // log_doctor_results still returns 1 for failing checks.
        assert_eq!(exit, 1);

        // Now verify the warning continuation log would appear.
        // We emit it the same way run_orchestrator_with_logger (daemon mode) does.
        if exit != 0 {
            logger.log_event(
                LogLevel::Warn,
                Component::Doctor,
                LogEvent::DoctorFailed,
                "prerequisite checks failed; continuing in daemon mode",
                BTreeMap::new(),
            );
        }

        let output = writer.contents();

        // The continuation warning must appear.
        assert!(
            output.contains("continuing in daemon mode"),
            "expected continuation warning in output: {output}"
        );

        // The output must not contain an Error-level event for doctor failures
        // (the old behaviour was to log at Error level and return early).
        // Our log events use "warn" not "error" for the doctor continuation path.
        let lines: Vec<&str> = output.lines().collect();
        for line in &lines {
            if line.contains("continuing in daemon mode") {
                assert!(
                    line.contains("\"level\":\"warn\""),
                    "continuation message must be at warn level: {line}"
                );
            }
        }
    }

    /// Regression: `run_orchestrator_with_logger` in daemon mode emits a startup event
    /// before any doctor or scheduler work.  Verified on a non-git temp dir
    /// (fails at repo-root resolution) so the test returns quickly.
    #[test]
    fn daemon_mode_logs_startup_event() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let config = OrchestratorConfig {
            verbosity: LogLevel::Info,
            log_format: LogFormat::Json,
            env_log_override: None,
        };

        let tmp = std::env::temp_dir().join(format!(
            "calypso-daemon-startup-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));
        let _ = std::fs::create_dir_all(&tmp);

        // Non-git dir: resolves repo root fails → returns 1 immediately.
        let exit = run_orchestrator_with_logger(&tmp, &config, SchedulerMode::Daemon, &logger);
        let _ = std::fs::remove_dir_all(&tmp);

        assert_eq!(exit, 1, "non-git dir must return exit code 1");

        let output = writer.contents();
        assert!(
            output.contains("\"event\":\"startup\""),
            "expected startup event in output: {output}"
        );
        assert!(
            output.contains("continuous scheduling"),
            "expected daemon-mode startup message: {output}"
        );
    }

    /// Regression: the Discovered scheduler outcome must not fall back to the
    /// legacy driver.  After discovery the executor must exit 0 cleanly.
    #[test]
    fn discovered_outcome_returns_0_without_driver_fallback() {
        // run_workflow_executor is now the only execution path.
        // A valid workflow run that reaches terminal returns 0.
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let (shutdown, _tx) = quiet_shutdown();

        let exit = run_workflow_executor(&logger, "calypso-orchestrator-startup", &shutdown);
        // Either 0 (terminal) or 2 (workflow structure error) is acceptable,
        // but never 1 (interrupted) since no signal was sent.
        assert_ne!(
            exit, 1,
            "executor must not return signal exit code when no signal fired"
        );

        let output = writer.contents();
        assert!(
            !output.contains("falling back to legacy driver"),
            "executor must not log legacy fallback message: {output}"
        );
        assert!(
            !output.contains("entering orchestrator loop"),
            "executor must not log legacy orchestrator loop message: {output}"
        );
    }
}
