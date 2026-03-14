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

    // Bearer tokens: `Bearer <value>` where value runs to whitespace / EOL.
    let bearer_re = regex_lite::Regex::new(r"(?i)Bearer\s+[A-Za-z0-9\-._~+/]+=*").unwrap();
    output = bearer_re
        .replace_all(&output, "Bearer [REDACTED]")
        .into_owned();

    // GitHub PATs: ghp_ and github_pat_ prefixes.
    let ghp_re = regex_lite::Regex::new(r"ghp_[A-Za-z0-9]{10,}").unwrap();
    output = ghp_re.replace_all(&output, "[REDACTED]").into_owned();

    let github_pat_re = regex_lite::Regex::new(r"github_pat_[A-Za-z0-9_]{10,}").unwrap();
    output = github_pat_re
        .replace_all(&output, "[REDACTED]")
        .into_owned();

    // Generic hex strings of 40+ characters (e.g. raw API keys / SHA tokens).
    let hex_re = regex_lite::Regex::new(r"\b[0-9a-fA-F]{40,}\b").unwrap();
    output = hex_re.replace_all(&output, "[REDACTED]").into_owned();

    output
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
}

impl fmt::Display for CalypsoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)
    }
}

impl std::error::Error for CalypsoError {}
