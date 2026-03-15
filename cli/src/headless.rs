//! Headless orchestrator loop — single-pass doctor, state load, and gate evaluation.
//!
//! This module implements the headless mode for CI / daemon environments where
//! no interactive TUI is available.  All output is structured log events written
//! to stderr via the [`Logger`](crate::telemetry::Logger).

use std::collections::BTreeMap;
use std::path::Path;

use crate::app::{gate_status_label, resolve_repo_root};
use crate::doctor::{DoctorReport, DoctorStatus, HostDoctorEnvironment, collect_doctor_report};
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
    let exit_code = evaluate_gates_headless(logger, &repo_root, &state);

    // 8. Log completion
    logger.log_event(
        LogLevel::Info,
        Component::Cli,
        LogEvent::Shutdown,
        "headless run complete",
        BTreeMap::new(),
    );

    exit_code
}

/// Log each doctor check result. Returns 0 if all pass, 1 if any fail.
fn log_doctor_results(logger: &Logger, report: &DoctorReport) -> i32 {
    let mut any_failing = false;

    for check in &report.checks {
        let status_str = match check.status {
            DoctorStatus::Passing => "pass",
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
/// Returns 0 on success.
fn evaluate_gates_headless(logger: &Logger, repo_root: &Path, state: &RepositoryState) -> i32 {
    let template = match load_embedded_template_set() {
        Ok(t) => t,
        Err(e) => {
            logger
                .entry(LogLevel::Error, "failed to load embedded templates")
                .component(Component::StateMachine)
                .field("error", e.to_string())
                .emit();
            return 1;
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

    if let Err(e) = feature.evaluate_gates(&template, &evidence) {
        logger
            .entry(LogLevel::Error, "gate evaluation failed")
            .component(Component::Gate)
            .field("error", e.to_string())
            .emit();
        return 1;
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
}
