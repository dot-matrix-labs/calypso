mod helpers;

use std::sync::{Mutex, OnceLock};

use helpers::fake_claude::{FakeClaude, FakeOutcome};
use helpers::spawned_calypso::spawned_calypso;

// Serialise PATH mutations: fake_claude installs itself into PATH and
// SpawnedCalypso reads PATH at spawn time; keeping this mutex prevents races
// when the test binary runs tests in parallel.
static PATH_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

fn path_mutex() -> &'static Mutex<()> {
    PATH_MUTEX.get_or_init(|| Mutex::new(()))
}

// ── Minimal valid state JSON ───────────────────────────────────────────────────

fn minimal_state_json(worktree_path: &str) -> String {
    format!(
        r#"{{
  "version": 1,
  "repo_id": "test-repo",
  "schema_version": 1,
  "current_feature": {{
    "feature_id": "feat-e2e-001",
    "branch": "feat/e2e-001",
    "worktree_path": "{worktree_path}",
    "pull_request": {{
      "number": 42,
      "url": "https://github.com/example/repo/pull/42"
    }},
    "workflow_state": "implementation",
    "gate_groups": [],
    "active_sessions": []
  }}
}}"#
    )
}

// ── Tests ──────────────────────────────────────────────────────────────────────

/// Verifies that the fake `claude` binary actually emits the configured marker
/// when invoked as a raw subprocess (PATH is prepended, binary is executable).
#[test]
fn fake_claude_emits_ok_marker_when_invoked_directly() {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let fake = FakeClaude::builder()
        .outcome(FakeOutcome::Ok {
            summary: "direct invocation ok".to_string(),
        })
        .install();

    let output = std::process::Command::new(&fake.binary_path)
        .arg("some prompt")
        .output()
        .expect("fake claude should execute");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(
        output.status.success(),
        "fake claude should exit 0, got: {:?}",
        output.status
    );
    assert!(
        stdout.contains("[CALYPSO:OK]"),
        "stdout should contain OK marker, got: {stdout:?}"
    );
    assert!(
        stdout.contains("direct invocation ok"),
        "stdout should contain the summary, got: {stdout:?}"
    );
}

#[test]
fn fake_claude_emits_nok_marker_when_configured() {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let fake = FakeClaude::builder()
        .outcome(FakeOutcome::Nok {
            summary: "something broke".to_string(),
            reason: "tests are red".to_string(),
        })
        .install();

    let output = std::process::Command::new(&fake.binary_path)
        .output()
        .expect("fake claude should execute");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("[CALYPSO:NOK]"), "got: {stdout:?}");
    assert!(stdout.contains("tests are red"), "got: {stdout:?}");
}

#[test]
fn fake_claude_emits_aborted_marker_when_configured() {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let fake = FakeClaude::builder()
        .outcome(FakeOutcome::Aborted {
            reason: "operator cancelled".to_string(),
        })
        .install();

    let output = std::process::Command::new(&fake.binary_path)
        .output()
        .expect("fake claude should execute");

    let stdout = String::from_utf8(output.stdout).expect("stdout should be UTF-8");
    assert!(stdout.contains("[CALYPSO:ABORTED]"), "got: {stdout:?}");
    assert!(stdout.contains("operator cancelled"), "got: {stdout:?}");
}

#[test]
fn fake_claude_respects_custom_exit_code() {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let fake = FakeClaude::builder()
        .outcome(FakeOutcome::Ok {
            summary: "exit code test".to_string(),
        })
        .exit_code(2)
        .install();

    let status = std::process::Command::new(&fake.binary_path)
        .status()
        .expect("fake claude should execute");

    assert_eq!(
        status.code(),
        Some(2),
        "exit code should be 2, got: {status:?}"
    );
}

/// Full end-to-end test: spawns `calypso-cli doctor` as a child process with a
/// temp working directory.  Verifies the full subprocess boundary (binary
/// resolution, exit code, stdout) is exercised without a live API key.
#[test]
fn spawned_calypso_doctor_exits_successfully() {
    let output = spawned_calypso().args(["doctor"]).run();

    // doctor exits 0 and produces some output
    assert_eq!(
        output.exit_code, 0,
        "calypso doctor should exit 0, stderr: {}",
        output.stderr
    );
    assert!(
        !output.stdout.is_empty(),
        "doctor should produce some output"
    );
}

/// Full e2e test: `calypso run` with a fake `claude` on PATH.
///
/// Spawns `calypso-cli run <feature-id> --role implementer` with the fake
/// binary prepended to PATH and a valid state file in `.calypso/`.  The
/// `run` subcommand picks up the fake `claude`, which emits an OK outcome.
/// We assert the process exits 0 and stdout contains "Outcome: OK".
#[test]
fn spawned_calypso_run_with_fake_claude_ok() {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let fake = FakeClaude::builder()
        .outcome(FakeOutcome::Ok {
            summary: "scaffold complete".to_string(),
        })
        .install();

    // Build a minimal state JSON whose worktree_path is irrelevant to `run`
    // (main.rs only reads workflow_state and feature_id from it).
    let state_json = minimal_state_json("/tmp");

    let output = spawned_calypso()
        .prepend_path(fake.dir.clone())
        .args(["run", "feat-e2e-001", "--role", "implementer"])
        .state_file_json(state_json)
        .run();

    assert_eq!(
        output.exit_code, 0,
        "calypso run should exit 0\nstdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );
    assert!(
        output.stdout.contains("Outcome: OK"),
        "stdout should contain 'Outcome: OK', got: {:?}",
        output.stdout
    );
    assert!(
        output.stdout.contains("scaffold complete"),
        "stdout should contain the summary, got: {:?}",
        output.stdout
    );
}
