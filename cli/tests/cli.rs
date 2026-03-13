use std::process::Command;

#[test]
fn version_flag_prints_required_build_metadata() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("--version")
        .output()
        .expect("failed to run calypso-cli --version");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("calypso-cli "));
    assert!(stdout.contains("Git hash: "));
    assert!(stdout.contains("Build time: "));
    assert!(stdout.contains("Git tags: "));
}

#[test]
fn help_flag_exposes_version_information() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("--help")
        .output()
        .expect("failed to run calypso-cli --help");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("calypso-cli"));
    assert!(stdout.contains("Version: "));
    assert!(stdout.contains("Git hash: "));
    assert!(stdout.contains("Usage:"));
}

#[test]
fn doctor_command_prints_local_prerequisite_checks() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("doctor")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run calypso-cli doctor");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Doctor checks"));
    assert!(stdout.contains("gh-installed"));
    assert!(stdout.contains("codex-installed"));
    assert!(stdout.contains("github-remote-configured"));
}

#[test]
fn status_command_prints_feature_gate_summary() {
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("status")
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("failed to run calypso-cli status");

    assert!(output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("stdout should be valid utf-8");
    assert!(stdout.contains("Feature status"));
    assert!(stdout.contains("Validation"));
    assert!(stdout.contains("Blocking gates"));
}
