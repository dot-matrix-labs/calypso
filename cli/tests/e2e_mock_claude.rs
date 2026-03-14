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

// ── Orchestrator-level scenarios ───────────────────────────────────────────────
//
// Each test below spawns `calypso` as a real child process and asserts on BOTH
// the process exit code AND the resulting state file on disk.  The fake `claude`
// binary is installed on PATH so that the full decision loop is exercised
// without a live Anthropic API key.

/// Scenario 1 — OK: calypso receives `[CALYPSO:OK]` → state file advances to
/// the next workflow state.
///
/// Starting from `implementation`, the forward state is `qa-validation`.
#[test]
fn orchestrator_ok_advances_workflow_state() {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let fake = FakeClaude::builder()
        .outcome(FakeOutcome::Ok {
            summary: "implementation complete".to_string(),
        })
        .install();

    // Start in `implementation` state.
    let state_json = r#"{
  "version": 1,
  "repo_id": "test-repo",
  "schema_version": 1,
  "current_feature": {
    "feature_id": "feat-orch-ok-001",
    "branch": "feat/orch-ok-001",
    "worktree_path": "/tmp",
    "pull_request": {
      "number": 10,
      "url": "https://github.com/example/repo/pull/10"
    },
    "workflow_state": "implementation",
    "gate_groups": [],
    "active_sessions": []
  }
}"#;

    let output = spawned_calypso()
        .prepend_path(fake.dir.clone())
        .args(["run", "feat-orch-ok-001", "--role", "implementer"])
        .state_file_json(state_json)
        .run();

    assert_eq!(
        output.exit_code, 0,
        "calypso run OK should exit 0\nstdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );

    let state_on_disk = output
        .read_state_json()
        .expect("state file should exist after OK run");

    assert!(
        state_on_disk.contains("qa-validation"),
        "state file should advance to qa-validation after OK, got: {state_on_disk}"
    );
    assert!(
        !state_on_disk.contains(r#""workflow_state": "implementation""#),
        "state file should no longer be in implementation state, got: {state_on_disk}"
    );
}

/// Scenario 2 — NOK: calypso receives `[CALYPSO:NOK]` → state file stays in
/// current state and the process exits non-zero.
#[test]
fn orchestrator_nok_preserves_workflow_state() {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let fake = FakeClaude::builder()
        .outcome(FakeOutcome::Nok {
            summary: "tests still red".to_string(),
            reason: "unit tests are failing".to_string(),
        })
        .install();

    let state_json = r#"{
  "version": 1,
  "repo_id": "test-repo",
  "schema_version": 1,
  "current_feature": {
    "feature_id": "feat-orch-nok-001",
    "branch": "feat/orch-nok-001",
    "worktree_path": "/tmp",
    "pull_request": {
      "number": 11,
      "url": "https://github.com/example/repo/pull/11"
    },
    "workflow_state": "implementation",
    "gate_groups": [],
    "active_sessions": []
  }
}"#;

    let output = spawned_calypso()
        .prepend_path(fake.dir.clone())
        .args(["run", "feat-orch-nok-001", "--role", "implementer"])
        .state_file_json(state_json)
        .run();

    assert_ne!(
        output.exit_code, 0,
        "calypso run NOK should exit non-zero\nstdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );

    assert!(
        output.stdout.contains("Outcome: NOK") || output.stderr.contains("NOK"),
        "NOK error should be surfaced in stdout or stderr, got stdout: {:?} stderr: {:?}",
        output.stdout,
        output.stderr
    );

    let state_on_disk = output
        .read_state_json()
        .expect("state file should still exist after NOK run");

    assert!(
        state_on_disk.contains("implementation"),
        "state file should remain in implementation after NOK, got: {state_on_disk}"
    );
    assert!(
        !state_on_disk.contains("qa-validation"),
        "state file must not advance on NOK, got: {state_on_disk}"
    );
}

/// Scenario 3 — CLARIFICATION: calypso receives `[CALYPSO:CLARIFICATION]` →
/// calypso surfaces the question (non-interactive mode exits with code 2).
#[test]
fn orchestrator_clarification_surfaces_operator_question() {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let fake = FakeClaude::builder()
        .outcome(FakeOutcome::Clarification {
            question: "Which ticket number should I use?".to_string(),
        })
        .install();

    let state_json = r#"{
  "version": 1,
  "repo_id": "test-repo",
  "schema_version": 1,
  "current_feature": {
    "feature_id": "feat-orch-clarify-001",
    "branch": "feat/orch-clarify-001",
    "worktree_path": "/tmp",
    "pull_request": {
      "number": 12,
      "url": "https://github.com/example/repo/pull/12"
    },
    "workflow_state": "implementation",
    "gate_groups": [],
    "active_sessions": []
  }
}"#;

    let output = spawned_calypso()
        .prepend_path(fake.dir.clone())
        .args(["run", "feat-orch-clarify-001", "--role", "implementer"])
        .state_file_json(state_json)
        .run();

    // In non-interactive mode calypso exits 2 when a clarification is needed.
    assert_eq!(
        output.exit_code, 2,
        "calypso run CLARIFICATION should exit 2\nstdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );

    // The question must appear somewhere in the process output.
    let combined = format!("{}{}", output.stdout, output.stderr);
    assert!(
        combined.contains("Which ticket number"),
        "clarification question should be surfaced in output, got stdout: {:?} stderr: {:?}",
        output.stdout,
        output.stderr
    );

    // State file must not be modified — workflow stays in implementation.
    let state_on_disk = output
        .read_state_json()
        .expect("state file should still exist after CLARIFICATION");

    assert!(
        state_on_disk.contains("implementation"),
        "state file should remain in implementation after CLARIFICATION, got: {state_on_disk}"
    );
}

/// Scenario 4 — ABORTED: calypso receives `[CALYPSO:ABORTED]` → state file
/// transitions to `aborted` and the process exits non-zero.
#[test]
fn orchestrator_aborted_transitions_to_aborted_state() {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let fake = FakeClaude::builder()
        .outcome(FakeOutcome::Aborted {
            reason: "operator cancelled the session".to_string(),
        })
        .install();

    let state_json = r#"{
  "version": 1,
  "repo_id": "test-repo",
  "schema_version": 1,
  "current_feature": {
    "feature_id": "feat-orch-abort-001",
    "branch": "feat/orch-abort-001",
    "worktree_path": "/tmp",
    "pull_request": {
      "number": 13,
      "url": "https://github.com/example/repo/pull/13"
    },
    "workflow_state": "implementation",
    "gate_groups": [],
    "active_sessions": []
  }
}"#;

    let output = spawned_calypso()
        .prepend_path(fake.dir.clone())
        .args(["run", "feat-orch-abort-001", "--role", "implementer"])
        .state_file_json(state_json)
        .run();

    assert_ne!(
        output.exit_code, 0,
        "calypso run ABORTED should exit non-zero\nstdout: {}\nstderr: {}",
        output.stdout, output.stderr
    );

    assert!(
        output.stdout.contains("Outcome: ABORTED"),
        "ABORTED outcome should appear in stdout, got: {:?}",
        output.stdout
    );

    let state_on_disk = output
        .read_state_json()
        .expect("state file should exist after ABORTED run");

    assert!(
        state_on_disk.contains("aborted"),
        "state file should transition to aborted state, got: {state_on_disk}"
    );
}
