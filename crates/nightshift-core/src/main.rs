//! Headless-only entry point for the nightshift engine.

use nightshift_core::headless::HeadlessConfig;
use nightshift_core::telemetry::{LogFormat, LogLevel};

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let cwd = if args.len() > 1 {
        std::path::PathBuf::from(&args[1])
    } else {
        std::env::current_dir().expect("current directory should be accessible")
    };
    let config = HeadlessConfig {
        verbosity: LogLevel::Debug,
        log_format: LogFormat::Text,
        env_log_override: None,
    };
    let exit = nightshift_core::headless::run_headless(&cwd, &config);
    std::process::exit(exit);
}
