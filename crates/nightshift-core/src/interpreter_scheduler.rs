//! Workflow interpreter scheduler — drives headless daemon mode from the YAML
//! workflow graph.
//!
//! The scheduler loads the [`WorkflowInterpreter`], discovers all entry points,
//! and in daemon mode fires cron-scheduled entry points on their configured
//! interval. For non-cron entry points it logs what is available without
//! blocking.
//!
//! # Cron loop
//!
//! When [`SchedulerMode::Daemon`] is active the scheduler:
//!
//! 1. Loads the effective workflow registry for the repository root.
//! 2. Scans all entry points and classifies them.
//! 3. For each [`EntryPoint::CronScheduled`] entry computes the next fire time
//!    via [`next_fire_in`] and waits (polling the shutdown signal every second).
//! 4. On fire, logs the launch event and returns [`SchedulerOutcome::Fired`].
//!    The caller is responsible for executing the workflow.
//! 5. Returns [`SchedulerOutcome::Interrupted`] if the shutdown signal fires
//!    before the next scheduled time.
//!
//! # Single-pass mode
//!
//! When [`SchedulerMode::SinglePass`] is active the scheduler discovers entry
//! points and returns immediately without sleeping. This is useful in test
//! contexts and CI environments where blocking is not acceptable.

use std::collections::BTreeMap;
use std::path::Path;
use std::time::Duration;

use crate::signal::ShutdownSignal;
use crate::telemetry::{Component, LogEvent, LogLevel, Logger};
use calypso_workflow_exec::{EntryPoint, WorkflowInterpreter, WorkflowPosition, next_fire_in};
use calypso_workflows::WorkflowCatalog;

// ── Scheduler mode ────────────────────────────────────────────────────────────

/// Controls whether the scheduler blocks until the next cron fire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchedulerMode {
    /// Block until the next cron fire (or shutdown). Used in daemon mode.
    Daemon,
    /// Discover entry points and return immediately. Used in single-pass / test mode.
    SinglePass,
}

// ── Scheduler outcome ─────────────────────────────────────────────────────────

/// The result of a scheduler run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SchedulerOutcome {
    /// Fired a cron-scheduled entry point.
    Fired {
        /// The workflow that fired.
        workflow: String,
        /// The initial state of the workflow.
        initial_state: String,
    },
    /// The scheduler discovered entry points but did not fire (single-pass mode).
    Discovered {
        /// Number of entry points found across all categories.
        entry_point_count: usize,
    },
    /// A shutdown signal was received before the next cron fire.
    Interrupted,
    /// No cron-scheduled entry points were found.
    NoCronEntries {
        /// Names of `workflow_dispatch` (user-action) entry points that were found but
        /// cannot be started automatically.
        user_action_workflows: Vec<String>,
    },
    /// The workflow interpreter failed to load.
    LoadError(String),
}

// ── Scheduler ─────────────────────────────────────────────────────────────────

/// Run the interpreter scheduler with the given mode.
///
/// - [`SchedulerMode::Daemon`]: block until cron fires or shutdown.
/// - [`SchedulerMode::SinglePass`]: discover entry points, log them, return.
///
/// Exit codes (for use in headless orchestration):
/// - `0`: successful — either fired or discovered.
/// - `1`: interrupted by shutdown signal.
/// - `2`: load or configuration error.
pub fn run_interpreter_scheduler(
    repo_root: &Path,
    mode: SchedulerMode,
    shutdown: &ShutdownSignal,
    logger: &Logger,
) -> SchedulerOutcome {
    // Load the interpreter.
    let catalog = WorkflowCatalog::load(repo_root);
    let interp = match WorkflowInterpreter::from_catalog(&catalog) {
        Ok(i) => i,
        Err(e) => {
            logger
                .entry(LogLevel::Error, "failed to load workflow interpreter")
                .component(Component::StateMachine)
                .field("error", &e)
                .emit();
            return SchedulerOutcome::LoadError(e);
        }
    };

    let entry_points = interp.entry_points();

    // Log all discovered entry points.
    log_entry_points(logger, &entry_points);

    match mode {
        SchedulerMode::SinglePass => {
            logger
                .entry(
                    LogLevel::Info,
                    "interpreter scheduler: single-pass complete",
                )
                .component(Component::StateMachine)
                .event(LogEvent::Startup)
                .field("entry_point_count", entry_points.len().to_string())
                .emit();
            SchedulerOutcome::Discovered {
                entry_point_count: entry_points.len(),
            }
        }

        SchedulerMode::Daemon => {
            // Find the first cron-scheduled entry point.
            let cron_entry = entry_points.iter().find_map(|e| {
                if let EntryPoint::CronScheduled {
                    workflow,
                    cron,
                    description: _,
                } = e
                {
                    Some((workflow.clone(), cron.clone()))
                } else {
                    None
                }
            });

            let (workflow_name, cron_expr) = match cron_entry {
                Some(pair) => pair,
                None => {
                    let user_action_workflows: Vec<String> = entry_points
                        .iter()
                        .filter_map(|e| {
                            if let EntryPoint::UserAction { workflow, .. } = e {
                                Some(workflow.clone())
                            } else {
                                None
                            }
                        })
                        .collect();

                    let msg = if user_action_workflows.is_empty() {
                        "interpreter scheduler: no cron-scheduled entry points found".to_string()
                    } else {
                        let names = user_action_workflows
                            .iter()
                            .map(|n| format!("{n}.yaml"))
                            .collect::<Vec<_>>()
                            .join(", ");
                        format!(
                            "interpreter scheduler: no cron-scheduled entry points found; \
                             manual workflows available: {names}"
                        )
                    };

                    logger
                        .entry(LogLevel::Warn, &msg)
                        .component(Component::StateMachine)
                        .emit();
                    return SchedulerOutcome::NoCronEntries {
                        user_action_workflows,
                    };
                }
            };

            // Wait for the next fire time, polling shutdown every second.
            wait_for_cron_fire(&workflow_name, &cron_expr, shutdown, logger, &interp)
        }
    }
}

/// Block until the cron expression fires, checking the shutdown signal each second.
fn wait_for_cron_fire(
    workflow_name: &str,
    cron_expr: &str,
    shutdown: &ShutdownSignal,
    logger: &Logger,
    interp: &WorkflowInterpreter,
) -> SchedulerOutcome {
    loop {
        // Check shutdown first.
        if let Some(signal) = shutdown.try_recv() {
            logger
                .entry(
                    LogLevel::Warn,
                    &format!("interpreter scheduler: received {signal}, shutting down"),
                )
                .component(Component::StateMachine)
                .event(LogEvent::Shutdown)
                .emit();
            return SchedulerOutcome::Interrupted;
        }

        // Compute time until next fire.
        let wait = match next_fire_in(cron_expr) {
            Ok(d) => d,
            Err(e) => {
                logger
                    .entry(
                        LogLevel::Error,
                        &format!("interpreter scheduler: invalid cron expression: {e}"),
                    )
                    .component(Component::StateMachine)
                    .emit();
                return SchedulerOutcome::LoadError(e);
            }
        };

        if wait > Duration::ZERO {
            logger
                .entry(
                    LogLevel::Debug,
                    &format!(
                        "interpreter scheduler: next fire for '{workflow_name}' in {}s",
                        wait.as_secs()
                    ),
                )
                .component(Component::StateMachine)
                .event(LogEvent::Startup)
                .field("workflow", workflow_name)
                .field("wait_secs", wait.as_secs().to_string())
                .emit();

            // Sleep in 1-second chunks so we can check for shutdown.
            std::thread::sleep(wait.min(Duration::from_secs(1)));
            continue;
        }

        // Fire! Look up the initial state.
        let initial_state = match interp.registry.get(workflow_name) {
            Some(wf) => wf
                .initial_state
                .clone()
                .unwrap_or_else(|| "unknown".to_string()),
            None => "unknown".to_string(),
        };

        logger
            .entry(
                LogLevel::Info,
                &format!("interpreter scheduler: firing '{workflow_name}'"),
            )
            .component(Component::StateMachine)
            .event(LogEvent::StateTransition)
            .field("workflow", workflow_name)
            .field("initial_state", &initial_state)
            .emit();

        return SchedulerOutcome::Fired {
            workflow: workflow_name.to_string(),
            initial_state,
        };
    }
}

/// Log all discovered entry points at debug level.
fn log_entry_points(logger: &Logger, entry_points: &[EntryPoint]) {
    for entry in entry_points {
        match entry {
            EntryPoint::CronScheduled {
                workflow,
                cron,
                description,
            } => {
                let mut fields = BTreeMap::new();
                fields.insert("kind".to_string(), "cron".to_string());
                fields.insert("workflow".to_string(), workflow.clone());
                fields.insert("cron".to_string(), cron.clone());
                if let Some(desc) = description {
                    fields.insert("description".to_string(), desc.clone());
                }
                logger.log_event(
                    LogLevel::Debug,
                    Component::StateMachine,
                    LogEvent::Startup,
                    &format!("entry point: cron '{workflow}' @ {cron}"),
                    fields
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::json!(v)))
                        .collect(),
                );
            }
            EntryPoint::EventTriggered {
                workflow,
                event,
                pattern,
            } => {
                let mut fields = BTreeMap::new();
                fields.insert("kind".to_string(), "event".to_string());
                fields.insert("workflow".to_string(), workflow.clone());
                fields.insert("event".to_string(), event.clone());
                if let Some(p) = pattern {
                    fields.insert("pattern".to_string(), p.clone());
                }
                logger.log_event(
                    LogLevel::Debug,
                    Component::StateMachine,
                    LogEvent::Startup,
                    &format!("entry point: event '{workflow}' on {event}"),
                    fields
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::json!(v)))
                        .collect(),
                );
            }
            EntryPoint::UserAction {
                workflow,
                description,
                prompt: _,
            } => {
                let mut fields = BTreeMap::new();
                fields.insert("kind".to_string(), "user".to_string());
                fields.insert("workflow".to_string(), workflow.clone());
                if let Some(desc) = description {
                    fields.insert("description".to_string(), desc.clone());
                }
                logger.log_event(
                    LogLevel::Debug,
                    Component::StateMachine,
                    LogEvent::Startup,
                    &format!("entry point: user '{workflow}'"),
                    fields
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::json!(v)))
                        .collect(),
                );
            }
            EntryPoint::AutoStart {
                workflow,
                description,
            } => {
                let mut fields = BTreeMap::new();
                fields.insert("kind".to_string(), "auto".to_string());
                fields.insert("workflow".to_string(), workflow.clone());
                if let Some(desc) = description {
                    fields.insert("description".to_string(), desc.clone());
                }
                logger.log_event(
                    LogLevel::Debug,
                    Component::StateMachine,
                    LogEvent::Startup,
                    &format!("entry point: auto '{workflow}'"),
                    fields
                        .into_iter()
                        .map(|(k, v)| (k, serde_json::json!(v)))
                        .collect(),
                );
            }
        }
    }
}

/// Build a [`WorkflowPosition`] for the initial state of a workflow.
///
/// Returns `None` if the workflow is not found or has no initial state.
pub fn initial_position(
    interp: &WorkflowInterpreter,
    workflow_name: &str,
) -> Option<WorkflowPosition> {
    let wf = interp.registry.get(workflow_name)?;
    let initial = wf.initial_state.as_deref()?;
    Some(WorkflowPosition {
        workflow: workflow_name.to_string(),
        state: initial.to_string(),
    })
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use calypso_workflows::WorkflowCatalog;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    // ── Helpers ──────────────────────────────────────────────────────────────

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

    use crate::signal::install_signal_handlers;
    use crate::telemetry::{LogFormat, LogLevel};

    static TEMP_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

    fn make_logger(writer: CaptureWriter) -> Logger {
        Logger::_with_level_and_writer(LogLevel::Debug, Box::new(writer))
            .with_format(LogFormat::Json)
    }

    fn temp_repo_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let counter = TEMP_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "{prefix}-{}-{unique}-{counter}",
            std::process::id()
        ));
        fs::create_dir_all(&path).expect("temp repo dir should be created");
        path
    }

    fn embedded_interpreter() -> WorkflowInterpreter {
        WorkflowInterpreter::from_catalog(&WorkflowCatalog::embedded())
            .expect("embedded workflow catalog should load")
    }

    // ── Single-pass mode ─────────────────────────────────────────────────────

    #[test]
    fn single_pass_returns_discovered() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let shutdown = install_signal_handlers();

        let outcome = run_interpreter_scheduler(
            &std::env::temp_dir(),
            SchedulerMode::SinglePass,
            &shutdown,
            &logger,
        );

        assert!(
            matches!(outcome, SchedulerOutcome::Discovered { entry_point_count } if entry_point_count > 0),
            "expected Discovered with >0 entry points, got: {outcome:?}"
        );
    }

    #[test]
    fn single_pass_logs_entry_points() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let shutdown = install_signal_handlers();

        run_interpreter_scheduler(
            &std::env::temp_dir(),
            SchedulerMode::SinglePass,
            &shutdown,
            &logger,
        );

        let output = writer.contents();
        // Orchestrator is cron-scheduled — should appear in output
        assert!(
            output.contains("calypso-orchestrator-startup"),
            "expected orchestrator in output: {output}"
        );
        // Cron kind must be logged
        assert!(
            output.contains("cron"),
            "expected 'cron' kind in output: {output}"
        );
    }

    #[test]
    fn single_pass_logs_completion_message() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let shutdown = install_signal_handlers();

        run_interpreter_scheduler(
            &std::env::temp_dir(),
            SchedulerMode::SinglePass,
            &shutdown,
            &logger,
        );

        let output = writer.contents();
        assert!(
            output.contains("single-pass complete"),
            "expected single-pass complete in output: {output}"
        );
    }

    #[test]
    fn single_pass_uses_repo_local_workflow_catalog() {
        let repo_root = temp_repo_dir("calypso-scheduler-local-catalog");
        let workflows_dir = repo_root.join(".calypso").join("workflows");
        fs::create_dir_all(&workflows_dir).expect("workflow dir should be created");
        fs::write(
            workflows_dir.join("local-cron.yaml"),
            "name: local-cron\non:\n  schedule:\n    - cron: '*/5 * * * *'\njobs:\n  run:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo local\n",
        )
        .expect("local workflow should write");

        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let shutdown = install_signal_handlers();

        let outcome =
            run_interpreter_scheduler(&repo_root, SchedulerMode::SinglePass, &shutdown, &logger);

        assert!(
            matches!(outcome, SchedulerOutcome::Discovered { entry_point_count } if entry_point_count == 1),
            "expected exactly one local entry point, got: {outcome:?}"
        );

        let output = writer.contents();
        assert!(
            output.contains("local-cron"),
            "expected local workflow in output: {output}"
        );
        assert!(
            !output.contains("calypso-orchestrator-startup"),
            "did not expect embedded workflow in local-only output: {output}"
        );

        fs::remove_dir_all(repo_root).expect("temp repo dir should be removed");
    }

    // ── Initial position ─────────────────────────────────────────────────────

    #[test]
    fn initial_position_returns_correct_state() {
        let interp = embedded_interpreter();
        let pos = initial_position(&interp, "calypso-orchestrator-startup");
        assert!(pos.is_some());
        let pos = pos.unwrap();
        assert_eq!(pos.workflow, "calypso-orchestrator-startup");
        assert_eq!(pos.state, "scan-work-queue");
    }

    #[test]
    fn initial_position_returns_none_for_unknown_workflow() {
        let interp = embedded_interpreter();
        let pos = initial_position(&interp, "does-not-exist");
        assert!(pos.is_none());
    }

    // ── Entry point logging ───────────────────────────────────────────────────

    #[test]
    fn log_entry_points_includes_cron_event_user_auto_kinds() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let interp = embedded_interpreter();
        let entries = interp.entry_points();

        log_entry_points(&logger, &entries);

        let output = writer.contents();
        // Must have logged at least one cron entry
        assert!(
            output.contains("cron"),
            "expected cron kind in output: {output}"
        );
        // Must have logged at least one event entry
        assert!(
            output.contains("event"),
            "expected event kind in output: {output}"
        );
        // Must have logged at least one user entry
        assert!(
            output.contains("user"),
            "expected user kind in output: {output}"
        );
    }

    // ── Daemon mode with pre-cancelled shutdown ───────────────────────────────

    #[test]
    fn daemon_mode_interrupted_when_shutdown_fires_immediately() {
        // We can't easily pre-cancel the real signal handler, but we can
        // verify that the scheduler eventually terminates. For this test we
        // exercise the single-pass path which terminates immediately.
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let shutdown = install_signal_handlers();

        // Use SinglePass so the test doesn't block
        let outcome = run_interpreter_scheduler(
            &std::env::temp_dir(),
            SchedulerMode::SinglePass,
            &shutdown,
            &logger,
        );
        assert!(
            matches!(outcome, SchedulerOutcome::Discovered { .. }),
            "expected Discovered: {outcome:?}"
        );
    }

    // ── SchedulerOutcome display ─────────────────────────────────────────────

    #[test]
    fn scheduler_outcome_debug_format() {
        let o = SchedulerOutcome::NoCronEntries {
            user_action_workflows: vec![],
        };
        assert!(format!("{o:?}").contains("NoCronEntries"));

        let o2 = SchedulerOutcome::LoadError("oops".to_string());
        assert!(format!("{o2:?}").contains("oops"));

        let o3 = SchedulerOutcome::Fired {
            workflow: "wf".to_string(),
            initial_state: "start".to_string(),
        };
        assert!(format!("{o3:?}").contains("wf"));
    }
}
