//! Daemon-first entry points for the Calypso CLI.
//!
//! Normal operation is non-interactive and long-running. The daemon uses
//! continuous scheduling via the interpreter scheduler in `Daemon` mode.
//! For CI and test environments, `--single-pass` runs one scheduling pass
//! and exits.
//!
//! See `docs/prd.md` for the headless daemon architecture and `review-2026-03-30.md`
//! for the design rationale behind making the CLI daemon-first.

use std::path::Path;

use nightshift_core::app::run_doctor;
use nightshift_core::headless::{HeadlessConfig, run_headless};
use nightshift_core::telemetry::{LogFormat, LogLevel};

/// Default entry point when `calypso` is invoked with no arguments and no
/// `--select-flow` flag.
///
/// If the project is initialised (`.calypso/repository-state.json` exists),
/// starts the daemon with continuous scheduling. Otherwise, falls back to
/// doctor output to guide the operator through setup.
pub fn run_daemon_default(cwd: &Path) {
    let state_path = cwd.join(".calypso").join("repository-state.json");
    if state_path.exists() {
        // Daemon mode: continuous scheduling, non-interactive.
        run_daemon_start(cwd, false);
    } else {
        // Not yet initialised: show doctor output to guide setup.
        println!("{}", run_doctor(cwd));
    }
}

/// Start the daemon explicitly.
///
/// - `single_pass = false`: continuous scheduling (daemon mode). The process
///   runs until interrupted by a signal.
/// - `single_pass = true`: one scheduling pass and exit (CI/test mode).
///
/// Both modes use the headless orchestrator which runs prerequisite checks,
/// loads state, evaluates gates, and enters the interpreter scheduler.
pub fn run_daemon_start(cwd: &Path, single_pass: bool) {
    let config = HeadlessConfig {
        verbosity: LogLevel::Debug,
        log_format: LogFormat::Text,
        env_log_override: None,
    };

    if single_pass {
        // Single-pass mode uses the existing headless orchestrator which
        // already runs the interpreter scheduler in SinglePass mode.
        let exit_code = run_headless(cwd, &config);
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
    } else {
        // Continuous daemon mode: run the headless orchestrator which uses
        // the interpreter scheduler. The current headless implementation
        // dispatches through SchedulerMode based on the run_headless call.
        //
        // We use run_headless_daemon which runs the scheduler in Daemon mode,
        // blocking until interrupted by a signal.
        let exit_code = run_headless_daemon(cwd, &config);
        if exit_code != 0 {
            std::process::exit(exit_code);
        }
    }
}

/// Run the headless daemon with continuous scheduling.
///
/// This wraps the headless orchestrator but switches the interpreter scheduler
/// to `Daemon` mode so it blocks on cron-scheduled workflows and fires them
/// when their scheduled time arrives.
fn run_headless_daemon(cwd: &Path, config: &HeadlessConfig) -> i32 {
    // For continuous daemon mode, we delegate to the headless module's
    // daemon-mode entry point. If the headless module does not yet expose
    // a separate daemon entry, we fall back to single-pass mode with a
    // note that full daemon mode requires the scheduler upgrade.
    //
    // The headless module's run_headless already supports the full
    // orchestration pipeline; the only difference is the scheduler mode.
    // We call run_headless_daemon_mode which uses SchedulerMode::Daemon.
    nightshift_core::headless::run_headless_daemon_mode(cwd, config)
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
    fn run_daemon_default_shows_doctor_when_not_initialised() {
        // When there is no .calypso/repository-state.json, the daemon default
        // should not panic or hang — it should fall back to doctor output.
        // We verify by checking that the function returns without error on
        // a non-git temp dir (doctor output is printed to stdout).
        let dir = temp_non_git_dir();
        // run_daemon_default prints to stdout; we just verify it does not panic.
        run_daemon_default(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
