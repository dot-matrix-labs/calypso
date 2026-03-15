use std::collections::BTreeMap;
use std::fmt;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Log level
// ---------------------------------------------------------------------------

/// Severity level for log entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    /// Parse a level from the `CALYPSO_LOG` env-var value.
    #[allow(clippy::should_implement_trait)]
    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_ascii_lowercase().as_str() {
            "trace" => Some(Self::Trace),
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warn" => Some(Self::Warn),
            "error" => Some(Self::Error),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warn => "warn",
            Self::Error => "error",
        }
    }
}

impl fmt::Display for LogLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Component
// ---------------------------------------------------------------------------

/// Identifies which subsystem produced a log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Component {
    Doctor,
    StateMachine,
    Gate,
    Agent,
    Github,
    Git,
    Init,
    Cli,
}

impl Component {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Doctor => "doctor",
            Self::StateMachine => "statemachine",
            Self::Gate => "gate",
            Self::Agent => "agent",
            Self::Github => "github",
            Self::Git => "git",
            Self::Init => "init",
            Self::Cli => "cli",
        }
    }
}

impl Serialize for Component {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl fmt::Display for Component {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// LogEvent (structured event tag on a log entry)
// ---------------------------------------------------------------------------

/// A structured event tag that can be attached to a log entry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogEvent {
    StateTransition,
    GateEvaluated,
    AgentStarted,
    AgentCompleted,
    DoctorCheck,
    DoctorFailed,
    Startup,
    Shutdown,
}

impl LogEvent {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::StateTransition => "state_transition",
            Self::GateEvaluated => "gate_evaluated",
            Self::AgentStarted => "agent_started",
            Self::AgentCompleted => "agent_completed",
            Self::DoctorCheck => "doctor_check",
            Self::DoctorFailed => "doctor_failed",
            Self::Startup => "startup",
            Self::Shutdown => "shutdown",
        }
    }
}

impl Serialize for LogEvent {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.as_str())
    }
}

impl fmt::Display for LogEvent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

// ---------------------------------------------------------------------------
// Log format
// ---------------------------------------------------------------------------

/// Output format for the [`Logger`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogFormat {
    /// JSON-lines (one JSON object per line).
    Json,
    /// Human-readable text lines with optional ANSI colours.
    Text,
}

// ---------------------------------------------------------------------------
// TTY detection
// ---------------------------------------------------------------------------

/// Returns `true` if stderr (fd 2) is a TTY.
pub fn is_tty() -> bool {
    #[cfg(unix)]
    {
        unsafe extern "C" {
            fn isatty(fd: std::ffi::c_int) -> std::ffi::c_int;
        }
        // SAFETY: isatty is safe to call with any fd; returns 0 for non-TTY.
        unsafe { isatty(2) != 0 }
    }
    #[cfg(not(unix))]
    {
        false
    }
}

// ---------------------------------------------------------------------------
// Redaction
// ---------------------------------------------------------------------------

/// Returns `true` if `value` looks like a secret that must not be logged.
///
/// The heuristic covers common env-var names and bearer-token shapes.
fn is_secret_key(key: &str) -> bool {
    let lower = key.to_ascii_lowercase();
    lower.contains("token")
        || lower.contains("secret")
        || lower.contains("password")
        || lower.contains("credential")
        || lower.contains("api_key")
        || lower.contains("auth")
}

fn redact_if_secret(key: &str, value: &str) -> String {
    if is_secret_key(key) {
        "[REDACTED]".to_string()
    } else {
        value.to_string()
    }
}

// ---------------------------------------------------------------------------
// Correlation context
// ---------------------------------------------------------------------------

/// Optional correlation identifiers that are stamped onto every log entry
/// emitted by a [`Logger`] that carries this context.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorrelationContext {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub feature_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thread_id: Option<String>,
}

impl CorrelationContext {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_feature_id(mut self, id: impl Into<String>) -> Self {
        self.feature_id = Some(id.into());
        self
    }

    pub fn with_session_id(mut self, id: impl Into<String>) -> Self {
        self.session_id = Some(id.into());
        self
    }

    pub fn with_thread_id(mut self, id: impl Into<String>) -> Self {
        self.thread_id = Some(id.into());
        self
    }
}

// ---------------------------------------------------------------------------
// Log entry (internal serialisation shape)
// ---------------------------------------------------------------------------

fn str_is_empty(s: &&str) -> bool {
    s.is_empty()
}

#[derive(Debug, Serialize)]
struct LogEntry<'a> {
    level: &'a str,
    timestamp: String,
    message: &'a str,
    #[serde(skip_serializing_if = "str_is_empty")]
    component: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    event: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    feature_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    session_id: Option<&'a str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thread_id: Option<&'a str>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    fields: BTreeMap<String, serde_json::Value>,
}

fn rfc3339_now() -> String {
    // Produce an RFC 3339 UTC timestamp without pulling in `chrono`.
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let days = secs / 86400; // days since 1970-01-01

    // Gregorian calendar computation
    let (year, month, day) = days_to_ymd(days);

    format!("{year:04}-{month:02}-{day:02}T{h:02}:{m:02}:{s:02}Z")
}

/// Convert days since Unix epoch (1970-01-01) to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from https://howardhinnant.github.io/date_algorithms.html
    let z = days as i64 + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as u64, m, d)
}

// ---------------------------------------------------------------------------
// ANSI colour helpers (text format)
// ---------------------------------------------------------------------------

fn ansi_level_prefix(level: LogLevel, use_color: bool) -> (&'static str, &'static str) {
    if !use_color {
        return ("", "");
    }
    match level {
        LogLevel::Error => ("\x1b[31m", "\x1b[0m"), // red
        LogLevel::Warn => ("\x1b[33m", "\x1b[0m"),  // yellow
        LogLevel::Info => ("\x1b[32m", "\x1b[0m"),  // green
        LogLevel::Debug => ("\x1b[2m", "\x1b[0m"),  // dim/gray
        LogLevel::Trace => ("\x1b[2m", "\x1b[0m"),  // dim
    }
}

// ---------------------------------------------------------------------------
// Logger
// ---------------------------------------------------------------------------

/// A lightweight structured logger that writes JSON-lines to stderr.
///
/// The minimum level is controlled by the `CALYPSO_LOG` environment variable
/// (default: `info`).  The logger is cheaply cloneable — clones share the
/// same underlying writer lock.
#[derive(Clone)]
pub struct Logger {
    min_level: LogLevel,
    format: LogFormat,
    context: CorrelationContext,
    writer: Arc<Mutex<Box<dyn Write + Send>>>,
}

impl fmt::Debug for Logger {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Logger")
            .field("min_level", &self.min_level)
            .field("format", &self.format)
            .field("context", &self.context)
            .finish()
    }
}

impl Logger {
    /// Create a new logger writing to stderr, reading the level from the
    /// `CALYPSO_LOG` env var (default: `info`).
    pub fn new() -> Self {
        Self::with_writer(Box::new(std::io::stderr()))
    }

    /// Create a logger with an explicit writer (useful for tests).
    pub fn with_writer(writer: Box<dyn Write + Send>) -> Self {
        let min_level = std::env::var("CALYPSO_LOG")
            .ok()
            .and_then(|v| LogLevel::from_str(&v))
            .unwrap_or(LogLevel::Info);
        Self {
            min_level,
            format: LogFormat::Json,
            context: CorrelationContext::default(),
            writer: Arc::new(Mutex::new(writer)),
        }
    }

    /// Create a logger with an explicit minimum level (overrides env var).
    pub fn with_level(level: LogLevel) -> Self {
        let mut logger = Self::new();
        logger.min_level = level;
        logger
    }

    /// Return a clone of this logger with additional correlation context.
    pub fn with_context(mut self, context: CorrelationContext) -> Self {
        self.context = context;
        self
    }

    /// Builder: set the output format.
    pub fn with_format(mut self, format: LogFormat) -> Self {
        self.format = format;
        self
    }

    /// Builder: set the minimum log level.
    pub fn with_min_level(mut self, level: LogLevel) -> Self {
        self.min_level = level;
        self
    }

    /// Emit a log entry if `level >= min_level`.
    pub fn log(&self, level: LogLevel, message: &str, fields: BTreeMap<String, serde_json::Value>) {
        self.log_full(level, "", None, message, fields);
    }

    /// Emit a fully structured log entry with component and event.
    pub fn log_event(
        &self,
        level: LogLevel,
        component: Component,
        event: LogEvent,
        message: &str,
        fields: BTreeMap<String, serde_json::Value>,
    ) {
        self.log_full(
            level,
            component.as_str(),
            Some(event.as_str()),
            message,
            fields,
        );
    }

    /// Internal: emit a log entry with all fields.
    fn log_full(
        &self,
        level: LogLevel,
        component: &str,
        event: Option<&str>,
        message: &str,
        fields: BTreeMap<String, serde_json::Value>,
    ) {
        if level < self.min_level {
            return;
        }

        let timestamp = rfc3339_now();

        match self.format {
            LogFormat::Json => {
                let entry = LogEntry {
                    level: level.as_str(),
                    timestamp,
                    message,
                    component,
                    event,
                    feature_id: self.context.feature_id.as_deref(),
                    session_id: self.context.session_id.as_deref(),
                    thread_id: self.context.thread_id.as_deref(),
                    fields,
                };

                if let Ok(mut json) = serde_json::to_string(&entry) {
                    json.push('\n');
                    if let Ok(mut w) = self.writer.lock() {
                        let _ = w.write_all(json.as_bytes());
                    }
                }
            }
            LogFormat::Text => {
                let use_color = is_tty();
                let (pre, suf) = ansi_level_prefix(level, use_color);
                let level_upper = level.as_str().to_ascii_uppercase();

                let comp_display = if component.is_empty() { "-" } else { component };

                let line = format!(
                    "{ts} {pre}{lvl}{suf} [{comp}] {msg}\n",
                    ts = timestamp,
                    pre = pre,
                    lvl = level_upper,
                    suf = suf,
                    comp = comp_display,
                    msg = message,
                );

                if let Ok(mut w) = self.writer.lock() {
                    let _ = w.write_all(line.as_bytes());
                }
            }
        }
    }

    /// Convenience: log at `trace` level.
    pub fn trace(&self, message: &str) {
        self.log(LogLevel::Trace, message, BTreeMap::new());
    }

    /// Convenience: log at `debug` level.
    pub fn debug(&self, message: &str) {
        self.log(LogLevel::Debug, message, BTreeMap::new());
    }

    /// Convenience: log at `info` level.
    pub fn info(&self, message: &str) {
        self.log(LogLevel::Info, message, BTreeMap::new());
    }

    /// Convenience: log at `warn` level.
    pub fn warn(&self, message: &str) {
        self.log(LogLevel::Warn, message, BTreeMap::new());
    }

    /// Convenience: log at `error` level.
    pub fn error(&self, message: &str) {
        self.log(LogLevel::Error, message, BTreeMap::new());
    }

    /// Emit an info-level notice when the `CALYPSO_LOG` env var was set but
    /// a CLI verbosity flag takes precedence.
    pub fn log_level_override_notice(&self, env_value: &str, resolved: LogLevel) {
        let msg = format!("verbosity flag overrides CALYPSO_LOG={env_value}; using {resolved}");
        self.log_event(
            LogLevel::Info,
            Component::Cli,
            LogEvent::Startup,
            &msg,
            BTreeMap::new(),
        );
    }

    /// Build a log entry with structured fields using the builder returned by
    /// this method.
    pub fn entry(&self, level: LogLevel, message: &str) -> LogEntryBuilder<'_> {
        LogEntryBuilder {
            logger: self,
            level,
            message: message.to_string(),
            component: None,
            event: None,
            fields: BTreeMap::new(),
        }
    }

    pub fn min_level(&self) -> LogLevel {
        self.min_level
    }

    /// Internal constructor for tests: set level explicitly and use the
    /// provided writer.  Not part of the public API surface — prefixed with
    /// `_` to signal internal use.
    #[doc(hidden)]
    pub fn _with_level_and_writer(level: LogLevel, writer: Box<dyn Write + Send>) -> Self {
        Self {
            min_level: level,
            format: LogFormat::Json,
            context: CorrelationContext::default(),
            writer: Arc::new(Mutex::new(writer)),
        }
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// Log entry builder (ergonomic field API)
// ---------------------------------------------------------------------------

/// Builder for a single structured log entry.
pub struct LogEntryBuilder<'a> {
    logger: &'a Logger,
    level: LogLevel,
    message: String,
    component: Option<Component>,
    event: Option<LogEvent>,
    fields: BTreeMap<String, serde_json::Value>,
}

impl<'a> LogEntryBuilder<'a> {
    /// Add a string field.  The value is automatically redacted if the key
    /// looks like a secret.
    pub fn field(mut self, key: &str, value: impl Into<String>) -> Self {
        let v = redact_if_secret(key, &value.into());
        self.fields
            .insert(key.to_string(), serde_json::Value::String(v));
        self
    }

    /// Add a field with an arbitrary JSON value.
    pub fn field_json(mut self, key: &str, value: serde_json::Value) -> Self {
        self.fields.insert(key.to_string(), value);
        self
    }

    /// Set the component for this log entry.
    pub fn component(mut self, component: Component) -> Self {
        self.component = component.into();
        self
    }

    /// Set the event tag for this log entry.
    pub fn event(mut self, event: LogEvent) -> Self {
        self.event = event.into();
        self
    }

    /// Emit the entry.
    pub fn emit(self) {
        let comp = self.component.map(|c| c.as_str()).unwrap_or("");
        let evt = self.event.map(|e| e.as_str());
        self.logger
            .log_full(self.level, comp, evt, &self.message, self.fields);
    }
}

// ---------------------------------------------------------------------------
// log_event! macro
// ---------------------------------------------------------------------------

/// Emit a structured log entry.
///
/// ```ignore
/// log_event!(logger, LogLevel::Info, "something happened", "key" => "value");
/// ```
#[macro_export]
macro_rules! log_event {
    ($logger:expr, $level:expr, $message:expr $(, $key:expr => $value:expr)* $(,)?) => {{
        let mut _builder = $logger.entry($level, $message);
        $(
            _builder = _builder.field($key, $value.to_string());
        )*
        _builder.emit();
    }};
}

// ---------------------------------------------------------------------------
// Event stream
// ---------------------------------------------------------------------------

/// The kind of a structured event.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    StateTransition,
    GateChanged,
    SessionStarted,
    SessionEnded,
    GitOp,
    GithubApiCall,
}

impl fmt::Display for EventKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let s = match self {
            Self::StateTransition => "state_transition",
            Self::GateChanged => "gate_changed",
            Self::SessionStarted => "session_started",
            Self::SessionEnded => "session_ended",
            Self::GitOp => "git_op",
            Self::GithubApiCall => "github_api_call",
        };
        f.write_str(s)
    }
}

/// A single structured event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub kind: EventKind,
    pub timestamp: String,
    pub payload: BTreeMap<String, serde_json::Value>,
}

impl Event {
    fn new(kind: EventKind, payload: BTreeMap<String, serde_json::Value>) -> Self {
        Self {
            kind,
            timestamp: rfc3339_now(),
            payload,
        }
    }

    // -- Convenience constructors --

    /// Create a `state_transition` event.
    pub fn state_transition(from: &str, to: &str, feature_id: Option<&str>) -> Self {
        let mut payload = BTreeMap::new();
        payload.insert(
            "from".to_string(),
            serde_json::Value::String(from.to_string()),
        );
        payload.insert("to".to_string(), serde_json::Value::String(to.to_string()));
        if let Some(fid) = feature_id {
            payload.insert(
                "feature_id".to_string(),
                serde_json::Value::String(fid.to_string()),
            );
        }
        Self::new(EventKind::StateTransition, payload)
    }

    /// Create a `gate_changed` event.
    pub fn gate_changed(gate_id: &str, status: &str, feature_id: Option<&str>) -> Self {
        let mut payload = BTreeMap::new();
        payload.insert(
            "gate_id".to_string(),
            serde_json::Value::String(gate_id.to_string()),
        );
        payload.insert(
            "status".to_string(),
            serde_json::Value::String(status.to_string()),
        );
        if let Some(fid) = feature_id {
            payload.insert(
                "feature_id".to_string(),
                serde_json::Value::String(fid.to_string()),
            );
        }
        Self::new(EventKind::GateChanged, payload)
    }

    /// Create a `session_started` event.
    pub fn session_started(session_id: &str, feature_id: Option<&str>) -> Self {
        let mut payload = BTreeMap::new();
        payload.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id.to_string()),
        );
        if let Some(fid) = feature_id {
            payload.insert(
                "feature_id".to_string(),
                serde_json::Value::String(fid.to_string()),
            );
        }
        Self::new(EventKind::SessionStarted, payload)
    }

    /// Create a `session_ended` event.
    pub fn session_ended(session_id: &str, outcome: &str, feature_id: Option<&str>) -> Self {
        let mut payload = BTreeMap::new();
        payload.insert(
            "session_id".to_string(),
            serde_json::Value::String(session_id.to_string()),
        );
        payload.insert(
            "outcome".to_string(),
            serde_json::Value::String(outcome.to_string()),
        );
        if let Some(fid) = feature_id {
            payload.insert(
                "feature_id".to_string(),
                serde_json::Value::String(fid.to_string()),
            );
        }
        Self::new(EventKind::SessionEnded, payload)
    }

    /// Create a `git_op` event.
    pub fn git_op(operation: &str, detail: Option<&str>) -> Self {
        let mut payload = BTreeMap::new();
        payload.insert(
            "operation".to_string(),
            serde_json::Value::String(operation.to_string()),
        );
        if let Some(d) = detail {
            payload.insert(
                "detail".to_string(),
                serde_json::Value::String(d.to_string()),
            );
        }
        Self::new(EventKind::GitOp, payload)
    }

    /// Create a `github_api_call` event.
    pub fn github_api_call(endpoint: &str, status_code: Option<u16>) -> Self {
        let mut payload = BTreeMap::new();
        payload.insert(
            "endpoint".to_string(),
            serde_json::Value::String(endpoint.to_string()),
        );
        if let Some(code) = status_code {
            payload.insert(
                "status_code".to_string(),
                serde_json::Value::Number(serde_json::Number::from(code)),
            );
        }
        Self::new(EventKind::GithubApiCall, payload)
    }
}

/// An append-only stream of structured events, safe to share across threads.
#[derive(Debug, Clone, Default)]
pub struct EventStream {
    events: Arc<Mutex<Vec<Event>>>,
}

impl EventStream {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event to the stream.
    pub fn push(&self, event: Event) {
        if let Ok(mut events) = self.events.lock() {
            events.push(event);
        }
    }

    /// Return a snapshot of all events recorded so far.
    pub fn snapshot(&self) -> Vec<Event> {
        self.events.lock().map(|e| e.clone()).unwrap_or_default()
    }

    /// Drain all events, returning them and leaving the stream empty.
    pub fn drain(&self) -> Vec<Event> {
        self.events
            .lock()
            .map(|mut e| std::mem::take(&mut *e))
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A writer that captures output for test assertions.
    #[derive(Clone)]
    struct CaptureWriter {
        buffer: Arc<Mutex<Vec<u8>>>,
    }

    impl CaptureWriter {
        fn new() -> Self {
            Self {
                buffer: Arc::new(Mutex::new(Vec::new())),
            }
        }

        fn contents(&self) -> String {
            let buf = self.buffer.lock().unwrap();
            String::from_utf8_lossy(&buf).to_string()
        }
    }

    impl std::io::Write for CaptureWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.buffer.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }

        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn make_logger(writer: CaptureWriter, format: LogFormat) -> Logger {
        Logger::_with_level_and_writer(LogLevel::Trace, Box::new(writer)).with_format(format)
    }

    // ---- Text format rendering ----

    #[test]
    fn text_format_renders_component_and_message() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone(), LogFormat::Text);

        logger.log_event(
            LogLevel::Info,
            Component::Doctor,
            LogEvent::DoctorCheck,
            "checking prerequisites",
            BTreeMap::new(),
        );

        let output = writer.contents();
        assert!(output.contains("INFO"), "expected INFO in: {output}");
        assert!(
            output.contains("[doctor]"),
            "expected [doctor] in: {output}"
        );
        assert!(
            output.contains("checking prerequisites"),
            "expected message in: {output}"
        );
    }

    #[test]
    fn text_format_uses_dash_for_empty_component() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone(), LogFormat::Text);

        logger.log(LogLevel::Warn, "bare message", BTreeMap::new());

        let output = writer.contents();
        assert!(output.contains("[-]"), "expected [-] in: {output}");
        assert!(output.contains("WARN"), "expected WARN in: {output}");
    }

    #[test]
    fn text_format_all_levels() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone(), LogFormat::Text);

        logger.trace("t");
        logger.debug("d");
        logger.info("i");
        logger.warn("w");
        logger.error("e");

        let output = writer.contents();
        assert!(output.contains("TRACE"), "expected TRACE in: {output}");
        assert!(output.contains("DEBUG"), "expected DEBUG in: {output}");
        assert!(output.contains("INFO"), "expected INFO in: {output}");
        assert!(output.contains("WARN"), "expected WARN in: {output}");
        assert!(output.contains("ERROR"), "expected ERROR in: {output}");
    }

    // ---- with_context ----

    #[test]
    fn with_context_stamps_correlation_ids_on_json_output() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone(), LogFormat::Json).with_context(
            CorrelationContext::new()
                .with_feature_id("feat-123")
                .with_session_id("sess-456")
                .with_thread_id("thread-789"),
        );

        logger.info("correlated message");

        let output = writer.contents();
        assert!(
            output.contains("\"feature_id\":\"feat-123\""),
            "expected feature_id in: {output}"
        );
        assert!(
            output.contains("\"session_id\":\"sess-456\""),
            "expected session_id in: {output}"
        );
        assert!(
            output.contains("\"thread_id\":\"thread-789\""),
            "expected thread_id in: {output}"
        );
    }

    // ---- LogEntryBuilder ----

    #[test]
    fn entry_builder_with_event_tag() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone(), LogFormat::Json);

        logger
            .entry(LogLevel::Info, "transition happened")
            .component(Component::StateMachine)
            .event(LogEvent::StateTransition)
            .field("from", "new")
            .field("to", "in_progress")
            .emit();

        let output = writer.contents();
        assert!(
            output.contains("\"event\":\"state_transition\""),
            "expected event in: {output}"
        );
        assert!(
            output.contains("\"component\":\"statemachine\""),
            "expected component in: {output}"
        );
        assert!(
            output.contains("\"from\":\"new\""),
            "expected from field in: {output}"
        );
    }

    #[test]
    fn entry_builder_field_json() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone(), LogFormat::Json);

        logger
            .entry(LogLevel::Info, "with json field")
            .field_json("count", serde_json::json!(42))
            .emit();

        let output = writer.contents();
        assert!(
            output.contains("\"count\":42"),
            "expected count field in: {output}"
        );
    }

    // ---- Redaction ----

    #[test]
    fn secret_fields_are_redacted() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone(), LogFormat::Json);

        logger
            .entry(LogLevel::Info, "auth check")
            .field("api_key", "sk-1234567890")
            .field("auth_token", "ghp_abc")
            .field("password", "hunter2")
            .field("safe_field", "visible")
            .emit();

        let output = writer.contents();
        assert!(
            output.contains("[REDACTED]"),
            "expected redaction in: {output}"
        );
        assert!(
            !output.contains("sk-1234567890"),
            "api_key should be redacted"
        );
        assert!(!output.contains("ghp_abc"), "auth_token should be redacted");
        assert!(!output.contains("hunter2"), "password should be redacted");
        assert!(output.contains("visible"), "safe_field should be visible");
    }

    // ---- Level filtering ----

    #[test]
    fn below_min_level_is_suppressed() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone(), LogFormat::Json).with_min_level(LogLevel::Warn);

        logger.info("should not appear");
        logger.debug("also hidden");
        logger.warn("should appear");

        let output = writer.contents();
        assert!(
            !output.contains("should not appear"),
            "info should be suppressed"
        );
        assert!(output.contains("should appear"), "warn should appear");
    }

    // ---- LogLevel ----

    #[test]
    fn log_level_from_str_coverage() {
        assert_eq!(LogLevel::from_str("trace"), Some(LogLevel::Trace));
        assert_eq!(LogLevel::from_str("DEBUG"), Some(LogLevel::Debug));
        assert_eq!(LogLevel::from_str("Info"), Some(LogLevel::Info));
        assert_eq!(LogLevel::from_str("WARN"), Some(LogLevel::Warn));
        assert_eq!(LogLevel::from_str("error"), Some(LogLevel::Error));
        assert_eq!(LogLevel::from_str("bogus"), None);
    }

    #[test]
    fn log_level_display() {
        assert_eq!(format!("{}", LogLevel::Trace), "trace");
        assert_eq!(format!("{}", LogLevel::Error), "error");
    }

    // ---- Component / LogEvent Display ----

    #[test]
    fn component_display_and_serialize() {
        assert_eq!(format!("{}", Component::Agent), "agent");
        assert_eq!(format!("{}", Component::Github), "github");
        assert_eq!(format!("{}", Component::Git), "git");
        assert_eq!(format!("{}", Component::Init), "init");

        // Serialize
        let json = serde_json::to_string(&Component::Gate).unwrap();
        assert_eq!(json, "\"gate\"");
    }

    #[test]
    fn log_event_display_and_serialize() {
        assert_eq!(format!("{}", LogEvent::AgentStarted), "agent_started");
        assert_eq!(format!("{}", LogEvent::AgentCompleted), "agent_completed");
        assert_eq!(format!("{}", LogEvent::Shutdown), "shutdown");
        assert_eq!(format!("{}", LogEvent::Startup), "startup");

        let json = serde_json::to_string(&LogEvent::StateTransition).unwrap();
        assert_eq!(json, "\"state_transition\"");
    }

    // ---- EventKind Display ----

    #[test]
    fn event_kind_display() {
        assert_eq!(
            format!("{}", EventKind::StateTransition),
            "state_transition"
        );
        assert_eq!(format!("{}", EventKind::GateChanged), "gate_changed");
        assert_eq!(format!("{}", EventKind::SessionStarted), "session_started");
        assert_eq!(format!("{}", EventKind::SessionEnded), "session_ended");
        assert_eq!(format!("{}", EventKind::GitOp), "git_op");
        assert_eq!(format!("{}", EventKind::GithubApiCall), "github_api_call");
    }

    // ---- Event constructors ----

    #[test]
    fn event_state_transition_with_feature_id() {
        let e = Event::state_transition("new", "in_progress", Some("feat-1"));
        assert_eq!(e.kind, EventKind::StateTransition);
        assert_eq!(e.payload.get("from").and_then(|v| v.as_str()), Some("new"));
        assert_eq!(
            e.payload.get("to").and_then(|v| v.as_str()),
            Some("in_progress")
        );
        assert_eq!(
            e.payload.get("feature_id").and_then(|v| v.as_str()),
            Some("feat-1")
        );
    }

    #[test]
    fn event_state_transition_without_feature_id() {
        let e = Event::state_transition("a", "b", None);
        assert!(!e.payload.contains_key("feature_id"));
    }

    #[test]
    fn event_gate_changed() {
        let e = Event::gate_changed("g1", "passing", Some("feat-2"));
        assert_eq!(e.kind, EventKind::GateChanged);
        assert_eq!(
            e.payload.get("gate_id").and_then(|v| v.as_str()),
            Some("g1")
        );
        assert_eq!(
            e.payload.get("status").and_then(|v| v.as_str()),
            Some("passing")
        );
    }

    #[test]
    fn event_session_started_and_ended() {
        let started = Event::session_started("s1", Some("feat-3"));
        assert_eq!(started.kind, EventKind::SessionStarted);
        assert_eq!(
            started.payload.get("session_id").and_then(|v| v.as_str()),
            Some("s1")
        );

        let ended = Event::session_ended("s1", "success", None);
        assert_eq!(ended.kind, EventKind::SessionEnded);
        assert_eq!(
            ended.payload.get("outcome").and_then(|v| v.as_str()),
            Some("success")
        );
        assert!(!ended.payload.contains_key("feature_id"));
    }

    #[test]
    fn event_git_op() {
        let e = Event::git_op("commit", Some("abc123"));
        assert_eq!(e.kind, EventKind::GitOp);
        assert_eq!(
            e.payload.get("detail").and_then(|v| v.as_str()),
            Some("abc123")
        );

        let e2 = Event::git_op("push", None);
        assert!(!e2.payload.contains_key("detail"));
    }

    #[test]
    fn event_github_api_call() {
        let e = Event::github_api_call("/repos/owner/repo", Some(200));
        assert_eq!(e.kind, EventKind::GithubApiCall);
        assert_eq!(
            e.payload.get("status_code").and_then(|v| v.as_u64()),
            Some(200)
        );

        let e2 = Event::github_api_call("/repos/owner/repo", None);
        assert!(!e2.payload.contains_key("status_code"));
    }

    // ---- EventStream ----

    #[test]
    fn event_stream_push_snapshot_drain() {
        let stream = EventStream::new();
        assert!(stream.snapshot().is_empty());

        stream.push(Event::git_op("fetch", None));
        stream.push(Event::git_op("pull", None));

        let snap = stream.snapshot();
        assert_eq!(snap.len(), 2);

        let drained = stream.drain();
        assert_eq!(drained.len(), 2);
        assert!(stream.snapshot().is_empty());
    }

    // ---- ANSI colour helper ----

    #[test]
    fn ansi_level_prefix_no_color() {
        let (pre, suf) = ansi_level_prefix(LogLevel::Error, false);
        assert!(pre.is_empty());
        assert!(suf.is_empty());
    }

    #[test]
    fn ansi_level_prefix_with_color() {
        let (pre, suf) = ansi_level_prefix(LogLevel::Error, true);
        assert!(pre.contains("\x1b["), "expected ANSI escape in: {pre}");
        assert!(!suf.is_empty());

        let (pre_w, _) = ansi_level_prefix(LogLevel::Warn, true);
        assert!(pre_w.contains("\x1b[33m"));

        let (pre_i, _) = ansi_level_prefix(LogLevel::Info, true);
        assert!(pre_i.contains("\x1b[32m"));

        let (pre_d, _) = ansi_level_prefix(LogLevel::Debug, true);
        assert!(pre_d.contains("\x1b[2m"));

        let (pre_t, _) = ansi_level_prefix(LogLevel::Trace, true);
        assert!(pre_t.contains("\x1b[2m"));
    }

    // ---- days_to_ymd ----

    #[test]
    fn days_to_ymd_epoch() {
        let (y, m, d) = days_to_ymd(0);
        assert_eq!((y, m, d), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2024-01-01 = day 19723
        let (y, m, d) = days_to_ymd(19723);
        assert_eq!((y, m, d), (2024, 1, 1));
    }

    // ---- Logger Debug ----

    #[test]
    fn logger_debug_impl() {
        let logger = Logger::_with_level_and_writer(LogLevel::Info, Box::new(std::io::sink()));
        let debug = format!("{logger:?}");
        assert!(debug.contains("Logger"));
        assert!(debug.contains("min_level"));
    }

    // ---- log_level_override_notice ----

    #[test]
    fn log_level_override_notice_emits_startup_event() {
        let writer = CaptureWriter::new();
        let logger = make_logger(writer.clone(), LogFormat::Json);

        logger.log_level_override_notice("debug", LogLevel::Info);

        let output = writer.contents();
        assert!(
            output.contains("CALYPSO_LOG=debug"),
            "expected env var in: {output}"
        );
        assert!(
            output.contains("\"event\":\"startup\""),
            "expected startup event in: {output}"
        );
    }

    // ---- CorrelationContext ----

    #[test]
    fn correlation_context_default_is_all_none() {
        let ctx = CorrelationContext::new();
        assert_eq!(ctx.feature_id, None);
        assert_eq!(ctx.session_id, None);
        assert_eq!(ctx.thread_id, None);
    }

    // ---- is_secret_key ----

    #[test]
    fn is_secret_key_coverage() {
        assert!(is_secret_key("auth_token"));
        assert!(is_secret_key("MY_SECRET"));
        assert!(is_secret_key("credential_file"));
        assert!(is_secret_key("API_KEY_ID"));
        assert!(is_secret_key("db_password"));
        assert!(!is_secret_key("username"));
        assert!(!is_secret_key("count"));
    }
}
