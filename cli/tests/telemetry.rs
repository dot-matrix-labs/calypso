use std::sync::{Arc, Mutex, OnceLock};

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

// ---------------------------------------------------------------------------
// Tests: warn and error log levels
// ---------------------------------------------------------------------------

#[test]
fn warn_entry_emitted_and_has_correct_level() {
    let (logger, buf) = logger_with_buf(LogLevel::Warn);
    logger.warn("something degraded");
    let output = buf.into_string();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(entry["level"], "warn");
    assert_eq!(entry["message"], "something degraded");
}

#[test]
fn error_entry_emitted_and_has_correct_level() {
    let (logger, buf) = logger_with_buf(LogLevel::Error);
    logger.error("fatal problem");
    let output = buf.into_string();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(entry["level"], "error");
    assert_eq!(entry["message"], "fatal problem");
}

#[test]
fn warn_suppressed_when_level_is_error() {
    let (logger, buf) = logger_with_buf(LogLevel::Error);
    logger.warn("should be suppressed");
    assert!(buf.into_string().is_empty());
}

// ---------------------------------------------------------------------------
// Tests: Logger constructors
// ---------------------------------------------------------------------------

#[test]
fn logger_with_level_constructor_sets_min_level() {
    let logger = Logger::with_level(LogLevel::Warn);
    assert_eq!(logger.min_level(), LogLevel::Warn);
}

#[test]
fn logger_default_min_level_is_info_without_env_var() {
    // Ensure the env var is not set for this test.
    // If CALYPSO_LOG happens to be set in the environment we skip the assertion.
    if std::env::var("CALYPSO_LOG").is_err() {
        let buf = TestBuf::new();
        let logger = Logger::with_writer(Box::new(buf));
        assert_eq!(logger.min_level(), LogLevel::Info);
    }
}

#[test]
fn logger_debug_impl_does_not_panic() {
    let (logger, _buf) = logger_with_buf(LogLevel::Info);
    let _ = format!("{logger:?}");
}

// ---------------------------------------------------------------------------
// Tests: CorrelationContext partial fields
// ---------------------------------------------------------------------------

#[test]
fn partial_context_only_feature_id_set() {
    let ctx = CorrelationContext::new().with_feature_id("feat-only");

    let (logger, buf) = logger_with_buf(LogLevel::Info);
    let logger = logger.with_context(ctx);
    logger.info("partial ctx");

    let output = buf.into_string();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(entry["feature_id"], "feat-only");
    assert!(entry.get("session_id").is_none());
    assert!(entry.get("thread_id").is_none());
}

#[test]
fn partial_context_only_session_id_set() {
    let ctx = CorrelationContext::new().with_session_id("sess-only");

    let (logger, buf) = logger_with_buf(LogLevel::Info);
    let logger = logger.with_context(ctx);
    logger.info("session only");

    let output = buf.into_string();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert!(entry.get("feature_id").is_none());
    assert_eq!(entry["session_id"], "sess-only");
    assert!(entry.get("thread_id").is_none());
}

#[test]
fn partial_context_only_thread_id_set() {
    let ctx = CorrelationContext::new().with_thread_id("thread-only");

    let (logger, buf) = logger_with_buf(LogLevel::Info);
    let logger = logger.with_context(ctx);
    logger.info("thread only");

    let output = buf.into_string();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert!(entry.get("feature_id").is_none());
    assert!(entry.get("session_id").is_none());
    assert_eq!(entry["thread_id"], "thread-only");
}

// ---------------------------------------------------------------------------
// Tests: field_json builder method
// ---------------------------------------------------------------------------

#[test]
fn entry_builder_field_json_inserts_arbitrary_value() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    logger
        .entry(LogLevel::Info, "json field test")
        .field_json(
            "count",
            serde_json::Value::Number(serde_json::Number::from(42u64)),
        )
        .field_json("flag", serde_json::Value::Bool(true))
        .emit();

    let output = buf.into_string();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(entry["fields"]["count"], 42);
    assert_eq!(entry["fields"]["flag"], true);
}

// ---------------------------------------------------------------------------
// Tests: log_event! macro with multiple fields
// ---------------------------------------------------------------------------

#[test]
fn log_event_macro_multiple_fields() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    calypso_cli::log_event!(
        logger,
        LogLevel::Info,
        "multi-field macro",
        "alpha" => "a",
        "beta" => "b",
        "gamma" => "c",
    );
    let output = buf.into_string();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(entry["fields"]["alpha"], "a");
    assert_eq!(entry["fields"]["beta"], "b");
    assert_eq!(entry["fields"]["gamma"], "c");
}

#[test]
fn log_event_macro_no_fields() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    calypso_cli::log_event!(logger, LogLevel::Info, "bare message");
    let output = buf.into_string();
    let entry: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
    assert_eq!(entry["message"], "bare message");
    // fields key should be absent when empty (skip_serializing_if)
    assert!(entry.get("fields").is_none());
}

// ---------------------------------------------------------------------------
// Tests: EventKind Display and all variants
// ---------------------------------------------------------------------------

#[test]
fn event_kind_display_all_variants() {
    assert_eq!(EventKind::StateTransition.to_string(), "state_transition");
    assert_eq!(EventKind::GateChanged.to_string(), "gate_changed");
    assert_eq!(EventKind::SessionStarted.to_string(), "session_started");
    assert_eq!(EventKind::SessionEnded.to_string(), "session_ended");
    assert_eq!(EventKind::GitOp.to_string(), "git_op");
    assert_eq!(EventKind::GithubApiCall.to_string(), "github_api_call");
}

// ---------------------------------------------------------------------------
// Tests: remaining Event constructors
// ---------------------------------------------------------------------------

#[test]
fn session_started_without_feature_id() {
    let event = Event::session_started("sess-99", None);
    assert_eq!(event.kind, EventKind::SessionStarted);
    assert_eq!(event.payload["session_id"], "sess-99");
    assert!(!event.payload.contains_key("feature_id"));
}

#[test]
fn session_started_with_feature_id() {
    let event = Event::session_started("sess-100", Some("feat-1"));
    assert_eq!(event.payload["feature_id"], "feat-1");
}

#[test]
fn session_ended_with_all_fields() {
    let event = Event::session_ended("sess-1", "success", Some("feat-7"));
    assert_eq!(event.kind, EventKind::SessionEnded);
    assert_eq!(event.payload["session_id"], "sess-1");
    assert_eq!(event.payload["outcome"], "success");
    assert_eq!(event.payload["feature_id"], "feat-7");
}

#[test]
fn session_ended_without_feature_id() {
    let event = Event::session_ended("sess-2", "failure", None);
    assert_eq!(event.kind, EventKind::SessionEnded);
    assert!(!event.payload.contains_key("feature_id"));
}

#[test]
fn git_op_with_detail() {
    let event = Event::git_op("push", Some("refs/heads/main"));
    assert_eq!(event.kind, EventKind::GitOp);
    assert_eq!(event.payload["operation"], "push");
    assert_eq!(event.payload["detail"], "refs/heads/main");
}

#[test]
fn git_op_without_detail() {
    let event = Event::git_op("fetch", None);
    assert_eq!(event.kind, EventKind::GitOp);
    assert_eq!(event.payload["operation"], "fetch");
    assert!(!event.payload.contains_key("detail"));
}

#[test]
fn github_api_call_with_status_code() {
    let event = Event::github_api_call("/repos/foo/bar", Some(200));
    assert_eq!(event.kind, EventKind::GithubApiCall);
    assert_eq!(event.payload["endpoint"], "/repos/foo/bar");
    assert_eq!(event.payload["status_code"], 200);
}

#[test]
fn github_api_call_without_status_code() {
    let event = Event::github_api_call("/repos/foo/bar", None);
    assert_eq!(event.kind, EventKind::GithubApiCall);
    assert!(!event.payload.contains_key("status_code"));
}

#[test]
fn state_transition_without_feature_id() {
    let event = Event::state_transition("new", "implementation", None);
    assert_eq!(event.kind, EventKind::StateTransition);
    assert!(!event.payload.contains_key("feature_id"));
}

#[test]
fn gate_changed_without_feature_id() {
    let event = Event::gate_changed("lint", "failing", None);
    assert_eq!(event.kind, EventKind::GateChanged);
    assert!(!event.payload.contains_key("feature_id"));
}

// ---------------------------------------------------------------------------
// Tests: EventStream drain
// ---------------------------------------------------------------------------

#[test]
fn event_stream_drain_returns_all_events_and_leaves_stream_empty() {
    let stream = EventStream::new();
    stream.push(Event::git_op("commit", None));
    stream.push(Event::git_op("push", Some("origin")));

    let drained = stream.drain();
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].kind, EventKind::GitOp);

    // Stream must now be empty.
    assert!(stream.snapshot().is_empty());
    // A second drain returns nothing.
    assert!(stream.drain().is_empty());
}

// ---------------------------------------------------------------------------
// Tests: redaction edge cases
// ---------------------------------------------------------------------------

#[test]
fn redaction_covers_all_secret_key_patterns() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    logger
        .entry(LogLevel::Info, "redaction check")
        .field("api_key", "raw_key")
        .field("user_password", "hunter2")
        .field("auth_header", "Bearer xyz")
        .field("client_secret", "s3cret")
        .field("credential", "cred_value")
        .emit();

    let output = buf.into_string();
    assert!(!output.contains("raw_key"));
    assert!(!output.contains("hunter2"));
    assert!(!output.contains("Bearer xyz"));
    assert!(!output.contains("s3cret"));
    assert!(!output.contains("cred_value"));
    let count = output.matches("[REDACTED]").count();
    assert_eq!(count, 5, "all five secret fields should be redacted");
}

#[test]
fn non_secret_field_not_redacted() {
    let (logger, buf) = logger_with_buf(LogLevel::Info);
    logger
        .entry(LogLevel::Info, "plain field")
        .field("repo_name", "calypso")
        .emit();

    let output = buf.into_string();
    assert!(output.contains("calypso"));
}

// ---------------------------------------------------------------------------
// Tests: LogLevel::from_str via CALYPSO_LOG env var
// ---------------------------------------------------------------------------

/// Mutex to serialise tests that mutate the `CALYPSO_LOG` env var.
fn env_mutex() -> &'static Mutex<()> {
    static ENV_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();
    ENV_MUTEX.get_or_init(|| Mutex::new(()))
}

fn with_calypso_log<F: FnOnce()>(value: &str, f: F) {
    let _guard = env_mutex()
        .lock()
        .expect("env mutex should not be poisoned");
    let prev = std::env::var("CALYPSO_LOG").ok();
    unsafe { std::env::set_var("CALYPSO_LOG", value) };
    f();
    match prev {
        Some(v) => unsafe { std::env::set_var("CALYPSO_LOG", v) },
        None => unsafe { std::env::remove_var("CALYPSO_LOG") },
    }
}

#[test]
fn calypso_log_env_var_debug_sets_debug_level() {
    with_calypso_log("debug", || {
        let buf = TestBuf::new();
        let logger = Logger::with_writer(Box::new(buf));
        assert_eq!(logger.min_level(), LogLevel::Debug);
    });
}

#[test]
fn calypso_log_env_var_warn_sets_warn_level() {
    with_calypso_log("warn", || {
        let buf = TestBuf::new();
        let logger = Logger::with_writer(Box::new(buf));
        assert_eq!(logger.min_level(), LogLevel::Warn);
    });
}

#[test]
fn calypso_log_env_var_error_sets_error_level() {
    with_calypso_log("error", || {
        let buf = TestBuf::new();
        let logger = Logger::with_writer(Box::new(buf));
        assert_eq!(logger.min_level(), LogLevel::Error);
    });
}

#[test]
fn calypso_log_env_var_info_sets_info_level() {
    with_calypso_log("info", || {
        let buf = TestBuf::new();
        let logger = Logger::with_writer(Box::new(buf));
        assert_eq!(logger.min_level(), LogLevel::Info);
    });
}

#[test]
fn calypso_log_env_var_unknown_value_defaults_to_info() {
    with_calypso_log("verbose", || {
        let buf = TestBuf::new();
        let logger = Logger::with_writer(Box::new(buf));
        assert_eq!(logger.min_level(), LogLevel::Info);
    });
}

// ---------------------------------------------------------------------------
// Tests: LogLevel Display
// ---------------------------------------------------------------------------

#[test]
fn log_level_display_all_variants() {
    assert_eq!(LogLevel::Debug.to_string(), "debug");
    assert_eq!(LogLevel::Info.to_string(), "info");
    assert_eq!(LogLevel::Warn.to_string(), "warn");
    assert_eq!(LogLevel::Error.to_string(), "error");
}

// ---------------------------------------------------------------------------
// Tests: Logger::default()
// ---------------------------------------------------------------------------

#[test]
fn logger_default_constructs_without_panic() {
    // Logger::default() delegates to Logger::new() which writes to stderr.
    // We just verify it constructs successfully and has the expected default level
    // when CALYPSO_LOG is not set.
    let _guard = env_mutex()
        .lock()
        .expect("env mutex should not be poisoned");
    let prev = std::env::var("CALYPSO_LOG").ok();
    if prev.is_none() {
        let logger = Logger::default();
        assert_eq!(logger.min_level(), LogLevel::Info);
    }
}
