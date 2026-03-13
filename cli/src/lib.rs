pub mod app;
pub mod codex;
pub mod doctor;
pub mod github;
pub mod runtime;
pub mod state;
pub mod template;
pub mod tui;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuildInfo<'a> {
    pub version: &'a str,
    pub git_hash: &'a str,
    pub build_time: &'a str,
    pub git_tags: &'a str,
}

pub fn render_version(info: BuildInfo<'_>) -> String {
    format!(
        "calypso-cli {}\nGit hash: {}\nBuild time: {}\nGit tags: {}",
        info.version, info.git_hash, info.build_time, info.git_tags
    )
}

pub fn render_help(info: BuildInfo<'_>) -> String {
    format!(
        "calypso-cli\nVersion: {}\nGit hash: {}\nBuild time: {}\nGit tags: {}\n\nUsage:\n  calypso-cli [OPTIONS] [COMMAND]\n\nCommands:\n  doctor      Check local Calypso prerequisites\n  status      Render the operator surface from a state file\n\nOptions:\n  -h, --help       Show this help output\n  -v, --version    Show build version information",
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

        assert!(output.contains("0.1.0+abc123"));
        assert!(output.contains("abc123"));
        assert!(output.contains("2026-03-13T12:00:00Z"));
        assert!(output.contains("v0.1.0"));
    }

    #[test]
    fn help_output_exposes_version_information() {
        let output = render_help(sample_info());

        assert!(output.contains("calypso-cli"));
        assert!(output.contains("Version: 0.1.0+abc123"));
        assert!(output.contains("Git hash: abc123"));
        assert!(output.contains("Commands:"));
    }
}
