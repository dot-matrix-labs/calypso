use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

fn unique_temp_dir(label: &str) -> std::path::PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("calypso-cli-{label}-{nanos}"));
    std::fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

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
    assert!(stdout.contains("required-workflows-present"));
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

#[test]
fn status_command_reports_errors_outside_git_repository() {
    let temp_dir = unique_temp_dir("status-no-git");
    let output = Command::new(env!("CARGO_BIN_EXE_calypso-cli"))
        .arg("status")
        .current_dir(&temp_dir)
        .output()
        .expect("failed to run calypso-cli status");

    assert!(!output.status.success());

    let stderr = String::from_utf8(output.stderr).expect("stderr should be valid utf-8");
    assert!(stderr.contains("status error: not inside a git repository"));

    std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
}
