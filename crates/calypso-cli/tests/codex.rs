// FUTURE: #48 — Codex provider tests; re-enable when multi-vendor registry is implemented (#48)
#![cfg(any())]

use std::thread;
use std::time::{Duration, Instant};

use calypso_cli::codex::{CodexCommand, CodexSession, SessionStatus, TerminalOutcome};
use calypso_cli::state::{AgentSessionStatus, SessionOutputStream};

fn shell_command(script: &str) -> CodexCommand {
    CodexCommand::new("/bin/sh").arg("-c").arg(script)
}

fn wait_until<F>(timeout: Duration, mut condition: F)
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + timeout;

    while Instant::now() < deadline {
        if condition() {
            return;
        }

        thread::sleep(Duration::from_millis(20));
    }

    panic!("condition not satisfied before timeout");
}

#[test]
fn session_streams_output_and_extracts_provider_session_id() {
    let mut session = CodexSession::spawn(
        "engineer",
        shell_command("printf 'Session ID: session_42\\n'; printf 'streamed chunk\\n'; sleep 0.1"),
    )
    .expect("session should spawn");

    wait_until(Duration::from_secs(2), || {
        !session
            .poll_events()
            .expect("poll should succeed")
            .is_empty()
    });

    let snapshot = session.snapshot();
    assert_eq!(snapshot.provider_session_id.as_deref(), Some("session_42"));
    assert_eq!(snapshot.output.len(), 2);
    assert_eq!(snapshot.output[0].stream, SessionOutputStream::Stdout);
    assert_eq!(snapshot.output[0].text, "Session ID: session_42");
    assert_eq!(snapshot.output[1].text, "streamed chunk");

    wait_until(Duration::from_secs(2), || {
        session.refresh_status().expect("refresh should succeed");
        snapshot_is_terminal(session.status())
    });

    assert_eq!(session.status(), SessionStatus::Completed);
    assert_eq!(session.terminal_outcome(), Some(TerminalOutcome::Ok));
}

#[test]
fn session_routes_follow_up_input_into_running_process() {
    let mut session = CodexSession::spawn(
        "engineer",
        shell_command("read line; printf 'echo:%s\\n' \"$line\""),
    )
    .expect("session should spawn");

    session
        .send_follow_up("continue please")
        .expect("follow-up should be accepted");

    wait_until(Duration::from_secs(2), || {
        let events = session.poll_events().expect("poll should succeed");
        events
            .iter()
            .any(|event| event.text == "echo:continue please")
    });

    let snapshot = session.snapshot();
    assert_eq!(snapshot.output.len(), 1);
    assert_eq!(snapshot.output[0].text, "echo:continue please");
}

#[test]
fn session_normalizes_failed_and_aborted_terminal_states() {
    let mut failed = CodexSession::spawn("engineer", shell_command("printf 'NOK\\n'; exit 7"))
        .expect("failed session should spawn");

    wait_until(Duration::from_secs(2), || {
        failed.refresh_status().expect("refresh should succeed");
        snapshot_is_terminal(failed.status())
    });

    assert_eq!(failed.status(), SessionStatus::Failed);
    assert_eq!(failed.terminal_outcome(), Some(TerminalOutcome::Nok));
    assert_eq!(
        failed.snapshot().persisted().status,
        AgentSessionStatus::Failed
    );

    let mut aborted = CodexSession::spawn(
        "engineer",
        shell_command("trap 'exit 130' TERM; while true; do sleep 1; done"),
    )
    .expect("aborted session should spawn");

    aborted.interrupt().expect("interrupt should succeed");

    wait_until(Duration::from_secs(2), || {
        aborted.refresh_status().expect("refresh should succeed");
        snapshot_is_terminal(aborted.status())
    });

    assert_eq!(aborted.status(), SessionStatus::Aborted);
    assert_eq!(aborted.terminal_outcome(), Some(TerminalOutcome::Aborted));

    let mut signaled_abort =
        CodexSession::spawn("engineer", shell_command("printf 'partial\\n'; exit 130"))
            .expect("signaled abort session should spawn");

    wait_until(Duration::from_secs(2), || {
        signaled_abort
            .refresh_status()
            .expect("refresh should succeed");
        snapshot_is_terminal(signaled_abort.status())
    });

    assert_eq!(signaled_abort.status(), SessionStatus::Aborted);
    assert_eq!(
        signaled_abort.terminal_outcome(),
        Some(TerminalOutcome::Aborted)
    );

    let mut raw_failed = CodexSession::spawn("engineer", shell_command("exit 9"))
        .expect("raw failure session should spawn");

    wait_until(Duration::from_secs(2), || {
        raw_failed.refresh_status().expect("refresh should succeed");
        snapshot_is_terminal(raw_failed.status())
    });

    assert_eq!(raw_failed.status(), SessionStatus::Failed);
    assert_eq!(raw_failed.terminal_outcome(), Some(TerminalOutcome::Nok));
}

#[test]
fn command_builder_preserves_program_and_arguments() {
    let command = CodexCommand::new("codex")
        .arg("exec")
        .arg("--json")
        .arg("ship it");

    assert_eq!(command.program(), "codex");
    assert_eq!(
        command.args(),
        &[
            "exec".to_string(),
            "--json".to_string(),
            "ship it".to_string()
        ]
    );
}

#[test]
fn interactive_command_targets_codex_cli_in_a_specific_worktree() {
    let command = CodexCommand::interactive("ship it", "/tmp/calypso-worktree");

    assert_eq!(command.program(), "codex");
    assert_eq!(
        command.args(),
        &[
            "--no-alt-screen".to_string(),
            "-C".to_string(),
            "/tmp/calypso-worktree".to_string(),
            "ship it".to_string()
        ]
    );
}

#[test]
fn session_spawn_reports_process_launch_failures() {
    let error = CodexSession::spawn("engineer", CodexCommand::new("/definitely/missing-binary"))
        .expect_err("missing binaries should fail to spawn");

    assert!(error.to_string().starts_with("codex runtime I/O error:"));
}

#[test]
fn session_snapshot_persists_runtime_details() {
    let mut session = CodexSession::spawn(
        "reviewer",
        shell_command("printf 'session_id=session_99\\n'; printf 'waiting for human input\\n';"),
    )
    .expect("session should spawn");

    wait_until(Duration::from_secs(2), || {
        !session
            .poll_events()
            .expect("poll should succeed")
            .is_empty()
    });

    let persisted = session.snapshot().persisted();
    assert_eq!(persisted.role, "reviewer");
    assert_eq!(persisted.provider_session_id.as_deref(), Some("session_99"));
    assert_eq!(persisted.status, AgentSessionStatus::WaitingForHuman);
    assert_eq!(persisted.output.len(), 2);
    assert_eq!(persisted.output[1].stream, SessionOutputStream::Stdout);
}

#[test]
fn session_follow_up_with_newline_resumes_waiting_session() {
    let mut session = CodexSession::spawn(
        "reviewer",
        shell_command(
            "printf 'waiting for human input\\n'; read line; printf 'echo:%s\\n' \"$line\"",
        ),
    )
    .expect("session should spawn");

    wait_until(Duration::from_secs(2), || {
        !session
            .poll_events()
            .expect("poll should succeed")
            .is_empty()
    });

    assert_eq!(session.status(), SessionStatus::WaitingForHuman);

    session
        .send_follow_up("continue now\n")
        .expect("follow-up should be accepted");

    assert_eq!(session.status(), SessionStatus::Running);

    wait_until(Duration::from_secs(2), || {
        let events = session.poll_events().expect("poll should succeed");
        events.iter().any(|event| event.text == "echo:continue now")
    });
}

#[test]
fn session_follow_up_reports_write_failures_after_process_exit() {
    let mut session =
        CodexSession::spawn("reviewer", shell_command("exit 0")).expect("session should spawn");

    wait_until(Duration::from_secs(2), || {
        session.refresh_status().expect("refresh should succeed");
        snapshot_is_terminal(session.status())
    });

    let error = session
        .send_follow_up("continue now")
        .expect_err("writing to an exited process should fail");
    assert!(error.to_string().starts_with("codex runtime I/O error:"));
}

fn snapshot_is_terminal(status: SessionStatus) -> bool {
    matches!(
        status,
        SessionStatus::Completed | SessionStatus::Failed | SessionStatus::Aborted
    )
}
