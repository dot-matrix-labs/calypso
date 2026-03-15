use std::fmt;
use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::state::{
    AgentSession, AgentSessionStatus, AgentTerminalOutcome, SessionOutput, SessionOutputStream,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexCommand {
    program: String,
    args: Vec<String>,
}

impl CodexCommand {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
        }
    }

    pub fn interactive(prompt: impl Into<String>, working_directory: impl Into<String>) -> Self {
        Self::new("codex")
            .arg("--no-alt-screen")
            .arg("-C")
            .arg(working_directory)
            .arg(prompt)
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn program(&self) -> &str {
        &self.program
    }

    pub fn args(&self) -> &[String] {
        &self.args
    }

    fn into_command(self) -> Command {
        let mut command = Command::new(self.program);
        command.args(self.args);
        command
    }
}

#[derive(Debug)]
pub struct CodexSession {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<RuntimeEvent>,
    snapshot: CodexSessionSnapshot,
    interrupted: bool,
}

impl CodexSession {
    pub fn spawn(role: &str, command: CodexCommand) -> Result<Self, CodexError> {
        let mut child = spawn_child(
            command
                .into_command()
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped()),
        )?;

        let (stdin, stdout, stderr) =
            take_session_pipes(child.stdin.take(), child.stdout.take(), child.stderr.take())
                .expect("piped codex child should provide stdin, stdout, and stderr handles");
        let (events_tx, events_rx) = mpsc::channel();

        spawn_reader(stdout, SessionOutputStream::Stdout, events_tx.clone());
        spawn_reader(stderr, SessionOutputStream::Stderr, events_tx);

        Ok(Self {
            child,
            stdin,
            events: events_rx,
            snapshot: CodexSessionSnapshot {
                role: role.to_string(),
                session_id: next_session_id(),
                provider_session_id: None,
                status: SessionStatus::Running,
                output: Vec::new(),
                terminal_outcome: None,
            },
            interrupted: false,
        })
    }

    pub fn poll_events(&mut self) -> Result<Vec<SessionOutput>, CodexError> {
        let mut drained = Vec::new();

        loop {
            match self.events.try_recv() {
                Ok(RuntimeEvent::Output(event)) => {
                    self.apply_output(&event);
                    drained.push(event);
                }
                Ok(RuntimeEvent::ReadFailure(message)) => {
                    return Err(CodexError::OutputRead(message));
                }
                Err(TryRecvError::Empty) => return Ok(drained),
                Err(TryRecvError::Disconnected) => return Ok(drained),
            }
        }
    }

    pub fn refresh_status(&mut self) -> Result<SessionStatus, CodexError> {
        self.poll_events()?;

        let wait_status = self
            .child
            .try_wait()
            .expect("codex child status should be queryable");
        if let Some(status) = wait_status {
            let terminal_outcome = terminal_outcome_from_exit(
                self.interrupted,
                self.snapshot.terminal_outcome,
                status,
            );

            self.snapshot.terminal_outcome = Some(terminal_outcome);
            self.snapshot.status = match terminal_outcome {
                TerminalOutcome::Ok => SessionStatus::Completed,
                TerminalOutcome::Nok => SessionStatus::Failed,
                TerminalOutcome::Aborted => SessionStatus::Aborted,
            };
        }

        Ok(self.snapshot.status)
    }

    pub fn send_follow_up(&mut self, input: &str) -> Result<(), CodexError> {
        if matches!(
            self.refresh_status()?,
            SessionStatus::Completed | SessionStatus::Failed | SessionStatus::Aborted
        ) {
            return Err(CodexError::Io(std::io::Error::from(
                std::io::ErrorKind::BrokenPipe,
            )));
        }

        write_follow_up_line(&mut self.stdin, input)?;

        if self.snapshot.status == SessionStatus::WaitingForHuman {
            self.snapshot.status = SessionStatus::Running;
        }

        Ok(())
    }

    pub fn interrupt(&mut self) -> Result<(), CodexError> {
        self.interrupted = true;
        self.child.kill().map_err(CodexError::Io)
    }

    pub fn snapshot(&self) -> CodexSessionSnapshot {
        self.snapshot.clone()
    }

    pub fn status(&self) -> SessionStatus {
        self.snapshot.status
    }

    pub fn terminal_outcome(&self) -> Option<TerminalOutcome> {
        self.snapshot.terminal_outcome
    }

    fn apply_output(&mut self, event: &SessionOutput) {
        if let Some(session_id) = extract_provider_session_id(event.text.as_str()) {
            self.snapshot.provider_session_id = Some(session_id);
        }

        if indicates_waiting_for_human(event.text.as_str()) {
            self.snapshot.status = SessionStatus::WaitingForHuman;
        }

        if let Some(outcome) = parse_terminal_outcome(event.text.as_str()) {
            self.snapshot.terminal_outcome = Some(outcome);
        }

        self.snapshot.output.push(event.clone());
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CodexSessionSnapshot {
    pub role: String,
    pub session_id: String,
    pub provider_session_id: Option<String>,
    pub status: SessionStatus,
    pub output: Vec<SessionOutput>,
    pub terminal_outcome: Option<TerminalOutcome>,
}

impl CodexSessionSnapshot {
    pub fn persisted(&self) -> AgentSession {
        AgentSession {
            role: self.role.clone(),
            session_id: self.session_id.clone(),
            provider_session_id: self.provider_session_id.clone(),
            status: self.status.into(),
            output: self.output.clone(),
            pending_follow_ups: Vec::new(),
            terminal_outcome: self.terminal_outcome.map(Into::into),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionStatus {
    Running,
    WaitingForHuman,
    Completed,
    Failed,
    Aborted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TerminalOutcome {
    Ok,
    Nok,
    Aborted,
}

impl From<SessionStatus> for AgentSessionStatus {
    fn from(value: SessionStatus) -> Self {
        match value {
            SessionStatus::Running => Self::Running,
            SessionStatus::WaitingForHuman => Self::WaitingForHuman,
            SessionStatus::Completed => Self::Completed,
            SessionStatus::Failed => Self::Failed,
            SessionStatus::Aborted => Self::Aborted,
        }
    }
}

impl From<TerminalOutcome> for AgentTerminalOutcome {
    fn from(value: TerminalOutcome) -> Self {
        match value {
            TerminalOutcome::Ok => Self::Ok,
            TerminalOutcome::Nok => Self::Nok,
            TerminalOutcome::Aborted => Self::Aborted,
        }
    }
}

#[derive(Debug)]
pub enum CodexError {
    Io(std::io::Error),
    MissingPipe(&'static str),
    OutputRead(String),
}

impl fmt::Display for CodexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CodexError::Io(error) => write!(f, "codex runtime I/O error: {error}"),
            CodexError::MissingPipe(pipe) => {
                write!(f, "codex runtime missing expected {pipe} pipe")
            }
            CodexError::OutputRead(message) => {
                write!(f, "codex runtime failed to read process output: {message}")
            }
        }
    }
}

impl std::error::Error for CodexError {}

#[derive(Debug, PartialEq, Eq)]
enum RuntimeEvent {
    Output(SessionOutput),
    ReadFailure(String),
}

fn spawn_reader<R>(stream: R, output_stream: SessionOutputStream, sender: Sender<RuntimeEvent>)
where
    R: std::io::Read + Send + 'static,
{
    thread::spawn(move || {
        let reader = BufReader::new(stream);

        for line in reader.lines() {
            match line {
                Ok(text) => {
                    if sender
                        .send(RuntimeEvent::Output(SessionOutput {
                            stream: output_stream,
                            text,
                        }))
                        .is_err()
                    {
                        break;
                    }
                }
                Err(error) => {
                    let _ = sender.send(RuntimeEvent::ReadFailure(error.to_string()));
                    break;
                }
            }
        }
    });
}

fn next_session_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();

    format!("codex-session-{timestamp}")
}

fn extract_provider_session_id(line: &str) -> Option<String> {
    let lower = line.to_ascii_lowercase();

    for marker in ["session id:", "session_id=", "session_id:", "session-id:"] {
        if let Some(index) = lower.find(marker) {
            let value = line[index + marker.len()..].trim();
            if !value.is_empty() {
                return Some(trim_wrapping_punctuation(value).to_string());
            }
        }
    }

    None
}

fn indicates_waiting_for_human(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("waiting for human input") || lower.contains("waiting for user input")
}

fn parse_terminal_outcome(line: &str) -> Option<TerminalOutcome> {
    match line.trim() {
        "OK" => Some(TerminalOutcome::Ok),
        "NOK" => Some(TerminalOutcome::Nok),
        "ABORTED" => Some(TerminalOutcome::Aborted),
        _ => None,
    }
}

fn trim_wrapping_punctuation(value: &str) -> &str {
    value.trim_matches(|character| matches!(character, '"' | '\'' | '[' | ']' | '(' | ')'))
}

fn take_pipe<T>(pipe: Option<T>, name: &'static str) -> Result<T, CodexError> {
    pipe.ok_or(CodexError::MissingPipe(name))
}

fn take_session_pipes(
    stdin: Option<ChildStdin>,
    stdout: Option<ChildStdout>,
    stderr: Option<ChildStderr>,
) -> Result<(ChildStdin, ChildStdout, ChildStderr), CodexError> {
    Ok((
        take_pipe(stdin, "stdin")?,
        take_pipe(stdout, "stdout")?,
        take_pipe(stderr, "stderr")?,
    ))
}

fn spawn_child(command: &mut Command) -> Result<Child, CodexError> {
    command.spawn().map_err(CodexError::Io)
}

fn terminal_outcome_from_exit(
    interrupted: bool,
    prior_outcome: Option<TerminalOutcome>,
    status: ExitStatus,
) -> TerminalOutcome {
    if interrupted {
        TerminalOutcome::Aborted
    } else if let Some(outcome) = prior_outcome {
        outcome
    } else if status.success() {
        TerminalOutcome::Ok
    } else if status.code() == Some(130) {
        TerminalOutcome::Aborted
    } else {
        TerminalOutcome::Nok
    }
}

fn write_follow_up_line<W: Write>(stdin: &mut W, input: &str) -> Result<(), CodexError> {
    stdin
        .write_all(input.as_bytes())
        .and_then(|_| {
            if input.ends_with('\n') {
                Ok(())
            } else {
                stdin.write_all(b"\n")
            }
        })
        .and_then(|_| stdin.flush())
        .map_err(CodexError::Io)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{self, Read};
    use std::sync::mpsc;
    use std::time::Duration;

    struct ErrorReader;

    impl Read for ErrorReader {
        fn read(&mut self, _buf: &mut [u8]) -> io::Result<usize> {
            Err(io::Error::new(io::ErrorKind::InvalidData, "broken stream"))
        }
    }

    #[derive(Default)]
    struct RecordingWriter {
        writes: Vec<u8>,
        flushes: usize,
    }

    impl Write for RecordingWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.writes.extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.flushes += 1;
            Ok(())
        }
    }

    struct FailingWriter;

    impl Write for FailingWriter {
        fn write(&mut self, _buf: &[u8]) -> io::Result<usize> {
            Err(io::Error::other("write failed"))
        }

        fn flush(&mut self) -> io::Result<()> {
            Ok(())
        }
    }

    #[test]
    fn spawn_reader_emits_read_failures_and_stops_when_receiver_is_gone() {
        let (sender, receiver) = mpsc::channel();
        spawn_reader(ErrorReader, SessionOutputStream::Stdout, sender);

        let event = receiver
            .recv_timeout(Duration::from_secs(1))
            .expect("reader should emit a read failure");
        assert_eq!(
            event,
            RuntimeEvent::ReadFailure("broken stream".to_string())
        );

        let (sender, receiver) = mpsc::channel();
        drop(receiver);
        spawn_reader(
            io::Cursor::new(b"line that nobody will receive\n".to_vec()),
            SessionOutputStream::Stdout,
            sender,
        );
    }

    #[test]
    fn poll_events_returns_output_read_errors() {
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("child should spawn");
        let stdin = child.stdin.take().expect("stdin should exist");
        let (events, receiver) = mpsc::channel();
        events
            .send(RuntimeEvent::ReadFailure("broken stream".to_string()))
            .expect("event should send");

        let mut session = CodexSession {
            child,
            stdin,
            events: receiver,
            snapshot: CodexSessionSnapshot {
                role: "engineer".to_string(),
                session_id: "codex-session-test".to_string(),
                provider_session_id: None,
                status: SessionStatus::Running,
                output: Vec::new(),
                terminal_outcome: None,
            },
            interrupted: false,
        };

        let error = session
            .poll_events()
            .expect_err("read failure event should become a codex error");
        assert_eq!(
            error.to_string(),
            "codex runtime failed to read process output: broken stream"
        );

        let _ = session.child.kill();
        let _ = session.child.wait();
    }

    #[test]
    fn provider_session_id_parser_handles_alternate_markers_and_empty_values() {
        assert_eq!(
            extract_provider_session_id("session-id: [session_123]").as_deref(),
            Some("session_123")
        );
        assert_eq!(extract_provider_session_id("session_id:"), None);
    }

    #[test]
    fn helper_functions_cover_pipe_lookup_wait_mapping_and_follow_up_writes() {
        assert_eq!(
            take_pipe(Some(7_u8), "stdin").expect("present pipe should succeed"),
            7
        );
        assert_eq!(
            take_pipe::<u8>(None, "stdout")
                .expect_err("missing pipe should fail")
                .to_string(),
            "codex runtime missing expected stdout pipe"
        );

        let mut missing_binary = Command::new("/definitely/missing-binary");
        let spawn_error = spawn_child(&mut missing_binary)
            .expect_err("missing binaries should map to codex errors");
        assert!(
            spawn_error
                .to_string()
                .starts_with("codex runtime I/O error:")
        );

        let mut writer = RecordingWriter::default();
        write_follow_up_line(&mut writer, "continue").expect("writer should succeed");
        assert_eq!(writer.writes, b"continue\n");
        assert_eq!(writer.flushes, 1);

        let mut writer = RecordingWriter::default();
        write_follow_up_line(&mut writer, "continue\n").expect("writer should succeed");
        assert_eq!(writer.writes, b"continue\n");
        assert_eq!(writer.flushes, 1);

        let error = write_follow_up_line(&mut FailingWriter, "continue")
            .expect_err("write failures should map to codex errors");
        assert_eq!(error.to_string(), "codex runtime I/O error: write failed");

        let mut writer = FailingWriter;
        writer.flush().expect("standalone flush should succeed");

        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("child should spawn");
        let stdin = child.stdin.take().expect("stdin should exist");
        let stdout = child.stdout.take().expect("stdout should exist");
        let stderr = child.stderr.take().expect("stderr should exist");
        let _ = take_session_pipes(Some(stdin), Some(stdout), Some(stderr))
            .expect("present pipes should succeed");
        let _ = child.kill();
        let _ = child.wait();
        assert_eq!(
            take_session_pipes(None, None, None)
                .expect_err("missing pipes should fail")
                .to_string(),
            "codex runtime missing expected stdin pipe"
        );

        let mut stdout_missing_child = Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("child should spawn");
        assert_eq!(
            take_session_pipes(
                stdout_missing_child.stdin.take(),
                None,
                stdout_missing_child.stderr.take(),
            )
            .expect_err("missing stdout should fail")
            .to_string(),
            "codex runtime missing expected stdout pipe"
        );
        let _ = stdout_missing_child.kill();
        let _ = stdout_missing_child.wait();

        let mut stderr_missing_child = Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("child should spawn");
        assert_eq!(
            take_session_pipes(
                stderr_missing_child.stdin.take(),
                stderr_missing_child.stdout.take(),
                None,
            )
            .expect_err("missing stderr should fail")
            .to_string(),
            "codex runtime missing expected stderr pipe"
        );
        let _ = stderr_missing_child.kill();
        let _ = stderr_missing_child.wait();
    }

    #[test]
    fn terminal_outcome_helpers_cover_process_and_marker_paths() {
        let ok_status = Command::new("/bin/sh")
            .arg("-c")
            .arg("exit 0")
            .status()
            .expect("status should succeed");
        assert_eq!(
            terminal_outcome_from_exit(false, None, ok_status),
            TerminalOutcome::Ok
        );

        let aborted_status = Command::new("/bin/sh")
            .arg("-c")
            .arg("exit 130")
            .status()
            .expect("status should succeed");
        assert_eq!(
            terminal_outcome_from_exit(false, None, aborted_status),
            TerminalOutcome::Aborted
        );

        let failed_status = Command::new("/bin/sh")
            .arg("-c")
            .arg("exit 9")
            .status()
            .expect("status should succeed");
        assert_eq!(
            terminal_outcome_from_exit(false, None, failed_status),
            TerminalOutcome::Nok
        );
        assert_eq!(
            terminal_outcome_from_exit(true, None, failed_status),
            TerminalOutcome::Aborted
        );
        assert_eq!(
            terminal_outcome_from_exit(false, Some(TerminalOutcome::Nok), ok_status),
            TerminalOutcome::Nok
        );

        assert_eq!(parse_terminal_outcome("OK"), Some(TerminalOutcome::Ok));
        assert_eq!(
            parse_terminal_outcome("ABORTED"),
            Some(TerminalOutcome::Aborted)
        );
    }

    #[test]
    fn refresh_status_propagates_polled_event_failures() {
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("sleep 1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("child should spawn");
        let stdin = child.stdin.take().expect("stdin should exist");
        let (events, receiver) = mpsc::channel();
        events
            .send(RuntimeEvent::ReadFailure("broken stream".to_string()))
            .expect("event should send");

        let mut session = CodexSession {
            child,
            stdin,
            events: receiver,
            snapshot: CodexSessionSnapshot {
                role: "engineer".to_string(),
                session_id: "codex-session-test".to_string(),
                provider_session_id: None,
                status: SessionStatus::Running,
                output: Vec::new(),
                terminal_outcome: None,
            },
            interrupted: false,
        };

        let error = session
            .refresh_status()
            .expect_err("refresh should propagate output read failures");
        assert_eq!(
            error.to_string(),
            "codex runtime failed to read process output: broken stream"
        );

        let _ = session.child.kill();
        let _ = session.child.wait();
    }
}
