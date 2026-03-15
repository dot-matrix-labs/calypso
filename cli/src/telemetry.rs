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

                let line =
                    format!("{timestamp} {pre}{level_upper}{suf} [{comp_display}] {message}\n",);

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

    /// Helper: create a logger that writes JSON to an in-memory buffer.
    fn json_logger(level: LogLevel) -> (Logger, Arc<Mutex<Vec<u8>>>) {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer: Box<dyn Write + Send> = Box::new(BufWriter(Arc::clone(&buf)));
        let logger = Logger {
            min_level: level,
            format: LogFormat::Json,
            context: CorrelationContext::default(),
            writer: Arc::new(Mutex::new(writer)),
        };
        (logger, buf)
    }

    /// Helper: create a logger that writes Text to an in-memory buffer.
    fn text_logger(level: LogLevel) -> (Logger, Arc<Mutex<Vec<u8>>>) {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer: Box<dyn Write + Send> = Box::new(BufWriter(Arc::clone(&buf)));
        let logger = Logger {
            min_level: level,
            format: LogFormat::Text,
            context: CorrelationContext::default(),
            writer: Arc::new(Mutex::new(writer)),
        };
        (logger, buf)
    }

    /// A simple writer that delegates to a shared `Vec<u8>`.
    struct BufWriter(Arc<Mutex<Vec<u8>>>);

    impl Write for BufWriter {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn buf_to_string(buf: &Arc<Mutex<Vec<u8>>>) -> String {
        String::from_utf8(buf.lock().unwrap().clone()).unwrap()
    }

    // -- Component --

    #[test]
    fn component_as_str_all_variants() {
        assert_eq!(Component::Doctor.as_str(), "doctor");
        assert_eq!(Component::StateMachine.as_str(), "statemachine");
        assert_eq!(Component::Gate.as_str(), "gate");
        assert_eq!(Component::Agent.as_str(), "agent");
        assert_eq!(Component::Github.as_str(), "github");
        assert_eq!(Component::Git.as_str(), "git");
        assert_eq!(Component::Init.as_str(), "init");
        assert_eq!(Component::Cli.as_str(), "cli");
    }

    #[test]
    fn component_display() {
        assert_eq!(format!("{}", Component::Doctor), "doctor");
        assert_eq!(format!("{}", Component::Agent), "agent");
    }

    #[test]
    fn component_serialize_json() {
        let json = serde_json::to_string(&Component::Git).unwrap();
        assert_eq!(json, "\"git\"");
    }

    // -- LogEvent --

    #[test]
    fn log_event_as_str_all_variants() {
        assert_eq!(LogEvent::StateTransition.as_str(), "state_transition");
        assert_eq!(LogEvent::GateEvaluated.as_str(), "gate_evaluated");
        assert_eq!(LogEvent::AgentStarted.as_str(), "agent_started");
        assert_eq!(LogEvent::AgentCompleted.as_str(), "agent_completed");
        assert_eq!(LogEvent::DoctorCheck.as_str(), "doctor_check");
        assert_eq!(LogEvent::DoctorFailed.as_str(), "doctor_failed");
        assert_eq!(LogEvent::Startup.as_str(), "startup");
        assert_eq!(LogEvent::Shutdown.as_str(), "shutdown");
    }

    #[test]
    fn log_event_display() {
        assert_eq!(format!("{}", LogEvent::Startup), "startup");
        assert_eq!(format!("{}", LogEvent::Shutdown), "shutdown");
    }

    #[test]
    fn log_event_serialize_json() {
        let json = serde_json::to_string(&LogEvent::GateEvaluated).unwrap();
        assert_eq!(json, "\"gate_evaluated\"");
    }

    // -- LogFormat --

    #[test]
    fn log_format_equality() {
        assert_eq!(LogFormat::Json, LogFormat::Json);
        assert_eq!(LogFormat::Text, LogFormat::Text);
        assert_ne!(LogFormat::Json, LogFormat::Text);
    }

    // -- LogLevel --

    #[test]
    fn log_level_trace_from_str_and_as_str() {
        assert_eq!(LogLevel::from_str("trace"), Some(LogLevel::Trace));
        assert_eq!(LogLevel::from_str("TRACE"), Some(LogLevel::Trace));
        assert_eq!(LogLevel::Trace.as_str(), "trace");
    }

    #[test]
    fn log_level_display() {
        assert_eq!(format!("{}", LogLevel::Trace), "trace");
        assert_eq!(format!("{}", LogLevel::Error), "error");
    }

    #[test]
    fn log_level_from_str_invalid_returns_none() {
        assert_eq!(LogLevel::from_str("bogus"), None);
    }

    // -- Text format --

    #[test]
    fn text_format_output_contains_level_and_message() {
        let (logger, buf) = text_logger(LogLevel::Info);
        logger.info("hello world");
        let output = buf_to_string(&buf);
        assert!(output.contains("INFO"), "expected INFO in: {output}");
        assert!(
            output.contains("hello world"),
            "expected message in: {output}"
        );
        // Component should be "-" when not set
        assert!(
            output.contains("[-]"),
            "expected [-] for no component in: {output}"
        );
    }

    #[test]
    fn text_format_with_component() {
        let (logger, buf) = text_logger(LogLevel::Debug);
        logger.log_event(
            LogLevel::Info,
            Component::Doctor,
            LogEvent::DoctorCheck,
            "checking",
            BTreeMap::new(),
        );
        let output = buf_to_string(&buf);
        assert!(
            output.contains("[doctor]"),
            "expected [doctor] in: {output}"
        );
        assert!(output.contains("checking"), "expected message in: {output}");
    }

    // -- ANSI colour helpers --

    #[test]
    fn ansi_level_prefix_no_color() {
        let (pre, suf) = ansi_level_prefix(LogLevel::Error, false);
        assert!(pre.is_empty());
        assert!(suf.is_empty());
    }

    #[test]
    fn ansi_level_prefix_with_color() {
        let (pre, suf) = ansi_level_prefix(LogLevel::Error, true);
        assert_eq!(pre, "\x1b[31m"); // red
        assert_eq!(suf, "\x1b[0m");

        let (pre, _) = ansi_level_prefix(LogLevel::Warn, true);
        assert_eq!(pre, "\x1b[33m"); // yellow

        let (pre, _) = ansi_level_prefix(LogLevel::Info, true);
        assert_eq!(pre, "\x1b[32m"); // green

        let (pre, _) = ansi_level_prefix(LogLevel::Debug, true);
        assert_eq!(pre, "\x1b[2m"); // dim

        let (pre, _) = ansi_level_prefix(LogLevel::Trace, true);
        assert_eq!(pre, "\x1b[2m"); // dim
    }

    // -- is_tty --

    #[test]
    fn is_tty_returns_false_in_tests() {
        // In CI and test environments, stderr is not a TTY.
        assert!(!is_tty());
    }

    // -- log_event() --

    #[test]
    fn log_event_includes_component_and_event_in_json() {
        let (logger, buf) = json_logger(LogLevel::Debug);
        logger.log_event(
            LogLevel::Info,
            Component::Gate,
            LogEvent::GateEvaluated,
            "gate passed",
            BTreeMap::new(),
        );
        let output = buf_to_string(&buf);
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(parsed["component"], "gate");
        assert_eq!(parsed["event"], "gate_evaluated");
        assert_eq!(parsed["message"], "gate passed");
        assert_eq!(parsed["level"], "info");
    }

    // -- log_level_override_notice --

    #[test]
    fn log_level_override_notice_emits_expected_message() {
        let (logger, buf) = json_logger(LogLevel::Debug);
        logger.log_level_override_notice("debug", LogLevel::Warn);
        let output = buf_to_string(&buf);
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(parsed["component"], "cli");
        assert_eq!(parsed["event"], "startup");
        let msg = parsed["message"].as_str().unwrap();
        assert!(
            msg.contains("CALYPSO_LOG=debug"),
            "expected env ref in: {msg}"
        );
        assert!(msg.contains("warn"), "expected resolved level in: {msg}");
    }

    // -- LogEntryBuilder with component and event --

    #[test]
    fn entry_builder_with_component_and_event() {
        let (logger, buf) = json_logger(LogLevel::Debug);
        logger
            .entry(LogLevel::Info, "builder test")
            .component(Component::Agent)
            .event(LogEvent::AgentStarted)
            .field("key", "value")
            .emit();
        let output = buf_to_string(&buf);
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(parsed["component"], "agent");
        assert_eq!(parsed["event"], "agent_started");
        assert_eq!(parsed["message"], "builder test");
        assert_eq!(parsed["fields"]["key"], "value");
    }

    #[test]
    fn entry_builder_without_component_omits_field() {
        let (logger, buf) = json_logger(LogLevel::Debug);
        logger.entry(LogLevel::Info, "no comp").emit();
        let output = buf_to_string(&buf);
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert!(
            parsed.get("component").is_none(),
            "component should be omitted"
        );
        assert!(parsed.get("event").is_none(), "event should be omitted");
    }

    // -- Redaction --

    #[test]
    fn redaction_of_secret_keys() {
        assert_eq!(redact_if_secret("api_token", "abc123"), "[REDACTED]");
        assert_eq!(redact_if_secret("secret_key", "xyz"), "[REDACTED]");
        assert_eq!(redact_if_secret("password", "pass"), "[REDACTED]");
        assert_eq!(redact_if_secret("credential_file", "f"), "[REDACTED]");
        assert_eq!(redact_if_secret("api_key", "k"), "[REDACTED]");
        assert_eq!(redact_if_secret("auth_header", "h"), "[REDACTED]");
        assert_eq!(redact_if_secret("host", "example.com"), "example.com");
    }

    #[test]
    fn entry_builder_redacts_secret_fields() {
        let (logger, buf) = json_logger(LogLevel::Debug);
        logger
            .entry(LogLevel::Info, "redact test")
            .field("auth_token", "super-secret")
            .field("hostname", "example.com")
            .emit();
        let output = buf_to_string(&buf);
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(parsed["fields"]["auth_token"], "[REDACTED]");
        assert_eq!(parsed["fields"]["hostname"], "example.com");
    }

    // -- Filtering by level --

    #[test]
    fn log_below_min_level_is_suppressed() {
        let (logger, buf) = json_logger(LogLevel::Warn);
        logger.info("should be suppressed");
        logger.debug("also suppressed");
        logger.warn("should appear");
        let output = buf_to_string(&buf);
        assert!(!output.contains("suppressed"));
        assert!(output.contains("should appear"));
    }

    // -- CorrelationContext --

    #[test]
    fn correlation_context_appears_in_json() {
        let (logger, buf) = json_logger(LogLevel::Debug);
        let logger = logger.with_context(
            CorrelationContext::new()
                .with_feature_id("feat-1")
                .with_session_id("sess-1")
                .with_thread_id("thread-1"),
        );
        logger.info("ctx test");
        let output = buf_to_string(&buf);
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(parsed["feature_id"], "feat-1");
        assert_eq!(parsed["session_id"], "sess-1");
        assert_eq!(parsed["thread_id"], "thread-1");
    }

    // -- with_format / with_min_level builders --

    #[test]
    fn with_format_switches_to_text() {
        let buf = Arc::new(Mutex::new(Vec::<u8>::new()));
        let writer: Box<dyn Write + Send> = Box::new(BufWriter(Arc::clone(&buf)));
        let logger = Logger {
            min_level: LogLevel::Info,
            format: LogFormat::Json,
            context: CorrelationContext::default(),
            writer: Arc::new(Mutex::new(writer)),
        };
        let logger = logger.with_format(LogFormat::Text);
        logger.info("format test");
        let output = buf_to_string(&buf);
        // Text format uses uppercase level and brackets
        assert!(output.contains("INFO"), "expected text format");
        assert!(!output.starts_with('{'), "should not be JSON");
    }

    #[test]
    fn with_min_level_changes_threshold() {
        let (logger, buf) = json_logger(LogLevel::Info);
        let logger = logger.with_min_level(LogLevel::Error);
        logger.warn("nope");
        logger.error("yes");
        let output = buf_to_string(&buf);
        assert!(!output.contains("nope"));
        assert!(output.contains("yes"));
    }

    // -- min_level accessor --

    #[test]
    fn min_level_returns_configured_level() {
        let (logger, _) = json_logger(LogLevel::Warn);
        assert_eq!(logger.min_level(), LogLevel::Warn);
    }

    // -- Logger Debug impl --

    #[test]
    fn logger_debug_impl() {
        let (logger, _) = json_logger(LogLevel::Info);
        let debug = format!("{logger:?}");
        assert!(debug.contains("Logger"));
        assert!(debug.contains("min_level"));
    }

    // -- field_json on builder --

    #[test]
    fn entry_builder_field_json() {
        let (logger, buf) = json_logger(LogLevel::Debug);
        logger
            .entry(LogLevel::Info, "json field")
            .field_json("count", serde_json::json!(42))
            .emit();
        let output = buf_to_string(&buf);
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(parsed["fields"]["count"], 42);
    }

    // -- days_to_ymd --

    #[test]
    fn days_to_ymd_epoch() {
        assert_eq!(days_to_ymd(0), (1970, 1, 1));
    }

    #[test]
    fn days_to_ymd_known_date() {
        // 2024-01-01 is day 19723
        assert_eq!(days_to_ymd(19723), (2024, 1, 1));
    }

    // -- EventKind Display --

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

    // -- Event constructors --

    #[test]
    fn event_state_transition() {
        let e = Event::state_transition("idle", "running", Some("f1"));
        assert_eq!(e.kind, EventKind::StateTransition);
        assert_eq!(e.payload["from"], "idle");
        assert_eq!(e.payload["to"], "running");
        assert_eq!(e.payload["feature_id"], "f1");
    }

    #[test]
    fn event_state_transition_no_feature() {
        let e = Event::state_transition("idle", "running", None);
        assert!(!e.payload.contains_key("feature_id"));
    }

    #[test]
    fn event_gate_changed() {
        let e = Event::gate_changed("g1", "pass", Some("f2"));
        assert_eq!(e.kind, EventKind::GateChanged);
        assert_eq!(e.payload["gate_id"], "g1");
        assert_eq!(e.payload["status"], "pass");
    }

    #[test]
    fn event_gate_changed_no_feature() {
        let e = Event::gate_changed("g1", "pass", None);
        assert!(!e.payload.contains_key("feature_id"));
    }

    #[test]
    fn event_session_started() {
        let e = Event::session_started("s1", Some("f3"));
        assert_eq!(e.kind, EventKind::SessionStarted);
        assert_eq!(e.payload["session_id"], "s1");
    }

    #[test]
    fn event_session_started_no_feature() {
        let e = Event::session_started("s1", None);
        assert!(!e.payload.contains_key("feature_id"));
    }

    #[test]
    fn event_session_ended() {
        let e = Event::session_ended("s1", "success", Some("f4"));
        assert_eq!(e.kind, EventKind::SessionEnded);
        assert_eq!(e.payload["outcome"], "success");
    }

    #[test]
    fn event_session_ended_no_feature() {
        let e = Event::session_ended("s1", "fail", None);
        assert!(!e.payload.contains_key("feature_id"));
    }

    #[test]
    fn event_git_op() {
        let e = Event::git_op("clone", Some("https://example.com"));
        assert_eq!(e.kind, EventKind::GitOp);
        assert_eq!(e.payload["operation"], "clone");
        assert_eq!(e.payload["detail"], "https://example.com");
    }

    #[test]
    fn event_git_op_no_detail() {
        let e = Event::git_op("fetch", None);
        assert!(!e.payload.contains_key("detail"));
    }

    #[test]
    fn event_github_api_call() {
        let e = Event::github_api_call("/repos", Some(200));
        assert_eq!(e.kind, EventKind::GithubApiCall);
        assert_eq!(e.payload["endpoint"], "/repos");
        assert_eq!(e.payload["status_code"], 200);
    }

    #[test]
    fn event_github_api_call_no_status() {
        let e = Event::github_api_call("/repos", None);
        assert!(!e.payload.contains_key("status_code"));
    }

    // -- EventStream --

    #[test]
    fn event_stream_push_snapshot_drain() {
        let stream = EventStream::new();
        assert!(stream.snapshot().is_empty());

        stream.push(Event::git_op("push", None));
        stream.push(Event::git_op("fetch", None));
        assert_eq!(stream.snapshot().len(), 2);

        let drained = stream.drain();
        assert_eq!(drained.len(), 2);
        assert!(stream.snapshot().is_empty());
    }

    // -- log_event! macro --

    #[test]
    fn log_event_macro_emits_fields() {
        let (logger, buf) = json_logger(LogLevel::Debug);
        log_event!(logger, LogLevel::Info, "macro test", "k1" => "v1", "k2" => "v2");
        let output = buf_to_string(&buf);
        let parsed: serde_json::Value = serde_json::from_str(output.trim()).unwrap();
        assert_eq!(parsed["message"], "macro test");
        assert_eq!(parsed["fields"]["k1"], "v1");
        assert_eq!(parsed["fields"]["k2"], "v2");
    }

    // -- _with_level_and_writer --

    #[test]
    fn with_level_and_writer_sets_level() {
        let buf: Vec<u8> = Vec::new();
        let logger = Logger::_with_level_and_writer(LogLevel::Error, Box::new(buf));
        assert_eq!(logger.min_level(), LogLevel::Error);
        assert_eq!(logger.format, LogFormat::Json);
    }

    // -- str_is_empty --

    #[test]
    fn str_is_empty_function() {
        assert!(str_is_empty(&""));
        assert!(!str_is_empty(&"hello"));
    }

    // -- is_secret_key --

    #[test]
    fn is_secret_key_detects_secrets() {
        assert!(is_secret_key("api_token"));
        assert!(is_secret_key("MY_SECRET"));
        assert!(is_secret_key("password"));
        assert!(is_secret_key("CREDENTIAL_FILE"));
        assert!(is_secret_key("api_key"));
        assert!(is_secret_key("auth_header"));
        assert!(!is_secret_key("hostname"));
        assert!(!is_secret_key("port"));
    }

    // -- rfc3339_now --

    #[test]
    fn rfc3339_now_format() {
        let ts = rfc3339_now();
        // Should match YYYY-MM-DDTHH:MM:SSZ
        assert!(ts.ends_with('Z'));
        assert_eq!(ts.len(), 20);
        assert_eq!(&ts[4..5], "-");
        assert_eq!(&ts[7..8], "-");
        assert_eq!(&ts[10..11], "T");
    }

    // -- Logger convenience methods --

    #[test]
    fn trace_debug_error_convenience_methods() {
        let (logger, buf) = json_logger(LogLevel::Trace);
        logger.trace("t");
        logger.debug("d");
        logger.error("e");
        let output = buf_to_string(&buf);
        assert!(output.contains("\"level\":\"trace\""));
        assert!(output.contains("\"level\":\"debug\""));
        assert!(output.contains("\"level\":\"error\""));
    }

    // -- CorrelationContext builders --

    #[test]
    fn correlation_context_builders() {
        let ctx = CorrelationContext::new()
            .with_feature_id("f")
            .with_session_id("s")
            .with_thread_id("t");
        assert_eq!(ctx.feature_id.as_deref(), Some("f"));
        assert_eq!(ctx.session_id.as_deref(), Some("s"));
        assert_eq!(ctx.thread_id.as_deref(), Some("t"));
    }

    #[test]
    fn correlation_context_default_is_empty() {
        let ctx = CorrelationContext::default();
        assert!(ctx.feature_id.is_none());
        assert!(ctx.session_id.is_none());
        assert!(ctx.thread_id.is_none());
    }
}
