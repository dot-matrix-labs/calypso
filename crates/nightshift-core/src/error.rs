use std::collections::BTreeMap;
use std::fmt;
use std::sync::{LazyLock, RwLock};

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Redaction registry
// ---------------------------------------------------------------------------

static REDACTION_REGISTRY: LazyLock<RwLock<Vec<String>>> =
    LazyLock::new(|| RwLock::new(Vec::new()));

/// Register a secret value that should be scrubbed from all output.
///
/// Values are stored in-process only and are never serialized.
pub fn register_secret(value: impl Into<String>) {
    let value = value.into();
    if value.is_empty() {
        return;
    }
    if let Ok(mut registry) = REDACTION_REGISTRY.write()
        && !registry.contains(&value)
    {
        registry.push(value);
    }
}

/// Scrub known secret patterns from `input` and return the sanitized string.
///
/// Patterns removed:
/// - `Bearer <token>` — HTTP authorization header values
/// - GitHub PAT prefixes: `ghp_…` and `github_pat_…`
/// - Generic hex strings of 40 or more characters
/// - Any value previously registered via [`register_secret`]
pub fn redact(input: &str) -> String {
    let mut output = input.to_string();

    // Registered secrets first (longest-first ordering avoids partial matches).
    if let Ok(registry) = REDACTION_REGISTRY.read() {
        let mut secrets: Vec<&String> = registry.iter().collect();
        secrets.sort_by_key(|s| std::cmp::Reverse(s.len()));
        for secret in secrets {
            output = output.replace(secret.as_str(), "[REDACTED]");
        }
    }

    output = scrub_bearer_tokens(&output);
    output = scrub_prefixed_tokens(&output, "ghp_", is_ascii_alnum, 10);
    output = scrub_prefixed_tokens(&output, "github_pat_", is_github_pat_char, 10);
    output = scrub_long_hex_tokens(&output);

    output
}

fn scrub_bearer_tokens(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if is_bearer_prefix_at(input, index) {
            let prefix_end = index + "Bearer".len();
            let mut cursor = prefix_end;
            while let Some(ch) = input[cursor..].chars().next() {
                if !ch.is_ascii_whitespace() {
                    break;
                }
                cursor += ch.len_utf8();
            }
            let token_start = cursor;
            while let Some(ch) = input[cursor..].chars().next() {
                if !is_bearer_token_char(ch) {
                    break;
                }
                cursor += ch.len_utf8();
            }
            if cursor > token_start {
                output.push_str(&input[..index]);
                output.push_str("Bearer [REDACTED]");
                output.push_str(&scrub_bearer_tokens(&input[cursor..]));
                return output;
            }
        }
        let ch = input[index..]
            .chars()
            .next()
            .expect("valid utf-8 boundary while redacting");
        output.push(ch);
        index += ch.len_utf8();
    }
    output
}

fn scrub_prefixed_tokens(
    input: &str,
    prefix: &str,
    allowed: fn(char) -> bool,
    min_len: usize,
) -> String {
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        if input[index..].starts_with(prefix) {
            let mut cursor = index + prefix.len();
            let mut len = 0usize;
            while let Some(ch) = input[cursor..].chars().next() {
                if !allowed(ch) {
                    break;
                }
                cursor += ch.len_utf8();
                len += 1;
            }
            if len >= min_len {
                output.push_str(&input[..index]);
                output.push_str("[REDACTED]");
                output.push_str(&scrub_prefixed_tokens(
                    &input[cursor..],
                    prefix,
                    allowed,
                    min_len,
                ));
                return output;
            }
        }
        let ch = input[index..]
            .chars()
            .next()
            .expect("valid utf-8 boundary while redacting");
        output.push(ch);
        index += ch.len_utf8();
    }
    output
}

fn scrub_long_hex_tokens(input: &str) -> String {
    let mut output = String::with_capacity(input.len());
    let mut index = 0;
    while index < input.len() {
        let Some(ch) = input[index..].chars().next() else {
            break;
        };
        if is_hex_char(ch) && is_hex_boundary(input, index, true) {
            let start = index;
            let mut cursor = index + ch.len_utf8();
            let mut count = 1usize;
            while let Some(next) = input[cursor..].chars().next() {
                if !is_hex_char(next) {
                    break;
                }
                cursor += next.len_utf8();
                count += 1;
            }
            if count >= 40 && is_hex_boundary(input, cursor, false) {
                output.push_str(&input[..start]);
                output.push_str("[REDACTED]");
                output.push_str(&scrub_long_hex_tokens(&input[cursor..]));
                return output;
            }
        }
        output.push(ch);
        index += ch.len_utf8();
    }
    output
}

fn is_bearer_prefix_at(input: &str, index: usize) -> bool {
    let prefix = "Bearer";
    let Some(segment) = input.get(index..index + prefix.len()) else {
        return false;
    };
    if !segment.eq_ignore_ascii_case(prefix) {
        return false;
    }
    if index > 0
        && let Some(prev) = input[..index].chars().next_back()
        && prev.is_ascii_alphanumeric()
    {
        return false;
    }
    input[index + prefix.len()..]
        .chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_whitespace())
}

fn is_bearer_token_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || matches!(ch, '-' | '.' | '_' | '~' | '+' | '/' | '=')
}

fn is_ascii_alnum(ch: char) -> bool {
    ch.is_ascii_alphanumeric()
}

fn is_github_pat_char(ch: char) -> bool {
    ch.is_ascii_alphanumeric() || ch == '_'
}

fn is_hex_char(ch: char) -> bool {
    ch.is_ascii_hexdigit()
}

fn is_hex_boundary(input: &str, index: usize, start: bool) -> bool {
    if start {
        return input[..index]
            .chars()
            .next_back()
            .is_none_or(|ch| !is_hex_char(ch));
    }
    input[index..]
        .chars()
        .next()
        .is_none_or(|ch| !is_hex_char(ch))
}

// ---------------------------------------------------------------------------
// Recoverability
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Recoverability {
    /// The operation may succeed if retried without any change.
    Recoverable,
    /// The user must take an action (e.g. re-authenticate, provide input) before retrying.
    UserActionRequired,
    /// The error represents an unrecoverable failure; the process should stop.
    Unrecoverable,
}

impl fmt::Display for Recoverability {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Recoverable => f.write_str("recoverable"),
            Self::UserActionRequired => f.write_str("user-action-required"),
            Self::Unrecoverable => f.write_str("unrecoverable"),
        }
    }
}

// ---------------------------------------------------------------------------
// Error codes
// ---------------------------------------------------------------------------

/// Machine-readable error code slugs.
pub mod codes {
    pub const PROVIDER_AUTH: &str = "provider_auth";
    pub const SUBPROCESS_SPAWN: &str = "subprocess_spawn";
    pub const MALFORMED_PROVIDER_OUTPUT: &str = "malformed_provider_output";
    pub const TRANSPORT: &str = "transport";
    pub const GIT: &str = "git";
    pub const GITHUB_API: &str = "github_api";
    pub const INVALID_STATE_TRANSITION: &str = "invalid_state_transition";
    pub const MISSING_CLARIFICATION: &str = "missing_clarification";
    pub const STATE_CORRUPTION: &str = "state_corruption";
    pub const STUDIO_LIFECYCLE: &str = "studio_lifecycle";
    pub const REPO_ROOT_NOT_FOUND: &str = "repo_root_not_found";
    pub const STATE_LOAD: &str = "state_load";
    pub const WORKFLOW_NOT_FOUND: &str = "workflow_not_found";
}

// ---------------------------------------------------------------------------
// CalypsoError
// ---------------------------------------------------------------------------

/// A structured, serializable error that carries a machine-readable code,
/// human-readable message, recoverability classification, and optional
/// key-value context.
///
/// Secret values must be redacted before being stored in `message` or
/// `context`. Use [`redact`] when constructing errors from external output.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalypsoError {
    pub code: String,
    pub message: String,
    pub recoverability: Recoverability,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub context: BTreeMap<String, String>,
}

impl CalypsoError {
    /// Create a new error.
    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        recoverability: Recoverability,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            recoverability,
            context: BTreeMap::new(),
        }
    }

    /// Attach a context key-value pair.
    pub fn with_context(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.context.insert(key.into(), value.into());
        self
    }

    /// Serialize to a compact JSON string.
    pub fn to_json(&self) -> String {
        serde_json::to_string(self).expect("CalypsoError is always serializable")
    }

    /// Write this error to stderr in JSON form.
    pub fn emit_stderr(&self) {
        eprintln!("{}", self.to_json());
    }

    // -----------------------------------------------------------------------
    // Constructors per category
    // -----------------------------------------------------------------------

    pub fn provider_auth(message: impl Into<String>) -> Self {
        Self::new(
            codes::PROVIDER_AUTH,
            message,
            Recoverability::UserActionRequired,
        )
    }

    pub fn subprocess_spawn(message: impl Into<String>) -> Self {
        Self::new(
            codes::SUBPROCESS_SPAWN,
            message,
            Recoverability::Unrecoverable,
        )
    }

    pub fn malformed_provider_output(message: impl Into<String>) -> Self {
        Self::new(
            codes::MALFORMED_PROVIDER_OUTPUT,
            message,
            Recoverability::Recoverable,
        )
    }

    pub fn transport(message: impl Into<String>) -> Self {
        Self::new(codes::TRANSPORT, message, Recoverability::Recoverable)
    }

    pub fn git(message: impl Into<String>) -> Self {
        Self::new(codes::GIT, message, Recoverability::Unrecoverable)
    }

    pub fn github_api(message: impl Into<String>) -> Self {
        Self::new(codes::GITHUB_API, message, Recoverability::Recoverable)
    }

    pub fn invalid_state_transition(message: impl Into<String>) -> Self {
        Self::new(
            codes::INVALID_STATE_TRANSITION,
            message,
            Recoverability::UserActionRequired,
        )
    }

    pub fn missing_clarification(message: impl Into<String>) -> Self {
        Self::new(
            codes::MISSING_CLARIFICATION,
            message,
            Recoverability::UserActionRequired,
        )
    }

    pub fn state_corruption(message: impl Into<String>) -> Self {
        Self::new(
            codes::STATE_CORRUPTION,
            message,
            Recoverability::Unrecoverable,
        )
    }

    pub fn studio_lifecycle(message: impl Into<String>) -> Self {
        Self::new(
            codes::STUDIO_LIFECYCLE,
            message,
            Recoverability::Unrecoverable,
        )
    }

    pub fn repo_root_not_found(message: impl Into<String>) -> Self {
        Self::new(
            codes::REPO_ROOT_NOT_FOUND,
            message,
            Recoverability::UserActionRequired,
        )
    }

    pub fn state_load(message: impl Into<String>) -> Self {
        Self::new(codes::STATE_LOAD, message, Recoverability::Unrecoverable)
    }

    pub fn workflow_not_found(message: impl Into<String>) -> Self {
        Self::new(
            codes::WORKFLOW_NOT_FOUND,
            message,
            Recoverability::UserActionRequired,
        )
    }
}

impl fmt::Display for CalypsoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for CalypsoError {}
