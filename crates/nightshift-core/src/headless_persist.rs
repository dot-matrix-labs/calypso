//! Persistence layer for interrupted headless runs.
//!
//! When a headless state-machine run is interrupted (SIGINT / SIGTERM),
//! hits its step limit, or encounters a fatal error, the driver can write a
//! small JSON snapshot to `.calypso/headless-state.json` (or a caller-supplied
//! path).  On the next run the driver reads that snapshot, re-enters the
//! recorded state, and logs a clear resumption notice to the terminal.
//!
//! After a clean `Terminal` exit the snapshot is deleted so that subsequent
//! runs start fresh from the state machine's `initial_state`.
//!
//! # Format
//!
//! ```json
//! {
//!   "current_state": "scan",
//!   "iteration": 7,
//!   "exit_reason": "interrupted",
//!   "exit_detail": "SIGINT",
//!   "timestamp": "2026-03-22T06:00:00Z"
//! }
//! ```
//!
//! `exit_detail` is optional and carries the signal name, error message, or
//! `null` when the run stopped for a reason that needs no extra detail.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

// ── Exit reason tag ───────────────────────────────────────────────────────────

/// A serialisable, human-readable tag for the cause of an interrupted run.
///
/// This is a simplified projection of [`crate::headless_sm_driver::ExitReason`]
/// that can be round-tripped through JSON without embedding the full driver
/// type hierarchy in the persistence format.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExitReasonTag {
    /// The driver stopped because a `terminal` state was reached.  A clean
    /// exit — the snapshot is deleted after writing a terminal entry.
    Terminal,
    /// The driver stopped because a shutdown signal was received.
    Interrupted,
    /// The driver stopped because of a fatal execution error.
    Error,
    /// The driver stopped because the configured step limit was reached.
    StepLimitReached,
}

impl ExitReasonTag {
    /// Human-readable label for log messages.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Terminal => "terminal",
            Self::Interrupted => "interrupted",
            Self::Error => "error",
            Self::StepLimitReached => "step_limit_reached",
        }
    }
}

impl std::fmt::Display for ExitReasonTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

// ── Persisted run state ───────────────────────────────────────────────────────

/// A JSON snapshot written to disk when a headless run stops non-terminally.
///
/// Reading this file on startup lets the driver resume from the correct state
/// and explain what happened in the previous run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadlessRunState {
    /// The state the machine was in (or about to enter) when it stopped.
    pub current_state: String,

    /// Number of loop iterations completed at the time of stopping.
    pub iteration: usize,

    /// Why the run stopped.
    pub exit_reason: ExitReasonTag,

    /// Optional detail: signal name for `Interrupted`, error message for
    /// `Error`, or step count for `StepLimitReached`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub exit_detail: Option<String>,

    /// RFC 3339 / ISO 8601 timestamp of when the snapshot was written.
    pub timestamp: String,
}

impl HeadlessRunState {
    /// Write this snapshot to `path`, creating parent directories as needed.
    ///
    /// # Errors
    ///
    /// Returns an `Err` if the directory cannot be created or the file cannot
    /// be written.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        std::fs::write(path, json)
    }

    /// Load a snapshot from `path`.
    ///
    /// Returns `Ok(None)` when the file does not exist (fresh run).
    /// Returns `Ok(Some(...))` when the file exists and parses correctly.
    /// Returns `Err` when the file exists but cannot be read or parsed.
    pub fn load(path: &Path) -> std::io::Result<Option<Self>> {
        if !path.exists() {
            return Ok(None);
        }
        let bytes = std::fs::read(path)?;
        let state: Self = serde_json::from_slice(&bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        Ok(Some(state))
    }

    /// Delete the snapshot file if it exists.
    ///
    /// Called after a clean `Terminal` exit so the next run starts fresh.
    /// A missing file is not an error.
    pub fn clear(path: &Path) -> std::io::Result<()> {
        if path.exists() {
            std::fs::remove_file(path)?;
        }
        Ok(())
    }

    /// Return the default snapshot path inside a repository root.
    ///
    /// The file lives at `<repo_root>/.calypso/headless-state.json`.
    pub fn default_path(repo_root: &Path) -> PathBuf {
        repo_root.join(".calypso").join("headless-state.json")
    }
}

/// Build an RFC 3339 timestamp string for the current UTC moment.
///
/// Uses [`chrono`] when available, falls back to a Unix-second string when
/// the time cannot be determined.
pub fn now_rfc3339() -> String {
    chrono::Utc::now().to_rfc3339()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_state(tag: ExitReasonTag) -> HeadlessRunState {
        HeadlessRunState {
            current_state: "scan".to_string(),
            iteration: 7,
            exit_reason: tag,
            exit_detail: None,
            timestamp: "2026-03-22T06:00:00Z".to_string(),
        }
    }

    // ── ExitReasonTag ─────────────────────────────────────────────────────────

    #[test]
    fn exit_reason_tag_as_str_covers_all_variants() {
        assert_eq!(ExitReasonTag::Terminal.as_str(), "terminal");
        assert_eq!(ExitReasonTag::Interrupted.as_str(), "interrupted");
        assert_eq!(ExitReasonTag::Error.as_str(), "error");
        assert_eq!(
            ExitReasonTag::StepLimitReached.as_str(),
            "step_limit_reached"
        );
    }

    #[test]
    fn exit_reason_tag_display_matches_as_str() {
        for tag in [
            ExitReasonTag::Terminal,
            ExitReasonTag::Interrupted,
            ExitReasonTag::Error,
            ExitReasonTag::StepLimitReached,
        ] {
            assert_eq!(format!("{tag}"), tag.as_str());
        }
    }

    #[test]
    fn exit_reason_tag_serde_round_trips() {
        for tag in [
            ExitReasonTag::Terminal,
            ExitReasonTag::Interrupted,
            ExitReasonTag::Error,
            ExitReasonTag::StepLimitReached,
        ] {
            let json = serde_json::to_string(&tag).unwrap();
            let decoded: ExitReasonTag = serde_json::from_str(&json).unwrap();
            assert_eq!(decoded, tag);
        }
    }

    #[test]
    fn exit_reason_tag_serde_uses_snake_case() {
        let json = serde_json::to_string(&ExitReasonTag::StepLimitReached).unwrap();
        assert_eq!(json, "\"step_limit_reached\"");
    }

    // ── HeadlessRunState ──────────────────────────────────────────────────────

    #[test]
    fn headless_run_state_serde_round_trips() {
        let original = HeadlessRunState {
            current_state: "check".to_string(),
            iteration: 3,
            exit_reason: ExitReasonTag::Interrupted,
            exit_detail: Some("SIGINT".to_string()),
            timestamp: "2026-03-22T06:00:00Z".to_string(),
        };

        let json = serde_json::to_string_pretty(&original).unwrap();
        let decoded: HeadlessRunState = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded, original);
    }

    #[test]
    fn exit_detail_is_omitted_when_none() {
        let state = sample_state(ExitReasonTag::Terminal);
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            !json.contains("exit_detail"),
            "exit_detail should be absent when None: {json}"
        );
    }

    #[test]
    fn exit_detail_is_present_when_some() {
        let state = HeadlessRunState {
            exit_detail: Some("SIGTERM".to_string()),
            ..sample_state(ExitReasonTag::Interrupted)
        };
        let json = serde_json::to_string(&state).unwrap();
        assert!(
            json.contains("exit_detail"),
            "exit_detail should be present when Some: {json}"
        );
        assert!(json.contains("SIGTERM"));
    }

    // ── save / load / clear ───────────────────────────────────────────────────

    fn tmp_path(name: &str) -> PathBuf {
        std::env::temp_dir()
            .join(format!("calypso-headless-persist-test-{name}"))
            .join("headless-state.json")
    }

    #[test]
    fn load_returns_none_when_file_missing() {
        let path = tmp_path("missing");
        let _ = HeadlessRunState::clear(&path);
        let result = HeadlessRunState::load(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = tmp_path("save-load");
        let _ = HeadlessRunState::clear(&path);

        let state = HeadlessRunState {
            current_state: "retry".to_string(),
            iteration: 12,
            exit_reason: ExitReasonTag::Interrupted,
            exit_detail: Some("SIGINT".to_string()),
            timestamp: "2026-03-22T06:30:00Z".to_string(),
        };

        state.save(&path).expect("save should succeed");
        let loaded = HeadlessRunState::load(&path)
            .expect("load should succeed")
            .expect("file should exist");

        assert_eq!(loaded, state);

        let _ = HeadlessRunState::clear(&path);
    }

    #[test]
    fn save_creates_parent_directories() {
        let path = std::env::temp_dir()
            .join("calypso-persist-mkdir-test")
            .join("deep")
            .join("nested")
            .join("headless-state.json");

        let _ = HeadlessRunState::clear(&path);
        if let Some(parent) = path.parent() {
            let _ = std::fs::remove_dir_all(parent);
        }

        let state = sample_state(ExitReasonTag::Error);
        state.save(&path).expect("save should create parent dirs");
        assert!(path.exists());

        let _ = HeadlessRunState::clear(&path);
    }

    #[test]
    fn clear_is_idempotent_when_file_missing() {
        let path = tmp_path("clear-missing");
        // Should not error if file doesn't exist
        HeadlessRunState::clear(&path).expect("clear should succeed on missing file");
    }

    #[test]
    fn clear_removes_existing_file() {
        let path = tmp_path("clear-existing");
        let state = sample_state(ExitReasonTag::Terminal);
        state.save(&path).unwrap();
        assert!(path.exists());

        HeadlessRunState::clear(&path).expect("clear should succeed");
        assert!(!path.exists());
    }

    #[test]
    fn load_error_on_corrupt_file() {
        let path = tmp_path("corrupt");
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&path, b"{ not valid json }").unwrap();

        let result = HeadlessRunState::load(&path);
        assert!(result.is_err(), "should return Err for corrupt JSON");

        let _ = HeadlessRunState::clear(&path);
    }

    #[test]
    fn default_path_returns_calypso_subdir() {
        let root = Path::new("/tmp/my-project");
        let path = HeadlessRunState::default_path(root);
        assert_eq!(
            path,
            PathBuf::from("/tmp/my-project/.calypso/headless-state.json")
        );
    }

    #[test]
    fn now_rfc3339_produces_nonempty_string() {
        let ts = now_rfc3339();
        assert!(!ts.is_empty());
        // Should contain a 'T' separator — basic RFC 3339 sanity
        assert!(ts.contains('T'), "expected 'T' in RFC 3339 timestamp: {ts}");
    }

    // ── Debug / Clone ──────────────────────────────────────────────────────────

    #[test]
    fn headless_run_state_derives_debug_and_clone() {
        let state = sample_state(ExitReasonTag::Error);
        let cloned = state.clone();
        assert_eq!(state, cloned);
        let debug = format!("{state:?}");
        assert!(debug.contains("HeadlessRunState"));
    }
}
