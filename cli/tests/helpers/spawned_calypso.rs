//! `SpawnedCalypso` — spawns the `calypso-cli` binary as a child process in a
//! temporary working directory with an isolated state file and a configurable
//! `PATH`.
//!
//! Use [`SpawnedCalypso::builder`] to configure the invocation, then call
//! [`SpawnedCalypso::run`] to execute and obtain a [`CalypsoOutput`] for
//! assertions.

use std::path::PathBuf;
use std::process::Command;

use super::fake_claude::unique_temp_dir;

// ── Builder ────────────────────────────────────────────────────────────────────

/// Builder for a `calypso-cli` subprocess invocation.
pub struct SpawnedCalypsoBuilder {
    args: Vec<String>,
    prepend_path: Option<PathBuf>,
    state_file_json: Option<String>,
}

impl SpawnedCalypsoBuilder {
    fn new() -> Self {
        Self {
            args: vec![],
            prepend_path: None,
            state_file_json: None,
        }
    }

    /// Arguments to pass to `calypso-cli` (after the binary name).
    pub fn args(mut self, args: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    /// Prepend `dir` to `PATH` so a fake binary is found first.
    pub fn prepend_path(mut self, dir: PathBuf) -> Self {
        self.prepend_path = Some(dir);
        self
    }

    /// Write `json` to the state file in the temp working directory before
    /// spawning.  The path is injected into args via the placeholder
    /// `{STATE_FILE}` if present in the args list.
    pub fn state_file_json(mut self, json: impl Into<String>) -> Self {
        self.state_file_json = Some(json.into());
        self
    }

    /// Run the binary and return its output.
    pub fn run(self) -> CalypsoOutput {
        let work_dir = unique_temp_dir("calypso-e2e-workdir");
        // The `calypso run` subcommand resolves the state file from
        // `<cwd>/.calypso/repository-state.json`.  Write the fixture there so
        // the binary can find it without any extra flags.
        let calypso_dir = work_dir.join(".calypso");
        std::fs::create_dir_all(&calypso_dir).expect(".calypso dir should be created");
        let state_path = calypso_dir.join("repository-state.json");

        if let Some(json) = &self.state_file_json {
            std::fs::write(&state_path, json).expect("state file should be written");
        }

        // Substitute {STATE_FILE} placeholder in args with the actual path.
        let state_path_str = state_path.to_string_lossy().into_owned();
        let args: Vec<String> = self
            .args
            .iter()
            .map(|a| {
                if a == "{STATE_FILE}" {
                    state_path_str.clone()
                } else {
                    a.clone()
                }
            })
            .collect();

        // Build PATH.
        let current_path = std::env::var_os("PATH").unwrap_or_default();
        let path_val = if let Some(ref prepend) = self.prepend_path {
            let mut parts = vec![prepend.clone()];
            parts.extend(std::env::split_paths(&current_path));
            std::env::join_paths(parts).expect("PATH components should join")
        } else {
            current_path
        };

        let binary = env!("CARGO_BIN_EXE_calypso-cli");
        let output = Command::new(binary)
            .args(&args)
            .current_dir(&work_dir)
            .env("PATH", &path_val)
            .output()
            .expect("calypso-cli should spawn");

        let stdout = String::from_utf8(output.stdout).expect("stdout should be valid UTF-8");
        let stderr = String::from_utf8(output.stderr).expect("stderr should be valid UTF-8");
        let exit_code = output.status.code().unwrap_or(-1);

        CalypsoOutput {
            exit_code,
            stdout,
            stderr,
            state_path,
            work_dir,
        }
    }
}

// ── Output ─────────────────────────────────────────────────────────────────────

/// The result of a `calypso-cli` subprocess invocation.
#[allow(dead_code)]
pub struct CalypsoOutput {
    pub exit_code: i32,
    pub stdout: String,
    pub stderr: String,
    /// Path to the state file in the temp working directory.
    pub state_path: PathBuf,
    /// The temp working directory (kept alive so callers can inspect it).
    pub work_dir: PathBuf,
}

impl CalypsoOutput {
    /// Load and return the state file JSON, or `None` if it doesn't exist.
    #[allow(dead_code)]
    pub fn read_state_json(&self) -> Option<String> {
        std::fs::read_to_string(&self.state_path).ok()
    }
}

// ── Entry point ────────────────────────────────────────────────────────────────

/// Returns a builder for spawning `calypso-cli`.
pub fn spawned_calypso() -> SpawnedCalypsoBuilder {
    SpawnedCalypsoBuilder::new()
}
