//! `FakeClaude` — writes a configurable shell script acting as a `claude`
//! binary stub and prepends the directory containing it to `PATH`.
//!
//! Use [`FakeClaude::builder`] to configure the outcome, then call
//! [`FakeClaude::install`] to write the script and obtain a [`FakeClaude`]
//! guard.  When the guard is dropped the original `PATH` is restored and the
//! temp directory is removed.

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

// ── Outcome payload ────────────────────────────────────────────────────────────

/// The outcome the fake `claude` binary should emit on stdout.
#[derive(Debug, Clone)]
pub enum FakeOutcome {
    Ok {
        summary: String,
    },
    Nok {
        summary: String,
        reason: String,
    },
    Aborted {
        reason: String,
    },
    /// Emits only a `[CALYPSO:CLARIFICATION]` line — no terminal outcome marker.
    /// Calypso should detect this and surface it as an operator input request.
    Clarification {
        question: String,
    },
}

impl FakeOutcome {
    fn as_marker_line(&self) -> String {
        match self {
            FakeOutcome::Ok { summary } => {
                format!(r#"[CALYPSO:OK]{{"summary":"{summary}"}}"#)
            }
            FakeOutcome::Nok { summary, reason } => {
                format!(r#"[CALYPSO:NOK]{{"summary":"{summary}","reason":"{reason}"}}"#)
            }
            FakeOutcome::Aborted { reason } => {
                format!(r#"[CALYPSO:ABORTED]{{"reason":"{reason}"}}"#)
            }
            FakeOutcome::Clarification { question } => {
                format!(r#"[CALYPSO:CLARIFICATION]{question}"#)
            }
        }
    }
}

// ── Builder ────────────────────────────────────────────────────────────────────

/// Builder for [`FakeClaude`].
pub struct FakeClaudeBuilder {
    outcome: FakeOutcome,
    exit_code: i32,
}

impl FakeClaudeBuilder {
    fn new() -> Self {
        Self {
            outcome: FakeOutcome::Ok {
                summary: "fake claude ok".to_string(),
            },
            exit_code: 0,
        }
    }

    /// Set the outcome payload emitted on stdout.
    pub fn outcome(mut self, outcome: FakeOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// Set the exit code of the fake binary (default: 0).
    pub fn exit_code(mut self, code: i32) -> Self {
        self.exit_code = code;
        self
    }

    /// Write the fake binary to a temp dir and prepend it to `PATH`.
    ///
    /// Returns a [`FakeClaude`] guard that restores `PATH` on drop.
    pub fn install(self) -> FakeClaude {
        let dir = unique_temp_dir("calypso-fake-claude");
        let marker_line = self.outcome.as_marker_line();
        let script = format!(
            "#!/bin/sh\nprintf '%s\\n' '{}'\nexit {}\n",
            marker_line, self.exit_code
        );
        write_executable(&dir, "claude", &script);

        let old_path = std::env::var_os("PATH");
        let mut parts = vec![dir.clone()];
        if let Some(ref existing) = old_path {
            parts.extend(std::env::split_paths(existing));
        }
        let new_path =
            std::env::join_paths(parts).expect("PATH components should join without error");
        // SAFETY: single-threaded test helpers; tests serialise PATH access via
        // the PATH_MUTEX in tests that use FakeClaude.
        unsafe { std::env::set_var("PATH", &new_path) };

        FakeClaude {
            dir: dir.clone(),
            binary_path: dir.join("claude"),
            old_path,
        }
    }
}

// ── Guard ──────────────────────────────────────────────────────────────────────

/// A live fake-claude installation.  Drop to restore `PATH` and remove the
/// temp directory.
pub struct FakeClaude {
    pub dir: PathBuf,
    /// Absolute path to the fake `claude` script — use this as `ClaudeConfig::binary`.
    pub binary_path: PathBuf,
    old_path: Option<std::ffi::OsString>,
}

impl FakeClaude {
    /// Return a builder for configuring the fake binary.
    pub fn builder() -> FakeClaudeBuilder {
        FakeClaudeBuilder::new()
    }
}

impl Drop for FakeClaude {
    fn drop(&mut self) {
        match self.old_path.take() {
            Some(p) => unsafe { std::env::set_var("PATH", p) },
            None => unsafe { std::env::remove_var("PATH") },
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

// ── Internal helpers ───────────────────────────────────────────────────────────

pub(crate) fn unique_temp_dir(prefix: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{prefix}-{ts}"));
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

pub(crate) fn write_executable(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let script = dir.join(name);
    fs::write(&script, contents).expect("script should be written");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script)
            .expect("script metadata should be readable")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("script permissions should be set");
    }
    script
}
