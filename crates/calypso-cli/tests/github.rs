use nightshift_core::github::{
    GithubCheckId, GithubEnvironment, GithubMergeability, GithubPullRequestSnapshot,
    GithubReviewStatus, collect_github_report, parse_pull_request_view_json,
};
use nightshift_core::state::{EvidenceStatus, PullRequestRef};

struct FakeGithubEnvironment {
    snapshot: Result<GithubPullRequestSnapshot, String>,
}

impl FakeGithubEnvironment {
    fn with_snapshot(mut self, snapshot: GithubPullRequestSnapshot) -> Self {
        self.snapshot = Ok(snapshot);
        self
    }

    fn with_error(mut self, error: &str) -> Self {
        self.snapshot = Err(error.to_string());
        self
    }
}

impl GithubEnvironment for FakeGithubEnvironment {
    fn pull_request_snapshot(
        &self,
        _pull_request: &PullRequestRef,
    ) -> Result<GithubPullRequestSnapshot, nightshift_core::github::GithubSnapshotError> {
        self.snapshot.clone().map_err(|error| {
            nightshift_core::github::GithubSnapshotError::UnsupportedValue {
                field: "gh",
                value: error,
            }
        })
    }
}

impl Default for FakeGithubEnvironment {
    fn default() -> Self {
        Self {
            snapshot: Err("gh unavailable".to_string()),
        }
    }
}

fn sample_pr() -> PullRequestRef {
    PullRequestRef {
        number: 231,
        url: "https://github.com/org/repo/pull/231".to_string(),
    }
}

#[test]
fn github_report_collects_expected_statuses() {
    let report = collect_github_report(
        &FakeGithubEnvironment::default().with_snapshot(GithubPullRequestSnapshot {
            is_draft: false,
            review_status: GithubReviewStatus::Approved,
            checks: EvidenceStatus::Passing,
            mergeability: GithubMergeability::Mergeable,
        }),
        &sample_pr(),
    );

    assert_eq!(
        report.checks[0],
        nightshift_core::github::GithubCheck {
            id: GithubCheckId::PullRequestExists,
            status: EvidenceStatus::Passing,
        }
    );
    assert_eq!(
        report.checks[1],
        nightshift_core::github::GithubCheck {
            id: GithubCheckId::PullRequestReadyForReview,
            status: EvidenceStatus::Passing,
        }
    );
    assert_eq!(
        report.checks[2],
        nightshift_core::github::GithubCheck {
            id: GithubCheckId::PullRequestChecksGreen,
            status: EvidenceStatus::Passing,
        }
    );
    assert_eq!(
        report.checks[3],
        nightshift_core::github::GithubCheck {
            id: GithubCheckId::PullRequestReviewApproved,
            status: EvidenceStatus::Passing,
        }
    );
    assert_eq!(
        report.checks[4],
        nightshift_core::github::GithubCheck {
            id: GithubCheckId::PullRequestMergeable,
            status: EvidenceStatus::Passing,
        }
    );
}

#[test]
fn github_report_converts_statuses_to_builtin_evidence() {
    let report = collect_github_report(
        &FakeGithubEnvironment::default().with_snapshot(GithubPullRequestSnapshot {
            is_draft: true,
            review_status: GithubReviewStatus::ReviewRequired,
            checks: EvidenceStatus::Failing,
            mergeability: GithubMergeability::Conflicting,
        }),
        &sample_pr(),
    );

    let evidence = report.to_builtin_evidence();

    assert_eq!(evidence.result_for("builtin.github.pr_exists"), Some(true));
    assert_eq!(
        evidence.result_for("builtin.github.pr_ready_for_review"),
        Some(false)
    );
    assert_eq!(
        evidence.result_for("builtin.github.pr_checks_green"),
        Some(false)
    );
    assert_eq!(
        evidence.status_for("builtin.github.pr_review_approved"),
        Some(EvidenceStatus::Manual)
    );
    assert_eq!(
        evidence.result_for("builtin.github.pr_mergeable"),
        Some(false)
    );
}

#[test]
fn github_report_preserves_actionable_errors() {
    let report = collect_github_report(
        &FakeGithubEnvironment::default().with_error("Run `gh auth login`."),
        &sample_pr(),
    );

    assert_eq!(report.snapshot, None);
    assert_eq!(
        report.error.as_deref(),
        Some("unsupported GitHub value for gh: Run `gh auth login`.")
    );
    assert_eq!(
        report.checks[0].status,
        EvidenceStatus::Failing,
        "the missing snapshot should still block the PR existence gate"
    );
}

#[test]
fn gh_pr_view_parser_maps_draft_review_mergeability_and_check_state() {
    let status = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "OPEN",
  "isDraft": false,
  "reviewDecision": "APPROVED",
  "mergeStateStatus": "CLEAN",
  "statusCheckRollup": [
    { "status": "COMPLETED", "conclusion": "SUCCESS" },
    { "status": "COMPLETED", "conclusion": "NEUTRAL" }
  ]
}"#,
    )
    .expect("json should parse");

    assert!(!status.is_draft);
    assert_eq!(status.review_status, GithubReviewStatus::Approved);
    assert_eq!(status.checks, EvidenceStatus::Passing);
    assert_eq!(status.mergeability, GithubMergeability::Mergeable);
}

#[test]
fn gh_pr_view_parser_marks_review_required_as_manual_and_pending_checks_as_pending() {
    let pending = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "OPEN",
  "isDraft": true,
  "reviewDecision": "REVIEW_REQUIRED",
  "mergeStateStatus": "BLOCKED",
  "statusCheckRollup": [
    { "status": "IN_PROGRESS", "conclusion": null }
  ]
}"#,
    )
    .expect("json should parse");
    assert!(pending.is_draft);
    assert_eq!(pending.review_status, GithubReviewStatus::ReviewRequired);
    assert_eq!(pending.checks, EvidenceStatus::Pending);
    assert_eq!(pending.mergeability, GithubMergeability::Blocked);
}

#[test]
fn gh_pr_view_parser_marks_failed_checks_and_conflicts_as_blocking() {
    let failing = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "OPEN",
  "isDraft": false,
  "reviewDecision": "CHANGES_REQUESTED",
  "mergeStateStatus": "DIRTY",
  "statusCheckRollup": [
    { "status": "COMPLETED", "conclusion": "FAILURE" }
  ]
}"#,
    )
    .expect("json should parse");
    assert_eq!(failing.review_status, GithubReviewStatus::ChangesRequested);
    assert_eq!(failing.checks, EvidenceStatus::Failing);
    assert_eq!(failing.mergeability, GithubMergeability::Conflicting);
}

#[test]
fn gh_pr_view_parser_rejects_incomplete_check_data() {
    let error = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "OPEN",
  "isDraft": false,
  "reviewDecision": "APPROVED",
  "mergeStateStatus": "CLEAN",
  "statusCheckRollup": [
    { "status": null, "conclusion": "SUCCESS" }
  ]
}"#,
    )
    .expect_err("missing check status should fail loudly");

    assert!(
        error
            .to_string()
            .contains("statusCheckRollup entry is missing status")
    );
}

#[test]
fn gh_pr_view_parser_rejects_unknown_state() {
    let error = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "CLOSED",
  "isDraft": false,
  "reviewDecision": "APPROVED",
  "mergeStateStatus": "CLEAN",
  "statusCheckRollup": []
}"#,
    )
    .expect_err("unknown state should fail");

    assert!(
        error
            .to_string()
            .contains("unsupported GitHub value for state: CLOSED")
    );
}

#[test]
fn gh_pr_view_parser_rejects_unknown_review_decision() {
    let error = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "OPEN",
  "isDraft": false,
  "reviewDecision": "UNKNOWN_STATUS",
  "mergeStateStatus": "CLEAN",
  "statusCheckRollup": []
}"#,
    )
    .expect_err("unknown review decision should fail");

    assert!(
        error
            .to_string()
            .contains("unsupported GitHub value for reviewDecision: UNKNOWN_STATUS")
    );
}

#[test]
fn gh_pr_view_parser_rejects_unknown_merge_state() {
    let error = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "OPEN",
  "isDraft": false,
  "reviewDecision": "APPROVED",
  "mergeStateStatus": "INVALID_VALUE",
  "statusCheckRollup": []
}"#,
    )
    .expect_err("unknown merge state should fail");

    assert!(
        error
            .to_string()
            .contains("unsupported GitHub value for mergeStateStatus: INVALID_VALUE")
    );
}

#[test]
fn gh_pr_view_parser_rejects_missing_check_conclusion() {
    let error = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "OPEN",
  "isDraft": false,
  "reviewDecision": "APPROVED",
  "mergeStateStatus": "CLEAN",
  "statusCheckRollup": [
    { "status": "COMPLETED", "conclusion": null }
  ]
}"#,
    )
    .expect_err("missing conclusion should fail");

    assert!(
        error
            .to_string()
            .contains("statusCheckRollup entry is missing conclusion")
    );
}

#[test]
fn github_snapshot_error_missing_field_formats_message() {
    let error =
        nightshift_core::github::GithubSnapshotError::MissingField("gh command failed to spawn");
    assert_eq!(error.to_string(), "gh command failed to spawn");
}

#[test]
fn github_snapshot_error_json_formats_message() {
    let error = parse_pull_request_view_json("not-json").expect_err("invalid json should fail");

    assert!(error.to_string().contains("invalid GitHub JSON:"));
}

#[test]
fn gh_pr_view_parser_treats_unknown_merge_state_as_pending() {
    let status = parse_pull_request_view_json(
        r#"{
  "state": "OPEN",
  "isDraft": false,
  "reviewDecision": "APPROVED",
  "mergeStateStatus": "UNKNOWN",
  "statusCheckRollup": []
}"#,
    )
    .expect("json should parse");

    assert_eq!(status.mergeability, GithubMergeability::Unknown);
}

#[test]
fn github_report_converts_changes_requested_review_to_failing_evidence() {
    let report = collect_github_report(
        &FakeGithubEnvironment::default().with_snapshot(GithubPullRequestSnapshot {
            is_draft: false,
            review_status: GithubReviewStatus::ChangesRequested,
            checks: EvidenceStatus::Pending,
            mergeability: GithubMergeability::Unknown,
        }),
        &sample_pr(),
    );

    let evidence = report.to_builtin_evidence();

    assert_eq!(
        evidence.result_for("builtin.github.pr_review_approved"),
        Some(false),
        "ChangesRequested review should map to failing evidence"
    );
    assert_eq!(
        evidence.status_for("builtin.github.pr_mergeable"),
        Some(EvidenceStatus::Pending),
        "Unknown mergeability should map to pending evidence"
    );
}

#[test]
fn gh_pr_view_parser_rejects_unknown_check_status() {
    let error = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "OPEN",
  "isDraft": false,
  "reviewDecision": "APPROVED",
  "mergeStateStatus": "CLEAN",
  "statusCheckRollup": [
    { "status": "UNKNOWN_STATUS", "conclusion": "SUCCESS" }
  ]
}"#,
    )
    .expect_err("unknown check status should fail");

    assert!(
        error
            .to_string()
            .contains("unsupported GitHub value for statusCheckRollup.status: UNKNOWN_STATUS")
    );
}
