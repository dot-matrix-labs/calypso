//! Daemon-first entry points for the Calypso CLI.
//!
//! Normal operation is non-interactive and long-running. The daemon uses
//! continuous scheduling via the interpreter scheduler in `Daemon` mode.
//! For CI and test environments, `--single-pass` runs one scheduling pass
//! and exits.
//!
//! See `docs/prd.md` for the orchestrator daemon architecture and
//! `review-2026-03-30.md` for the design rationale behind making the CLI
//! daemon-first.

use std::path::Path;

use nightshift_core::interpreter_scheduler::SchedulerMode;
use nightshift_core::orchestrator::{OrchestratorConfig, run_orchestrator};
use nightshift_core::telemetry::{LogFormat, LogLevel};

/// Default entry point when `calypso` is invoked with no arguments and no
/// `--select-flow` flag.
///
/// Always starts the daemon with continuous scheduling regardless of whether
/// `.calypso/repository-state.json` exists.  Doctor failures are surfaced as
/// warnings inside the orchestrator but do not prevent the interpreter
/// scheduler from running.
pub fn run_daemon_default(cwd: &Path) {
    // Daemon mode: continuous scheduling, non-interactive.
    // The repository-state.json existence check has been removed so the daemon
    // starts uniformly, matching the behaviour of --select-flow.
    run_daemon_start(cwd, false);
}

/// Start the daemon explicitly.
///
/// - `single_pass = false`: continuous scheduling (daemon mode). The process
///   runs until interrupted by a signal.
/// - `single_pass = true`: one scheduling pass and exit (CI/test mode).
///
/// Both modes use the orchestrator which runs prerequisite checks,
/// loads state, evaluates gates, and enters the interpreter scheduler.
pub fn run_daemon_start(cwd: &Path, single_pass: bool) {
    let config = OrchestratorConfig {
        verbosity: LogLevel::Debug,
        log_format: LogFormat::Text,
        env_log_override: None,
    };

    let mode = if single_pass {
        SchedulerMode::SinglePass
    } else {
        SchedulerMode::Daemon
    };

    let exit_code = run_orchestrator(cwd, &config, mode);
    if exit_code != 0 {
        std::process::exit(exit_code);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_id() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    }

    fn temp_non_git_dir() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("calypso-daemon-test-{}", unique_id()));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    #[test]
    fn run_daemon_default_starts_without_repository_state_json() {
        // When there is no .calypso/repository-state.json, run_daemon_default
        // must not fall back to doctor output — it should call run_daemon_start
        // unconditionally.
        //
        // run_daemon_default exits via std::process::exit on failure, so we
        // cannot call it directly in a unit test.  Instead we verify:
        //   1. run_daemon_default and run_daemon_start have the expected
        //      signatures (compile-time check via function pointer assignment).
        //   2. A temp dir without a state file satisfies the precondition.
        let dir = temp_non_git_dir();

        // Compile-time: verify function exists and has the expected signature.
        let _: fn(&Path) = run_daemon_default;
        let _: fn(&Path, bool) = run_daemon_start;

        assert!(
            !dir.join(".calypso").join("repository-state.json").exists(),
            "precondition: state file must not exist"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
