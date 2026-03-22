pub mod app;
pub mod blueprint_workflows;
pub mod claude;
// FUTURE: #48 — Codex provider; re-enable when multi-vendor registry is implemented
// pub mod codex;
pub mod doctor;
pub mod driver;
pub mod error;
pub mod execution;
pub mod feature_start;
pub mod github;
pub mod headless;
pub mod headless_persist;
pub mod headless_sm;
pub mod headless_sm_driver;
pub mod init;
pub mod interpreter;
pub mod interpreter_scheduler;
pub mod keys;
pub mod operator_surface;
pub mod pinned_prompt;
pub mod policy;
pub mod pr_checklist;
pub mod report;
pub mod runtime;
pub mod signal;
pub mod sm_audit;
pub mod state;
pub mod telemetry;
pub mod template;
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
  calypso [OPTIONS] [path] [COMMAND]

Options:
  -p, --path <dir>    Project directory (default: current working directory)
  -h, --help          Show this help output
  -v, --version       Show build version information
  -v, -vv             Verbosity: -v = info, -vv = trace (default: debug)
  --json              Emit JSON-lines instead of human-readable text

Positional:
  [path]              Project directory (alternative to --path)

Commands:
  (none)              Drive the state machine for the project directory
  --step              Drive the state machine one step at a time
  doctor              Check local prerequisites and environment
  doctor --json       Output doctor results as JSON (exit 1 if any failing)
  doctor --fix <id>   Apply an available fix for a doctor check
  status              Render the feature status for the project directory
  state               Alias for `state status`
  state --json        Alias for `state status --json`
  state status        Show a human-readable summary of .calypso/repository-state.json
  state status --json Output state status as JSON
  state show          Print the current state file as raw JSON
  agents              Show active agent sessions
  agents --json       Output agent sessions as JSON
  init                Initialise a repository for Calypso
  init --reinit       Re-initialise an already-initialised repository
  init --json         Initialise and output results as JSON
  init --status       Show current init state machine progress
  init --step <step>  Manually trigger a specific init step
  init --refresh      Refresh/overwrite GitHub Actions workflow files
  init --org <org> --repo <name>
                      Create upstream GitHub remote during init
  feature-start <id> --worktree-base <path>
                      Create a feature branch, worktree, draft PR, and state file
  template validate   Validate the local workflow template
  workflows list      List all embedded blueprint workflow names
  workflows show <name>
                      Print the raw YAML for the named blueprint workflow
  workflows validate <name>
                      Parse the named workflow and report OK or the parse error
  webview             Start a local HTTP server (port 7373) with live state UI
  webview --port <N>  Start the webview server on a custom port
  keys list           List all managed keys with metadata
  keys list --json    List managed keys as JSON
  keys rotate <name>  Rotate the named key (generates new material, archives old)
  keys revoke <name>  Revoke the named key (marks it unusable)

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

    #[test]
    fn help_output_documents_json_flag() {
        let output = render_help(sample_info());

        assert!(output.contains("--json"), "missing --json flag");
    }
}
