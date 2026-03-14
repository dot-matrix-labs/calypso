use std::process::Command;

use serde::Deserialize;

use crate::state::{BuiltinEvidence, EvidenceStatus, PullRequestRef};

pub use crate::state::{GithubMergeability, GithubPullRequestSnapshot, GithubReviewStatus};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum GithubCheckId {
    PullRequestExists,
    PullRequestReadyForReview,
    PullRequestChecksGreen,
    PullRequestReviewApproved,
    PullRequestMergeable,
}

impl GithubCheckId {
    fn builtin_key(self) -> &'static str {
        match self {
            GithubCheckId::PullRequestExists => "builtin.github.pr_exists",
            GithubCheckId::PullRequestReadyForReview => "builtin.github.pr_ready_for_review",
            GithubCheckId::PullRequestChecksGreen => "builtin.github.pr_checks_green",
            GithubCheckId::PullRequestReviewApproved => "builtin.github.pr_review_approved",
            GithubCheckId::PullRequestMergeable => "builtin.github.pr_mergeable",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubCheck {
    pub id: GithubCheckId,
    pub status: EvidenceStatus,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GithubReport {
    pub snapshot: Option<GithubPullRequestSnapshot>,
    pub error: Option<String>,
    pub checks: Vec<GithubCheck>,
}

impl GithubReport {
    pub fn to_builtin_evidence(&self) -> BuiltinEvidence {
        self.checks
            .iter()
            .fold(BuiltinEvidence::new(), |evidence, check| {
                evidence.with_status(check.id.builtin_key(), check.status)
            })
    }
}

pub trait GithubEnvironment {
    fn pull_request_snapshot(
        &self,
        pull_request: &PullRequestRef,
    ) -> Result<GithubPullRequestSnapshot, GithubSnapshotError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HostGithubEnvironment;

impl GithubEnvironment for HostGithubEnvironment {
    fn pull_request_snapshot(
        &self,
        pull_request: &PullRequestRef,
    ) -> Result<GithubPullRequestSnapshot, GithubSnapshotError> {
        fetch_pull_request_snapshot(pull_request)
    }
}

pub fn collect_github_report(
    environment: &impl GithubEnvironment,
    pull_request: &PullRequestRef,
) -> GithubReport {
    let (snapshot, error) = match environment.pull_request_snapshot(pull_request) {
        Ok(snapshot) => (Some(snapshot), None),
        Err(error) => (None, Some(error.to_string())),
    };
    GithubReport {
        snapshot: snapshot.clone(),
        error,
        checks: checks_from_snapshot(snapshot.as_ref()),
    }
}

fn checks_from_snapshot(snapshot: Option<&GithubPullRequestSnapshot>) -> Vec<GithubCheck> {
    let mut checks = vec![GithubCheck {
        id: GithubCheckId::PullRequestExists,
        status: if snapshot.is_some() {
            EvidenceStatus::Passing
        } else {
            EvidenceStatus::Failing
        },
    }];

    if let Some(snapshot) = snapshot {
        checks.extend([
            GithubCheck {
                id: GithubCheckId::PullRequestReadyForReview,
                status: if snapshot.is_draft {
                    EvidenceStatus::Failing
                } else {
                    EvidenceStatus::Passing
                },
            },
            GithubCheck {
                id: GithubCheckId::PullRequestChecksGreen,
                status: snapshot.checks,
            },
            GithubCheck {
                id: GithubCheckId::PullRequestReviewApproved,
                status: review_status_to_evidence(snapshot.review_status.clone()),
            },
            GithubCheck {
                id: GithubCheckId::PullRequestMergeable,
                status: mergeability_to_evidence(snapshot.mergeability.clone()),
            },
        ]);
    }

    checks
}

fn review_status_to_evidence(review_status: GithubReviewStatus) -> EvidenceStatus {
    match review_status {
        GithubReviewStatus::Approved => EvidenceStatus::Passing,
        GithubReviewStatus::ReviewRequired => EvidenceStatus::Manual,
        GithubReviewStatus::ChangesRequested => EvidenceStatus::Failing,
    }
}

fn mergeability_to_evidence(mergeability: GithubMergeability) -> EvidenceStatus {
    match mergeability {
        GithubMergeability::Mergeable => EvidenceStatus::Passing,
        GithubMergeability::Conflicting | GithubMergeability::Blocked => EvidenceStatus::Failing,
        GithubMergeability::Unknown => EvidenceStatus::Pending,
    }
}

#[derive(Debug)]
pub enum GithubSnapshotError {
    Json(serde_json::Error),
    MissingField(&'static str),
    UnsupportedValue { field: &'static str, value: String },
}

impl std::fmt::Display for GithubSnapshotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            GithubSnapshotError::Json(error) => write!(f, "invalid GitHub JSON: {error}"),
            GithubSnapshotError::MissingField(message) => write!(f, "{message}"),
            GithubSnapshotError::UnsupportedValue { field, value } => {
                write!(f, "unsupported GitHub value for {field}: {value}")
            }
        }
    }
}

impl std::error::Error for GithubSnapshotError {}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct PrView {
    state: String,
    is_draft: bool,
    review_decision: Option<String>,
    merge_state_status: String,
    status_check_rollup: Vec<CheckRollup>,
}

#[derive(Deserialize)]
struct CheckRollup {
    status: Option<String>,
    conclusion: Option<String>,
}

pub fn parse_pull_request_view_json(
    json: &str,
) -> Result<GithubPullRequestSnapshot, GithubSnapshotError> {
    let view: PrView = serde_json::from_str(json).map_err(GithubSnapshotError::Json)?;
    if view.state != "OPEN" && view.state != "MERGED" {
        return Err(GithubSnapshotError::UnsupportedValue {
            field: "state",
            value: view.state,
        });
    }

    Ok(GithubPullRequestSnapshot {
        is_draft: view.is_draft,
        review_status: parse_review_status(view.review_decision.as_deref())?,
        checks: parse_checks_status(&view.status_check_rollup)?,
        mergeability: parse_mergeability(view.merge_state_status.as_str())?,
    })
}

fn parse_review_status(
    review_decision: Option<&str>,
) -> Result<GithubReviewStatus, GithubSnapshotError> {
    match review_decision {
        Some("APPROVED") => Ok(GithubReviewStatus::Approved),
        Some("REVIEW_REQUIRED") | None => Ok(GithubReviewStatus::ReviewRequired),
        Some("CHANGES_REQUESTED") => Ok(GithubReviewStatus::ChangesRequested),
        Some(value) => Err(GithubSnapshotError::UnsupportedValue {
            field: "reviewDecision",
            value: value.to_string(),
        }),
    }
}

fn parse_checks_status(checks: &[CheckRollup]) -> Result<EvidenceStatus, GithubSnapshotError> {
    if checks.is_empty() {
        return Ok(EvidenceStatus::Pending);
    }

    let mut saw_pending = false;
    for check in checks {
        let status = check
            .status
            .as_deref()
            .ok_or(GithubSnapshotError::MissingField(
                "statusCheckRollup entry is missing status",
            ))?;

        match status {
            "QUEUED" | "IN_PROGRESS" | "PENDING" | "WAITING" | "REQUESTED" => {
                saw_pending = true;
            }
            "COMPLETED" => {
                let conclusion =
                    check
                        .conclusion
                        .as_deref()
                        .ok_or(GithubSnapshotError::MissingField(
                            "statusCheckRollup entry is missing conclusion",
                        ))?;

                if !matches!(conclusion, "SUCCESS" | "NEUTRAL" | "SKIPPED") {
                    return Ok(EvidenceStatus::Failing);
                }
            }
            value => {
                return Err(GithubSnapshotError::UnsupportedValue {
                    field: "statusCheckRollup.status",
                    value: value.to_string(),
                });
            }
        }
    }

    if saw_pending {
        Ok(EvidenceStatus::Pending)
    } else {
        Ok(EvidenceStatus::Passing)
    }
}

fn parse_mergeability(value: &str) -> Result<GithubMergeability, GithubSnapshotError> {
    match value {
        "CLEAN" | "HAS_HOOKS" | "UNSTABLE" => Ok(GithubMergeability::Mergeable),
        "DIRTY" => Ok(GithubMergeability::Conflicting),
        "BLOCKED" | "BEHIND" | "DRAFT" => Ok(GithubMergeability::Blocked),
        "UNKNOWN" => Ok(GithubMergeability::Unknown),
        other => Err(GithubSnapshotError::UnsupportedValue {
            field: "mergeStateStatus",
            value: other.to_string(),
        }),
    }
}

fn fetch_pull_request_snapshot(
    pull_request: &PullRequestRef,
) -> Result<GithubPullRequestSnapshot, GithubSnapshotError> {
    let mut command = Command::new("gh");
    command.args([
        "pr",
        "view",
        &pull_request.number.to_string(),
        "--json",
        "state,isDraft,reviewDecision,mergeStateStatus,statusCheckRollup",
    ]);

    fetch_pull_request_snapshot_with_command(&mut command)
}

fn fetch_pull_request_snapshot_with_command(
    command: &mut Command,
) -> Result<GithubPullRequestSnapshot, GithubSnapshotError> {
    let output = command
        .output()
        .map_err(|_| GithubSnapshotError::MissingField("gh command failed to spawn"))?;

    if !output.status.success() {
        return Err(GithubSnapshotError::MissingField(
            "gh command returned a non-zero exit status",
        ));
    }

    parse_pull_request_view_json(&String::from_utf8_lossy(&output.stdout))
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
    fn fetch_pull_request_status_returns_error_for_failed_gh_command() {
        let mut command = Command::new("/bin/sh");
        command.args(["-c", "exit 1"]);

        assert!(
            fetch_pull_request_snapshot_with_command(&mut command)
                .expect_err("failed command should return an error")
                .to_string()
                .contains("non-zero exit status")
        );
    }

    #[test]
    fn fetch_pull_request_status_returns_error_when_command_cannot_spawn() {
        let mut command = Command::new("/definitely/missing-binary");

        assert!(
            fetch_pull_request_snapshot_with_command(&mut command)
                .expect_err("missing command should return an error")
                .to_string()
                .contains("failed to spawn")
        );
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
            "printf '{\"state\":\"OPEN\",\"isDraft\":false,\"reviewDecision\":\"APPROVED\",\"mergeStateStatus\":\"CLEAN\",\"statusCheckRollup\":[{\"status\":\"COMPLETED\",\"conclusion\":\"SUCCESS\"}]}'",
        ]);

        let status =
            fetch_pull_request_snapshot_with_command(&mut command).expect("status should parse");

        assert!(!status.is_draft);
        assert_eq!(status.review_status, GithubReviewStatus::Approved);
        assert_eq!(status.checks, EvidenceStatus::Passing);
        assert_eq!(status.mergeability, GithubMergeability::Mergeable);
    }

    #[test]
    fn fetch_pull_request_status_uses_gh_command_for_pr_number() {
        with_fake_gh(
            "printf '{\"state\":\"OPEN\",\"isDraft\":false,\"reviewDecision\":\"APPROVED\",\"mergeStateStatus\":\"CLEAN\",\"statusCheckRollup\":[]}'",
            || {
                let status = fetch_pull_request_snapshot(&PullRequestRef {
                    number: 42,
                    url: "https://github.com/dot-matrix-labs/calypso/pull/42".to_string(),
                })
                .expect("status should parse");

                assert!(!status.is_draft);
                assert_eq!(status.review_status, GithubReviewStatus::Approved);
                assert_eq!(status.checks, EvidenceStatus::Pending);
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

            assert!(
                environment
                    .pull_request_snapshot(&pull_request)
                    .expect_err("failed gh command should surface an error")
                    .to_string()
                    .contains("non-zero exit status")
            );
        });
    }
}
