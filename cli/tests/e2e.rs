use std::process::Command;

#[test]
fn running_without_arguments_exits_cleanly() {
    // With no .calypso/state.json in the test working directory, the binary
    // attempts to launch the doctor TUI. In a non-terminal environment the TUI
    // setup fails gracefully and the process exits 0.
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .output()
        .expect("failed to run calypso-cli");

    assert!(output.status.success());
}

#[test]
fn version_flag_prints_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("--version")
        .output()
        .expect("failed to run calypso-cli");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("0.1.0"));
}

#[test]
fn help_flag_prints_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("help")
        .output()
        .expect("failed to run calypso-cli");

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("Commands:"));
}
