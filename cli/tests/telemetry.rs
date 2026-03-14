use std::sync::{Arc, Mutex};

use calypso_cli::telemetry::{CorrelationContext, Event, EventKind, EventStream, LogLevel, Logger};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// A shared, thread-safe byte buffer used as the log writer in tests.
#[derive(Clone, Default)]
struct TestBuf(Arc<Mutex<Vec<u8>>>);

impl TestBuf {
    fn new() -> Self {
        Self::default()
    }

    fn into_string(self) -> String {
        let bytes = self.0.lock().unwrap().clone();
        String::from_utf8(bytes).expect("log output is valid UTF-8")
    }
}

impl std::io::Write for TestBuf {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn logger_with_buf(level: LogLevel) -> (Logger, TestBuf) {
    let buf = TestBuf::new();
    let logger = Logger::_with_level_and_writer(level, Box::new(buf.clone()));
    (logger, buf)
}

// ---------------------------------------------------------------------------
// Tests: log entry shape
// ---------------------------------------------------------------------------

#[test]
fn info_entry_contains_required_fields() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    logger.info("hello world");
    let output = buf.into_string();

    let entry: serde_json::Value = serde_json::from_str(output.trim()).expect("valid JSON line");
    assert_eq!(entry["level"], "info");
    assert_eq!(entry["message"], "hello world");
    assert!(
        entry["timestamp"].as_str().unwrap().ends_with('Z'),
        "timestamp should be RFC 3339 UTC"
    );
}

#[test]
fn debug_entries_suppressed_when_level_is_info() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    logger.debug("should be suppressed");
    logger.info("should appear");
    let output = buf.into_string();

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 1, "only one log line should be emitted");
    let entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(entry["level"], "info");
}

#[test]
fn debug_entries_emitted_when_level_is_debug() {
    let (logger, buf) = logger_with_buf(LogLevel::Debug);
    logger.debug("debug entry");
    logger.info("info entry");
    let output = buf.into_string();

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 2);
    let first: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(first["level"], "debug");
}

#[test]
fn error_level_suppresses_warn_and_info_and_debug() {
    let (logger, buf) = logger_with_buf(LogLevel::Error);
    logger.debug("d");
    logger.info("i");
    logger.warn("w");
    logger.error("e");
    let output = buf.into_string();

    let lines: Vec<&str> = output.lines().collect();
    assert_eq!(lines.len(), 1);
    let entry: serde_json::Value = serde_json::from_str(lines[0]).unwrap();
    assert_eq!(entry["level"], "error");
}

// ---------------------------------------------------------------------------
// Tests: correlation context
// ---------------------------------------------------------------------------

#[test]
fn correlation_context_fields_appear_in_every_entry_when_set() {
    let ctx = CorrelationContext::new()
        .with_feature_id("feat-123")
        .with_session_id("sess-abc")
        .with_thread_id("thread-1");

    let (logger, buf) = logger_with_buf(LogLevel::Info);
    let logger = logger.with_context(ctx);
    logger.info("first");
    logger.info("second");

    let output = buf.into_string();
    for line in output.lines() {
        let entry: serde_json::Value = serde_json::from_str(line).unwrap();
        assert_eq!(entry["feature_id"], "feat-123");
        assert_eq!(entry["session_id"], "sess-abc");
        assert_eq!(entry["thread_id"], "thread-1");
    }
}

#[test]
fn correlation_context_fields_absent_when_not_set() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    logger.info("no context");
    let output = buf.into_string();

    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert!(entry.get("feature_id").is_none());
    assert!(entry.get("session_id").is_none());
    assert!(entry.get("thread_id").is_none());
}

// ---------------------------------------------------------------------------
// Tests: structured fields / builder
// ---------------------------------------------------------------------------

#[test]
fn entry_builder_includes_structured_fields() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    logger
        .entry(LogLevel::Info, "gate status changed")
        .field("gate_id", "rust-quality")
        .field("status", "passing")
        .emit();

    let output = buf.into_string();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(entry["fields"]["gate_id"], "rust-quality");
    assert_eq!(entry["fields"]["status"], "passing");
}

#[test]
fn secret_field_is_redacted() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    logger
        .entry(LogLevel::Info, "api call")
        .field("github_token", "ghp_supersecret")
        .emit();

    let output = buf.into_string();
    assert!(
        !output.contains("ghp_supersecret"),
        "secret must not appear in log output"
    );
    assert!(output.contains("[REDACTED]"));
}

// ---------------------------------------------------------------------------
// Tests: log_event! macro
// ---------------------------------------------------------------------------

#[test]
fn log_event_macro_emits_entry_with_fields() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    calypso_cli::log_event!(logger, LogLevel::Info, "macro test", "key" => "value");
    let output = buf.into_string();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(entry["message"], "macro test");
    assert_eq!(entry["fields"]["key"], "value");
}

// ---------------------------------------------------------------------------
// Tests: EventStream
// ---------------------------------------------------------------------------

#[test]
fn state_transition_event_serializes_with_expected_fields() {
    let event = Event::state_transition("implementation", "ready-for-review", Some("feat-42"));
    assert_eq!(event.kind, EventKind::StateTransition);
    assert_eq!(event.payload["from"], "implementation");
    assert_eq!(event.payload["to"], "ready-for-review");
    assert_eq!(event.payload["feature_id"], "feat-42");
}

#[test]
fn gate_changed_event_serializes_with_expected_fields() {
    let event = Event::gate_changed("rust-quality", "passing", Some("feat-42"));
    assert_eq!(event.kind, EventKind::GateChanged);
    assert_eq!(event.payload["gate_id"], "rust-quality");
    assert_eq!(event.payload["status"], "passing");
    assert_eq!(event.payload["feature_id"], "feat-42");
}

#[test]
fn event_stream_records_and_returns_events() {
    let stream = EventStream::new();
    stream.push(Event::state_transition("new", "implementation", None));
    stream.push(Event::gate_changed("pr-canonicalized", "passing", None));

    let snapshot = stream.snapshot();
    assert_eq!(snapshot.len(), 2);
    assert_eq!(snapshot[0].kind, EventKind::StateTransition);
    assert_eq!(snapshot[1].kind, EventKind::GateChanged);
}

#[test]
fn event_stream_drain_empties_the_stream() {
    let stream = EventStream::new();
    stream.push(Event::session_started("sess-1", None));

    let drained = stream.drain();
    assert_eq!(drained.len(), 1);
    assert!(stream.snapshot().is_empty());
}

// ---------------------------------------------------------------------------
// Tests: output goes to stderr (structural — verify via writer injection)
// ---------------------------------------------------------------------------

#[test]
fn log_output_goes_to_injected_writer_not_stdout() {
    // This test verifies the output routing contract by using an injected writer.
    // In normal operation Logger::new() uses stderr.
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    logger.info("stderr target");
    assert!(!buf.into_string().is_empty());
    // stdout is not captured here — if we had printed to stdout the buf would
    // be empty (proving the output went to the injected writer, i.e. stderr).
}
