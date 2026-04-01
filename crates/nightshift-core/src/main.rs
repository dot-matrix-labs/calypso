//! Orchestrator entry point for the nightshift engine.

use nightshift_core::interpreter_scheduler::SchedulerMode;
use nightshift_core::orchestrator::OrchestratorConfig;
use nightshift_core::telemetry::{LogFormat, LogLevel};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cwd = if args.len() > 1 {
        std::path::PathBuf::from(&args[1])
    } else {
        std::env::current_dir().expect("current directory should be accessible")
    };
    let config = OrchestratorConfig {
        verbosity: LogLevel::Debug,
        log_format: LogFormat::Text,
        env_log_override: None,
    };
    let exit = nightshift_core::orchestrator::run_orchestrator(&cwd, &config, SchedulerMode::SinglePass);
    std::process::exit(exit);
}
