use std::process::Command;

#[test]
fn running_without_arguments_shows_help_output() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .output()
        .expect("failed to run calypso-cli");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Usage:"));
    assert!(stdout.contains("Commands:"));
}
