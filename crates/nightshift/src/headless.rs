//! Headless orchestrator loop — single-pass doctor, state load, and gate evaluation.
//!
//! This module implements the headless mode for CI / daemon environments where
//! no interactive TUI is available.  All output is structured log events written
//! to stderr via the [`Logger`](crate::telemetry::Logger).

use std::collections::BTreeMap;
use std::path::Path;

use crate::app::{gate_status_label, resolve_repo_root};
use crate::doctor::{DoctorReport, DoctorStatus, HostDoctorEnvironment, collect_doctor_report};
use crate::driver::{DriverMode, DriverStepResult, StateMachineDriver};
use crate::execution::ExecutionConfig;
use crate::github::{HostGithubEnvironment, collect_github_report};
use crate::policy::{HostPolicyEnvironment, collect_policy_evidence};
use crate::signal::install_signal_handlers;
use crate::state::RepositoryState;
use crate::telemetry::{Component, LogEvent, LogFormat, LogLevel, Logger};
use crate::template::load_embedded_template_set;

/// Configuration resolved from CLI flags when `--headless` is active.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessConfig {
    /// Resolved verbosity: Warn (default), Info (`-v`), or Debug (`-vv`).
    pub verbosity: LogLevel,
    /// Output format for log lines.
    pub log_format: LogFormat,
    /// If both `-v`/`-vv` and `CALYPSO_LOG` are set, captures the env value
    /// so the caller can emit a notice.
    pub env_log_override: Option<String>,
}

/// Outcome of a headless run, used for testing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessOutcome {
    pub exit_code: i32,
}

/// Run the headless orchestrator with the given configuration.
///
/// Returns the process exit code (0 = success, 1 = doctor failure / error).
pub fn run_headless(cwd: &Path, config: &HeadlessConfig) -> i32 {
    let logger = Logger::with_level(config.verbosity).with_format(config.log_format);
    run_headless_with_logger(cwd, config, &logger)
}

/// Inner implementation that accepts a logger — useful for testing.
pub fn run_headless_with_logger(cwd: &Path, config: &HeadlessConfig, logger: &Logger) -> i32 {
    // 1. If env override, emit notice
    if let Some(env_val) = &config.env_log_override {
        logger.log_level_override_notice(env_val, config.verbosity);
    }

    // 2. Log startup
    logger.log_event(
        LogLevel::Info,
        Component::Cli,
        LogEvent::Startup,
        "calypso headless mode starting",
        BTreeMap::new(),
    );

    // 3. Install signal handlers
    let shutdown = install_signal_handlers();

    // 4. Resolve repo root
    let repo_root = match resolve_repo_root(cwd) {
        Some(root) => root,
        None => {
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
        logger.log_event(
            LogLevel::Error,
            Component::Doctor,
            LogEvent::DoctorFailed,
            "prerequisite checks failed",
            BTreeMap::new(),
        );
        return doctor_exit;
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

    // 6. Load or initialise state
    let state_path = repo_root.join(".calypso").join("state.json");

    let state = match RepositoryState::load_from_path(&state_path) {
        Ok(state) => {
            logger
                .entry(LogLevel::Info, "state loaded")
                .component(Component::StateMachine)
                .field(
                    "workflow_state",
                    state.current_feature.workflow_state.as_str(),
                )
                .field("feature_id", &state.current_feature.feature_id)
                .emit();
            state
        }
        Err(e) => {
            logger
                .entry(LogLevel::Error, "failed to load state")
                .component(Component::StateMachine)
                .field("error", e.to_string())
                .emit();
            return 1;
        }
    };

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

    // 7. Evaluate gates (single-pass)
    let gate_exit = evaluate_gates_headless(logger, &repo_root, &state);

    if gate_exit != 0 {
        return gate_exit;
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

    // 8. Enter orchestrator loop (state machine driver)
    let exit_code = run_driver_loop(logger, &state_path, &shutdown);

    // 9. Log completion
    logger.log_event(
        LogLevel::Info,
        Component::Cli,
        LogEvent::Shutdown,
        "headless run complete",
        BTreeMap::new(),
    );

    exit_code
}

/// Run the state machine driver in auto mode, logging each step result.
///
/// Exit codes follow the headless convention:
/// - 0: completed successfully (terminal state reached)
/// - 2: state machine error (invalid state, transition failure, persistence error)
/// - 3: agent failure (provider error, agent aborted, unrecoverable execution failure)
fn run_driver_loop(
    logger: &Logger,
    state_path: &Path,
    shutdown: &crate::signal::ShutdownSignal,
) -> i32 {
    let template = match load_embedded_template_set() {
        Ok(t) => t,
        Err(e) => {
            logger
                .entry(LogLevel::Error, "failed to load embedded templates")
                .component(Component::StateMachine)
                .field("error", e.to_string())
                .emit();
            return 2;
        }
    };

    let driver = StateMachineDriver {
        mode: DriverMode::Auto,
        state_path: state_path.to_path_buf(),
        template,
        config: ExecutionConfig::default(),
        executor: None,
    };

    logger.log_event(
        LogLevel::Info,
        Component::StateMachine,
        LogEvent::StateTransition,
        "entering orchestrator loop",
        BTreeMap::new(),
    );

    loop {
        // Check for shutdown before each step
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

        let result = driver.step();

        match &result {
            DriverStepResult::Advanced(state) => {
                let mut fields = BTreeMap::new();
                fields.insert(
                    "to_state".to_string(),
                    serde_json::Value::String(state.as_str().to_string()),
                );
                logger.log_event(
                    LogLevel::Warn,
                    Component::StateMachine,
                    LogEvent::StateTransition,
                    &format!("advanced to {}", state.as_str()),
                    fields,
                );
            }
            DriverStepResult::Terminal => {
                logger.log_event(
                    LogLevel::Warn,
                    Component::StateMachine,
                    LogEvent::StateTransition,
                    "state machine reached terminal state",
                    BTreeMap::new(),
                );
                return 0;
            }
            DriverStepResult::Unchanged => {
                logger.log_event(
                    LogLevel::Info,
                    Component::StateMachine,
                    LogEvent::StateTransition,
                    "state unchanged after step",
                    BTreeMap::new(),
                );
                return 0;
            }
            DriverStepResult::ClarificationRequired(question) => {
                let mut fields = BTreeMap::new();
                fields.insert(
                    "question".to_string(),
                    serde_json::Value::String(question.clone()),
                );
                logger.log_event(
                    LogLevel::Error,
                    Component::Agent,
                    LogEvent::AgentCompleted,
                    "clarification required (non-interactive — failing)",
                    fields,
                );
                return 3;
            }
            DriverStepResult::Failed { reason } => {
                let mut fields = BTreeMap::new();
                fields.insert(
                    "reason".to_string(),
                    serde_json::Value::String(reason.clone()),
                );
                logger.log_event(
                    LogLevel::Error,
                    Component::Agent,
                    LogEvent::AgentCompleted,
                    &format!("step failed: {reason}"),
                    fields,
                );
                return 3;
            }
            DriverStepResult::Error(e) => {
                let mut fields = BTreeMap::new();
                fields.insert("error".to_string(), serde_json::Value::String(e.clone()));
                logger.log_event(
                    LogLevel::Error,
                    Component::StateMachine,
                    LogEvent::StateTransition,
                    &format!("driver error: {e}"),
                    fields,
                );
                return 2;
            }
        }
    }
}

/// Log each doctor check result. Returns 0 if all pass, 1 if any fail.
fn log_doctor_results(logger: &Logger, report: &DoctorReport) -> i32 {
    let mut any_failing = false;

    for check in &report.checks {
        let status_str = match check.status {
            DoctorStatus::Passing => "pass",
            DoctorStatus::Warning => "warn",
            DoctorStatus::Failing => "fail",
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

        let level = match check.status {
            DoctorStatus::Passing => LogLevel::Info,
            DoctorStatus::Warning => LogLevel::Warn,
            DoctorStatus::Failing => LogLevel::Warn,
        };

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

/// Evaluate gates for the current feature state and log each result.
/// Returns 0 on success, 2 on state machine error.
fn evaluate_gates_headless(logger: &Logger, repo_root: &Path, state: &RepositoryState) -> i32 {
    let template = match load_embedded_template_set() {
        Ok(t) => t,
        Err(e) => {
            logger
                .entry(LogLevel::Error, "failed to load embedded templates")
                .component(Component::StateMachine)
                .field("error", e.to_string())
                .emit();
            return 2;
        }
    };

    // Build a mutable copy of the feature to evaluate gates
    let mut feature = state.current_feature.clone();

    // Collect evidence from doctor, github, and policy
    let doctor_evidence =
        collect_doctor_report(&HostDoctorEnvironment, repo_root).to_builtin_evidence();

    let github_report = collect_github_report(&HostGithubEnvironment, &feature.pull_request);
    let github_evidence = github_report.to_builtin_evidence();

    let policy_evidence = collect_policy_evidence(&HostPolicyEnvironment, repo_root, &template);

    let evidence = doctor_evidence
        .merge(&github_evidence)
        .merge(&policy_evidence);

    evaluate_and_log_gates(logger, &mut feature, &template, &evidence)
}

/// Core gate evaluation and logging, separated from evidence collection for testability.
fn evaluate_and_log_gates(
    logger: &Logger,
    feature: &mut crate::state::FeatureState,
    template: &crate::template::TemplateSet,
    evidence: &crate::state::BuiltinEvidence,
) -> i32 {
    if let Err(e) = feature.evaluate_gates(template, evidence) {
        logger
            .entry(LogLevel::Error, "gate evaluation failed")
            .component(Component::Gate)
            .field("error", e.to_string())
            .emit();
        return 2;
    }

    // Log each gate result
    logger.log_event(
        LogLevel::Info,
        Component::Gate,
        LogEvent::GateEvaluated,
        &format!(
            "evaluating gates for state {}",
            feature.workflow_state.as_str()
        ),
        BTreeMap::new(),
    );

    for group in &feature.gate_groups {
        for gate in &group.gates {
            let mut fields = BTreeMap::new();
            fields.insert(
                "gate_id".to_string(),
                serde_json::Value::String(gate.id.clone()),
            );
            fields.insert(
                "group".to_string(),
                serde_json::Value::String(group.id.clone()),
            );
            fields.insert(
                "status".to_string(),
                serde_json::Value::String(gate_status_label(&gate.status).to_string()),
            );

            logger.log_event(
                LogLevel::Debug,
                Component::Gate,
                LogEvent::GateEvaluated,
                &format!("{}: {}", gate.id, gate_status_label(&gate.status)),
                fields,
            );
        }
    }

    // Log blocking gates summary
    let blocking = feature.blocking_gate_ids();
    if blocking.is_empty() {
        logger.log_event(
            LogLevel::Info,
            Component::Gate,
            LogEvent::GateEvaluated,
            "all gates passing",
            BTreeMap::new(),
        );
    } else {
        let mut fields = BTreeMap::new();
        fields.insert(
            "blocking_gates".to_string(),
            serde_json::Value::Array(
                blocking
                    .iter()
                    .map(|id| serde_json::Value::String(id.clone()))
                    .collect(),
            ),
        );
        fields.insert("count".to_string(), serde_json::json!(blocking.len()));

        logger.log_event(
            LogLevel::Info,
            Component::Gate,
            LogEvent::GateEvaluated,
            &format!("{} gate(s) blocking", blocking.len()),
            fields,
        );
    }

    0
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
    fn headless_logs_startup_event() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let config = HeadlessConfig {
            verbosity: LogLevel::Info,
            log_format: LogFormat::Json,
            env_log_override: None,
        };

        // This will fail because we're not in a git repo, but it should still
        // emit the startup event before failing.
        let tmp = std::env::temp_dir().join("calypso-headless-test-startup");
        let _ = std::fs::create_dir_all(&tmp);

        let exit = run_headless_with_logger(&tmp, &config, &logger);
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
    fn headless_env_override_emits_notice() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());

        let config = HeadlessConfig {
            verbosity: LogLevel::Info,
            log_format: LogFormat::Json,
            env_log_override: Some("debug".to_string()),
        };

        let tmp = std::env::temp_dir().join("calypso-headless-test-override");
        let _ = std::fs::create_dir_all(&tmp);

        let _ = run_headless_with_logger(&tmp, &config, &logger);
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

    /// Build a phony TemplateSet from the test fixture YAML files.
    fn phony_template() -> crate::template::TemplateSet {
        let sm = include_str!("../tests/fixtures/phony-template/.calypso/state-machine.yml");
        let agents = include_str!("../tests/fixtures/phony-template/.calypso/agents.yml");
        let prompts = include_str!("../tests/fixtures/phony-template/.calypso/prompts.yml");
        crate::template::TemplateSet::from_yaml_strings(sm, agents, prompts)
            .expect("phony template should parse")
    }

    /// Build a minimal FeatureState from the phony template for gate evaluation tests.
    fn phony_feature() -> crate::state::FeatureState {
        let template = phony_template();
        crate::state::FeatureState::from_template(
            "test-feature",
            "feat/test",
            "/tmp/worktree",
            crate::state::PullRequestRef {
                number: 0,
                url: String::new(),
            },
            &template,
        )
        .expect("phony template should create feature state")
    }

    #[test]
    fn evaluate_and_log_gates_all_pending_reports_blocking() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let template = phony_template();
        let mut feature = phony_feature();
        let evidence = crate::state::BuiltinEvidence::new();

        let exit = evaluate_and_log_gates(&logger, &mut feature, &template, &evidence);
        assert_eq!(exit, 0);

        let output = writer.contents();
        // Should report blocking gates since all agent gates stay pending
        assert!(
            output.contains("gate(s) blocking"),
            "expected blocking summary in output: {output}"
        );
        assert!(
            output.contains("\"event\":\"gate_evaluated\""),
            "expected gate_evaluated event in output: {output}"
        );
        // Should log individual gate results at debug level
        assert!(
            output.contains("phony-alpha-gate"),
            "expected gate id in output: {output}"
        );
    }

    #[test]
    fn evaluate_and_log_gates_all_passing() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let template = phony_template();
        let mut feature = phony_feature();

        // Manually set all gates to Passing before calling evaluate
        for group in &mut feature.gate_groups {
            for gate in &mut group.gates {
                gate.status = crate::state::GateStatus::Passing;
            }
        }

        // Use empty evidence — since gates are agent-kind they'll be reset to Pending.
        // Instead, to get all-passing, we need to NOT call evaluate_gates and just test
        // the logging portion.  Let's test the all-passing branch directly by skipping
        // evaluate_gates and going straight to the logging code.
        // Actually, evaluate_and_log_gates calls evaluate_gates which resets statuses.
        // So we need to test that the "all gates passing" branch works.
        // Agent-kind gates default to Pending, so let's use a feature with no gates.
        let mut empty_feature = phony_feature();
        empty_feature.gate_groups.clear();

        let evidence = crate::state::BuiltinEvidence::new();
        let exit = evaluate_and_log_gates(&logger, &mut empty_feature, &template, &evidence);
        assert_eq!(exit, 0);

        let output = writer.contents();
        assert!(
            output.contains("all gates passing"),
            "expected all-passing summary in output: {output}"
        );
    }

    #[test]
    fn evaluate_and_log_gates_logs_state_name() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let template = phony_template();
        let mut feature = phony_feature();
        let evidence = crate::state::BuiltinEvidence::new();

        let _ = evaluate_and_log_gates(&logger, &mut feature, &template, &evidence);

        let output = writer.contents();
        assert!(
            output.contains("evaluating gates for state new"),
            "expected state name in output: {output}"
        );
    }

    #[test]
    fn evaluate_and_log_gates_unknown_task_returns_2() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let template = phony_template();

        // Create a feature with a gate referencing a non-existent task
        let mut feature = phony_feature();
        feature.gate_groups = vec![crate::state::GateGroup {
            id: "bad-group".to_string(),
            label: "Bad Group".to_string(),
            gates: vec![crate::state::Gate {
                id: "bad-gate".to_string(),
                label: "Bad Gate".to_string(),
                task: "nonexistent-task".to_string(),
                status: crate::state::GateStatus::Pending,
            }],
        }];

        let evidence = crate::state::BuiltinEvidence::new();
        let exit = evaluate_and_log_gates(&logger, &mut feature, &template, &evidence);
        assert_eq!(exit, 2);

        let output = writer.contents();
        assert!(
            output.contains("gate evaluation failed"),
            "expected error message in output: {output}"
        );
        assert!(
            output.contains("nonexistent-task"),
            "expected task name in error output: {output}"
        );
    }

    #[test]
    fn evaluate_and_log_gates_with_mixed_statuses() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let template = phony_template();
        let mut feature = phony_feature();

        // Set first gate to Passing, leave others as Pending
        if let Some(group) = feature.gate_groups.first_mut()
            && let Some(gate) = group.gates.first_mut()
        {
            gate.status = crate::state::GateStatus::Passing;
        }

        let evidence = crate::state::BuiltinEvidence::new();
        let exit = evaluate_and_log_gates(&logger, &mut feature, &template, &evidence);
        assert_eq!(exit, 0);

        let output = writer.contents();
        // Should report blocking gates (the non-passing ones)
        assert!(
            output.contains("gate(s) blocking"),
            "expected blocking summary in output: {output}"
        );
        assert!(
            output.contains("\"count\""),
            "expected count field in output: {output}"
        );
    }

    #[test]
    fn evaluate_and_log_gates_blocking_summary_includes_gate_ids() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let template = phony_template();
        let mut feature = phony_feature();
        let evidence = crate::state::BuiltinEvidence::new();

        let _ = evaluate_and_log_gates(&logger, &mut feature, &template, &evidence);

        let output = writer.contents();
        // The blocking_gates field should contain our gate ids
        assert!(
            output.contains("blocking_gates"),
            "expected blocking_gates field in output: {output}"
        );
    }

    #[test]
    fn headless_outcome_debug_and_equality() {
        let a = HeadlessOutcome { exit_code: 0 };
        let b = a.clone();
        assert_eq!(a, b);
        assert_eq!(format!("{a:?}"), "HeadlessOutcome { exit_code: 0 }");
    }

    #[test]
    fn headless_config_debug_and_equality() {
        let a = HeadlessConfig {
            verbosity: LogLevel::Warn,
            log_format: LogFormat::Json,
            env_log_override: None,
        };
        let b = a.clone();
        assert_eq!(a, b);
        let debug = format!("{a:?}");
        assert!(debug.contains("HeadlessConfig"));
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
    fn run_driver_loop_returns_2_on_invalid_state_path() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let (shutdown, _tx) = quiet_shutdown();

        let bogus_path = std::path::Path::new("/tmp/calypso-test-no-such-state.json");
        let exit = run_driver_loop(&logger, bogus_path, &shutdown);
        assert_eq!(exit, 2, "expected exit code 2 for invalid state path");

        let output = writer.contents();
        assert!(
            output.contains("driver error"),
            "expected driver error in output: {output}"
        );
    }

    #[test]
    fn run_driver_loop_returns_signal_exit_code_on_shutdown() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone());
        let (shutdown, tx) = quiet_shutdown();

        // Send a signal before calling run_driver_loop
        tx.send(crate::signal::SignalKind::Terminate).unwrap();

        let bogus_path = std::path::Path::new("/tmp/calypso-test-no-such-state.json");
        let exit = run_driver_loop(&logger, bogus_path, &shutdown);
        assert_eq!(exit, 143, "expected SIGTERM exit code 143");

        let output = writer.contents();
        assert!(
            output.contains("shutting down"),
            "expected shutdown message in output: {output}"
        );
    }
}
