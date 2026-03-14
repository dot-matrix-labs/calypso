use std::fmt;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

/// Configuration for the Claude CLI provider.
#[derive(Debug, Clone)]
pub struct ClaudeConfig {
    /// Path to the `claude` binary (default: `"claude"`).
    pub binary: String,
    /// Extra flags to pass on every invocation.
    pub default_flags: Vec<String>,
    /// Name of the environment variable that holds the Anthropic API key.
    pub auth_env_var: String,
}

impl Default for ClaudeConfig {
    fn default() -> Self {
        Self {
            binary: "claude".to_string(),
            default_flags: Vec::new(),
            auth_env_var: "ANTHROPIC_API_KEY".to_string(),
        }
    }
}

/// A single Claude invocation session.
#[derive(Debug, Clone)]
pub struct ClaudeSession {
    pub session_id: String,
    pub config: ClaudeConfig,
}

/// The structured outcome of a completed Claude invocation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClaudeOutcome {
    Ok {
        summary: String,
        artifact_refs: Vec<String>,
        suggested_next_state: Option<String>,
    },
    Nok {
        summary: String,
        reason: String,
    },
    Aborted {
        reason: String,
    },
}

/// A clarification request emitted by Claude mid-session.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClarificationRequest {
    pub question: String,
    pub session_id: String,
}

/// Context passed into a Claude session invocation.
#[derive(Debug, Clone, Default)]
pub struct SessionContext {
    /// Optional working directory for the subprocess.
    pub working_directory: Option<String>,
}

#[derive(Debug)]
pub enum ClaudeError {
    Io(std::io::Error),
    MalformedOutput(String),
    Utf8(std::string::FromUtf8Error),
}

impl fmt::Display for ClaudeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ClaudeError::Io(error) => write!(f, "claude I/O error: {error}"),
            ClaudeError::MalformedOutput(detail) => {
                write!(
                    f,
                    "claude output did not contain a recognised outcome marker: {detail}"
                )
            }
            ClaudeError::Utf8(error) => write!(f, "claude output was not valid UTF-8: {error}"),
        }
    }
}

impl std::error::Error for ClaudeError {}

// ── Outcome / clarification marker constants ──────────────────────────────────

const MARKER_OK: &str = "[CALYPSO:OK]";
const MARKER_NOK: &str = "[CALYPSO:NOK]";
const MARKER_ABORTED: &str = "[CALYPSO:ABORTED]";
const MARKER_CLARIFICATION: &str = "[CALYPSO:CLARIFICATION]";

// ── ClaudeSession impl ────────────────────────────────────────────────────────

impl ClaudeSession {
    /// Create a new session with the supplied config.
    pub fn new(config: ClaudeConfig) -> Self {
        Self {
            session_id: next_session_id(),
            config,
        }
    }

    /// Verify that the Claude binary is reachable and responds.
    ///
    /// Returns `true` when the binary executes successfully, `false` otherwise.
    pub fn check_auth(config: &ClaudeConfig) -> bool {
        Command::new(&config.binary)
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_ok_and(|status| status.success())
    }

    /// Invoke Claude with `prompt`, capturing output and parsing the outcome.
    ///
    /// The full transcript is written as JSON-lines to `transcript_path` when
    /// provided.  The API key is read from the environment variable named in
    /// `config.auth_env_var` — it is never stored, serialised, or logged.
    pub fn invoke(
        &self,
        prompt: &str,
        context: &SessionContext,
        transcript_path: Option<&Path>,
    ) -> Result<ClaudeOutcome, ClaudeError> {
        let mut cmd = Command::new(&self.config.binary);
        cmd.args(&self.config.default_flags);
        cmd.arg(prompt);
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        if let Some(dir) = &context.working_directory {
            cmd.current_dir(dir);
        }

        let output = cmd.output().map_err(ClaudeError::Io)?;

        let stdout = String::from_utf8(output.stdout).map_err(ClaudeError::Utf8)?;
        let stderr = String::from_utf8(output.stderr).map_err(ClaudeError::Utf8)?;

        if let Some(path) = transcript_path {
            write_transcript(path, &self.session_id, prompt, &stdout, &stderr)?;
        }

        parse_outcome(&stdout)
    }
}

// ── Parsing helpers ───────────────────────────────────────────────────────────

/// Parse a `ClaudeOutcome` from raw output text.
pub fn parse_outcome(output: &str) -> Result<ClaudeOutcome, ClaudeError> {
    for line in output.lines() {
        if let Some(payload) = line.strip_prefix(MARKER_OK) {
            return parse_ok(payload.trim());
        }
        if let Some(payload) = line.strip_prefix(MARKER_NOK) {
            return parse_nok(payload.trim());
        }
        if let Some(payload) = line.strip_prefix(MARKER_ABORTED) {
            return parse_aborted(payload.trim());
        }
    }

    Err(ClaudeError::MalformedOutput(
        "no [CALYPSO:OK], [CALYPSO:NOK], or [CALYPSO:ABORTED] marker found".to_string(),
    ))
}

/// Parse a `ClarificationRequest` from raw output text.
pub fn parse_clarification(output: &str, session_id: &str) -> Option<ClarificationRequest> {
    for line in output.lines() {
        if let Some(question) = line.strip_prefix(MARKER_CLARIFICATION) {
            return Some(ClarificationRequest {
                question: question.trim().to_string(),
                session_id: session_id.to_string(),
            });
        }
    }
    None
}

fn parse_ok(payload: &str) -> Result<ClaudeOutcome, ClaudeError> {
    let v: serde_json::Value = serde_json::from_str(payload).map_err(|e| {
        ClaudeError::MalformedOutput(format!("invalid JSON after [CALYPSO:OK]: {e}"))
    })?;

    let summary = string_field(&v, "summary")?;
    let artifact_refs = v
        .get("artifact_refs")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let suggested_next_state = v
        .get("suggested_next_state")
        .and_then(|x| x.as_str())
        .map(str::to_string);

    Ok(ClaudeOutcome::Ok {
        summary,
        artifact_refs,
        suggested_next_state,
    })
}

fn parse_nok(payload: &str) -> Result<ClaudeOutcome, ClaudeError> {
    let v: serde_json::Value = serde_json::from_str(payload).map_err(|e| {
        ClaudeError::MalformedOutput(format!("invalid JSON after [CALYPSO:NOK]: {e}"))
    })?;

    let summary = string_field(&v, "summary")?;
    let reason = string_field(&v, "reason")?;

    Ok(ClaudeOutcome::Nok { summary, reason })
}

fn parse_aborted(payload: &str) -> Result<ClaudeOutcome, ClaudeError> {
    let v: serde_json::Value = serde_json::from_str(payload).map_err(|e| {
        ClaudeError::MalformedOutput(format!("invalid JSON after [CALYPSO:ABORTED]: {e}"))
    })?;

    let reason = string_field(&v, "reason")?;

    Ok(ClaudeOutcome::Aborted { reason })
}

fn string_field(v: &serde_json::Value, field: &str) -> Result<String, ClaudeError> {
    v.get(field)
        .and_then(|x| x.as_str())
        .map(str::to_string)
        .ok_or_else(|| {
            ClaudeError::MalformedOutput(format!("missing required string field `{field}`"))
        })
}

// ── Transcript writer ─────────────────────────────────────────────────────────

fn write_transcript(
    path: &Path,
    session_id: &str,
    prompt: &str,
    stdout: &str,
    stderr: &str,
) -> Result<(), ClaudeError> {
    let entry = serde_json::json!({
        "session_id": session_id,
        "prompt": prompt,
        "stdout": stdout,
        "stderr": stderr,
    });

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .map_err(ClaudeError::Io)?;

    writeln!(file, "{entry}").map_err(ClaudeError::Io)
}

// ── Internal helpers ──────────────────────────────────────────────────────────

fn next_session_id() -> String {
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();

    format!("claude-session-{timestamp}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_outcome_ok_produces_correct_variant() {
        let output = r#"[CALYPSO:OK]{"summary":"done"}"#;
        let outcome = parse_outcome(output).expect("should parse");
        assert_eq!(
            outcome,
            ClaudeOutcome::Ok {
                summary: "done".to_string(),
                artifact_refs: vec![],
                suggested_next_state: None,
            }
        );
    }

    #[test]
    fn parse_outcome_nok_produces_correct_variant() {
        let output = r#"[CALYPSO:NOK]{"summary":"failed","reason":"tests red"}"#;
        let outcome = parse_outcome(output).expect("should parse");
        assert_eq!(
            outcome,
            ClaudeOutcome::Nok {
                summary: "failed".to_string(),
                reason: "tests red".to_string(),
            }
        );
    }

    #[test]
    fn parse_outcome_aborted_produces_correct_variant() {
        let output = r#"[CALYPSO:ABORTED]{"reason":"operator cancelled"}"#;
        let outcome = parse_outcome(output).expect("should parse");
        assert_eq!(
            outcome,
            ClaudeOutcome::Aborted {
                reason: "operator cancelled".to_string(),
            }
        );
    }

    #[test]
    fn parse_clarification_extracts_question() {
        let output = "[CALYPSO:CLARIFICATION]What is the ticket number?";
        let req =
            parse_clarification(output, "claude-session-1").expect("should find clarification");
        assert_eq!(req.question, "What is the ticket number?");
        assert_eq!(req.session_id, "claude-session-1");
    }

    #[test]
    fn parse_outcome_with_no_marker_returns_error() {
        let output = "Some text without any marker";
        let err = parse_outcome(output).expect_err("should fail");
        assert!(err.to_string().contains("no [CALYPSO"));
    }

    #[test]
    fn claude_error_display_covers_all_variants() {
        let io_err = ClaudeError::Io(std::io::Error::other("disk gone"));
        assert!(io_err.to_string().contains("claude I/O error"));

        let malformed = ClaudeError::MalformedOutput("missing field".to_string());
        assert!(malformed.to_string().contains("recognised outcome marker"));

        // Utf8 variant via round-trip
        let bad_bytes = vec![0xFF_u8, 0xFE];
        let utf8_err = String::from_utf8(bad_bytes).unwrap_err();
        let claude_utf8 = ClaudeError::Utf8(utf8_err);
        assert!(claude_utf8.to_string().contains("UTF-8"));
    }

    #[test]
    fn parse_ok_with_invalid_json_returns_error() {
        let err = parse_outcome("[CALYPSO:OK]not-json").expect_err("should fail on bad JSON");
        assert!(err.to_string().contains("invalid JSON after [CALYPSO:OK]"));
    }

    #[test]
    fn parse_nok_with_invalid_json_returns_error() {
        let err = parse_outcome("[CALYPSO:NOK]not-json").expect_err("should fail on bad JSON");
        assert!(err.to_string().contains("invalid JSON after [CALYPSO:NOK]"));
    }

    #[test]
    fn parse_aborted_with_invalid_json_returns_error() {
        let err = parse_outcome("[CALYPSO:ABORTED]not-json").expect_err("should fail on bad JSON");
        assert!(
            err.to_string()
                .contains("invalid JSON after [CALYPSO:ABORTED]")
        );
    }

    #[test]
    fn parse_ok_with_missing_summary_returns_error() {
        let err = parse_outcome(r#"[CALYPSO:OK]{"artifact_refs":[]}"#)
            .expect_err("should fail on missing summary");
        assert!(err.to_string().contains("`summary`"));
    }

    #[test]
    fn parse_nok_with_missing_reason_returns_error() {
        let err = parse_outcome(r#"[CALYPSO:NOK]{"summary":"oops"}"#)
            .expect_err("should fail on missing reason");
        assert!(err.to_string().contains("`reason`"));
    }

    #[test]
    fn parse_aborted_with_missing_reason_returns_error() {
        let err =
            parse_outcome(r#"[CALYPSO:ABORTED]{}"#).expect_err("should fail on missing reason");
        assert!(err.to_string().contains("`reason`"));
    }

    #[test]
    fn parse_clarification_returns_none_when_no_marker() {
        let result = parse_clarification("no clarification here", "session-1");
        assert!(result.is_none());
    }

    #[test]
    fn parse_outcome_ok_with_artifact_refs() {
        let output = r#"[CALYPSO:OK]{"summary":"done","artifact_refs":["a.rs","b.rs"],"suggested_next_state":"deploy"}"#;
        let outcome = parse_outcome(output).expect("should parse");
        assert_eq!(
            outcome,
            ClaudeOutcome::Ok {
                summary: "done".to_string(),
                artifact_refs: vec!["a.rs".to_string(), "b.rs".to_string()],
                suggested_next_state: Some("deploy".to_string()),
            }
        );
    }
}
