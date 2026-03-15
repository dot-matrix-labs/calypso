pub mod app;
pub mod claude;
// FUTURE: #48 — Codex provider; re-enable when multi-vendor registry is implemented
// pub mod codex;
pub mod doctor;
pub mod driver;
pub mod error;
pub mod execution;
pub mod feature_start;
pub mod github;
pub mod init;
pub mod policy;
pub mod pr_checklist;
pub mod report;
pub mod runtime;
pub mod state;
pub mod telemetry;
pub mod template;
pub mod tui;
pub mod workflows;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildInfo<'a> {
    pub version: &'a str,
    pub git_hash: &'a str,
    pub build_time: &'a str,
    pub git_tags: &'a str,
}

pub fn render_version(info: BuildInfo<'_>) -> String {
    format!(
        "calypso-cli {} git:{} built:{} tags:{}",
        info.version, info.git_hash, info.build_time, info.git_tags
    )
}

pub fn render_help(info: BuildInfo<'_>) -> String {
    format!(
        "\
calypso-cli {}

Usage:
  calypso [OPTIONS] [COMMAND]

Options:
  -p, --path <dir>    Project directory (default: current working directory)
  -h, --help          Show this help output
  -v, --version       Show build version information

Commands:
  (none)              Drive the state machine for the project directory
  --step              Drive the state machine one step at a time
  doctor              Check local prerequisites and environment
  doctor --json       Output doctor results as JSON (exit 1 if any failing)
  doctor --fix <id>   Apply an available fix for a doctor check
  status              Render the feature status for the project directory
  state status        Show a human-readable summary of .calypso/state.json
  state status --json Output state status as JSON
  state show          Print the current state file as raw JSON
  agents              Show active agent sessions
  agents --json       Output agent sessions as JSON
  watch               Open the interactive operator surface (live TUI)
  init                Initialise a repository for Calypso
  init --reinit       Re-initialise an already-initialised repository
  feature-start <id> --worktree-base <path>
                      Create a feature branch, worktree, draft PR, and state file
  template validate   Validate the local workflow template

Git hash: {}  Built: {}  Tags: {}",
        info.version, info.git_hash, info.build_time, info.git_tags
    )
}

#[cfg(test)]
mod tests {
    use super::{BuildInfo, render_help, render_version};

    fn sample_info() -> BuildInfo<'static> {
        BuildInfo {
            version: "0.1.0+abc123",
            git_hash: "abc123",
            build_time: "2026-03-13T12:00:00Z",
            git_tags: "v0.1.0",
        }
    }

    #[test]
    fn version_output_contains_required_build_metadata() {
        let output = render_version(sample_info());

        assert!(output.contains("0.1.0+abc123"), "missing semver+hash");
        assert!(output.contains("abc123"), "missing git hash");
        assert!(output.contains("2026-03-13T12:00:00Z"), "missing timestamp");
        assert!(output.contains("v0.1.0"), "missing git tag");
    }

    #[test]
    fn version_output_is_a_single_line() {
        let output = render_version(sample_info());
        assert_eq!(output.lines().count(), 1, "version output must be one line");
    }

    #[test]
    fn help_output_exposes_version_information() {
        let output = render_help(sample_info());

        assert!(output.contains("calypso-cli"));
        assert!(output.contains("0.1.0+abc123"));
        assert!(output.contains("Git hash: abc123"));
        assert!(output.contains("Commands:"));
        assert!(output.contains("--path"));
        assert!(output.contains("-h, --help"));
    }
}
