use std::process::Command;

use serde::Deserialize;

use crate::state::{BuiltinEvidence, PullRequestRef};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GithubCheckId {
    PullRequestExists,
    PullRequestMerged,
    PullRequestChecksGreen,
}

impl GithubCheckId {
    fn builtin_key(self) -> &'static str {
        match self {
            GithubCheckId::PullRequestExists => "builtin.github.pr_exists",
            GithubCheckId::PullRequestMerged => "builtin.github.pr_merged",
            GithubCheckId::PullRequestChecksGreen => "builtin.github.pr_checks_green",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GithubStatus {
    Passing,
    Failing,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubCheck {
    pub id: GithubCheckId,
    pub status: GithubStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubReport {
    pub checks: Vec<GithubCheck>,
}

impl GithubReport {
    pub fn to_builtin_evidence(&self) -> BuiltinEvidence {
        self.checks
            .iter()
            .fold(BuiltinEvidence::new(), |evidence, check| {
                evidence.with_result(
                    check.id.builtin_key(),
                    check.status == GithubStatus::Passing,
                )
            })
    }
}

pub trait GithubEnvironment {
    fn pr_exists(&self, pull_request: &PullRequestRef) -> bool;
    fn pr_merged(&self, pull_request: &PullRequestRef) -> bool;
    fn checks_green(&self, pull_request: &PullRequestRef) -> bool;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PullRequestStatus {
    pub exists: bool,
    pub merged: bool,
    pub checks_green: bool,
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HostGithubEnvironment;

impl GithubEnvironment for HostGithubEnvironment {
    fn pr_exists(&self, pull_request: &PullRequestRef) -> bool {
        fetch_pull_request_status(pull_request).is_some()
    }

    fn pr_merged(&self, pull_request: &PullRequestRef) -> bool {
        fetch_pull_request_status(pull_request).is_some_and(|status| status.merged)
    }

    fn checks_green(&self, pull_request: &PullRequestRef) -> bool {
        fetch_pull_request_status(pull_request).is_some_and(|status| status.checks_green)
    }
}

pub fn collect_github_report(
    environment: &impl GithubEnvironment,
    pull_request: &PullRequestRef,
) -> GithubReport {
    GithubReport {
        checks: vec![
            GithubCheck {
                id: GithubCheckId::PullRequestExists,
                status: status_from_bool(environment.pr_exists(pull_request)),
            },
            GithubCheck {
                id: GithubCheckId::PullRequestMerged,
                status: status_from_bool(environment.pr_merged(pull_request)),
            },
            GithubCheck {
                id: GithubCheckId::PullRequestChecksGreen,
                status: status_from_bool(environment.checks_green(pull_request)),
            },
        ],
    }
}

fn status_from_bool(passing: bool) -> GithubStatus {
    if passing {
        GithubStatus::Passing
    } else {
        GithubStatus::Failing
    }
}

pub fn parse_pull_request_view_json(json: &str) -> Result<PullRequestStatus, serde_json::Error> {
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PrView {
        merged_at: Option<String>,
        state: String,
        #[serde(default)]
        status_check_rollup: Vec<CheckRollup>,
    }

    #[derive(Deserialize, Default)]
    struct CheckRollup {
        status: Option<String>,
        conclusion: Option<String>,
    }

    let view: PrView = serde_json::from_str(json)?;
    let checks_green = view.status_check_rollup.iter().all(|check| {
        matches!(check.status.as_deref(), None | Some("COMPLETED"))
            && matches!(
                check.conclusion.as_deref(),
                None | Some("SUCCESS") | Some("NEUTRAL") | Some("SKIPPED")
            )
    });

    Ok(PullRequestStatus {
        exists: true,
        merged: view.state == "MERGED" || view.merged_at.is_some(),
        checks_green,
    })
}

fn fetch_pull_request_status(pull_request: &PullRequestRef) -> Option<PullRequestStatus> {
    let mut command = Command::new("gh");
    command.args([
        "pr",
        "view",
        &pull_request.number.to_string(),
        "--json",
        "state,mergedAt,statusCheckRollup",
    ]);

    fetch_pull_request_status_with_command(&mut command)
}

fn fetch_pull_request_status_with_command(command: &mut Command) -> Option<PullRequestStatus> {
    let output = command.output().ok()?;

    if !output.status.success() {
        return None;
    }

    parse_pull_request_view_json(&String::from_utf8_lossy(&output.stdout)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::{LazyLock, Mutex};

    static PATH_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

    fn make_temp_dir(name: &str) -> PathBuf {
        let unique = format!(
            "{name}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("system clock should be after epoch")
                .as_nanos()
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).expect("temp dir should be created");
        path
    }

    fn with_fake_gh(script: &str, test: impl FnOnce()) {
        let _guard = PATH_LOCK.lock().expect("path lock should be available");
        let temp_dir = make_temp_dir("calypso-cli-github-tests");
        let gh_path = temp_dir.join("gh");
        std::fs::write(&gh_path, format!("#!/bin/sh\n{script}\n"))
            .expect("fake gh script should be written");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = std::fs::metadata(&gh_path)
                .expect("fake gh metadata should exist")
                .permissions();
            permissions.set_mode(0o755);
            std::fs::set_permissions(&gh_path, permissions).expect("fake gh should be executable");
        }

        let original_path = std::env::var_os("PATH").unwrap_or_default();
        let mut search_path = std::ffi::OsString::new();
        search_path.push(&temp_dir);
        search_path.push(std::ffi::OsStr::new(":"));
        search_path.push(&original_path);

        unsafe {
            std::env::set_var("PATH", &search_path);
        }

        test();

        unsafe {
            std::env::set_var("PATH", original_path);
        }
        std::fs::remove_dir_all(temp_dir).expect("temp dir should be removed");
    }

    #[test]
    fn fetch_pull_request_status_returns_none_for_failed_gh_command() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exit 1"]);

        assert_eq!(fetch_pull_request_status_with_command(&mut command), None);
    }

    #[test]
    fn fetch_pull_request_status_returns_none_when_command_cannot_spawn() {
        let mut command = Command::new("/definitely/missing-binary");

        assert_eq!(fetch_pull_request_status_with_command(&mut command), None);
    }

    #[test]
    fn gh_pr_view_parser_rejects_invalid_json() {
        assert!(parse_pull_request_view_json("not-json").is_err());
    }

    #[test]
    fn fetch_pull_request_status_parses_successful_command_output() {
        let mut command = Command::new("/bin/sh");
        command.args([
            "-c",
            "printf '{\"state\":\"MERGED\",\"mergedAt\":\"2026-03-13T00:00:00Z\",\"statusCheckRollup\":[{\"status\":\"COMPLETED\",\"conclusion\":\"SUCCESS\"}]}'",
        ]);

        let status =
            fetch_pull_request_status_with_command(&mut command).expect("status should parse");

        assert!(status.exists);
        assert!(status.merged);
        assert!(status.checks_green);
    }

    #[test]
    fn fetch_pull_request_status_uses_gh_command_for_pr_number() {
        with_fake_gh(
            "printf '{\"state\":\"MERGED\",\"mergedAt\":\"2026-03-13T00:00:00Z\",\"statusCheckRollup\":[]}'",
            || {
                let status = fetch_pull_request_status(&PullRequestRef {
                    number: 42,
                    url: "https://github.com/dot-matrix-labs/calypso/pull/42".to_string(),
                })
                .expect("status should parse");

                assert!(status.exists);
                assert!(status.merged);
                assert!(status.checks_green);
            },
        );
    }

    #[test]
    fn host_github_environment_reports_missing_pull_request_as_failing() {
        with_fake_gh("exit 1", || {
            let environment = HostGithubEnvironment;
            let pull_request = PullRequestRef {
                number: 99,
                url: "https://github.com/dot-matrix-labs/calypso/pull/99".to_string(),
            };

            assert!(!environment.pr_exists(&pull_request));
            assert!(!environment.pr_merged(&pull_request));
            assert!(!environment.checks_green(&pull_request));
        });
    }
}
