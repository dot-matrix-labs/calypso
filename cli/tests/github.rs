use calypso_cli::github::{
    GithubCheckId, GithubEnvironment, GithubStatus, collect_github_report,
    parse_pull_request_view_json,
};
use calypso_cli::state::PullRequestRef;

#[derive(Default)]
struct FakeGithubEnvironment {
    pr_exists: bool,
    pr_merged: bool,
    checks_green: bool,
}

impl FakeGithubEnvironment {
    fn with_pr_exists(mut self, exists: bool) -> Self {
        self.pr_exists = exists;
        self
    }

    fn with_pr_merged(mut self, merged: bool) -> Self {
        self.pr_merged = merged;
        self
    }

    fn with_checks_green(mut self, green: bool) -> Self {
        self.checks_green = green;
        self
    }
}

impl GithubEnvironment for FakeGithubEnvironment {
    fn pr_exists(&self, _pull_request: &PullRequestRef) -> bool {
        self.pr_exists
    }

    fn pr_merged(&self, _pull_request: &PullRequestRef) -> bool {
        self.pr_merged
    }

    fn checks_green(&self, _pull_request: &PullRequestRef) -> bool {
        self.checks_green
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
        &FakeGithubEnvironment::default()
            .with_pr_exists(true)
            .with_pr_merged(false)
            .with_checks_green(true),
        &sample_pr(),
    );

    assert_eq!(
        report.checks[0],
        calypso_cli::github::GithubCheck {
            id: GithubCheckId::PullRequestExists,
            status: GithubStatus::Passing,
        }
    );
    assert_eq!(
        report.checks[1],
        calypso_cli::github::GithubCheck {
            id: GithubCheckId::PullRequestMerged,
            status: GithubStatus::Failing,
        }
    );
    assert_eq!(
        report.checks[2],
        calypso_cli::github::GithubCheck {
            id: GithubCheckId::PullRequestChecksGreen,
            status: GithubStatus::Passing,
        }
    );
}

#[test]
fn github_report_converts_statuses_to_builtin_evidence() {
    let report = collect_github_report(
        &FakeGithubEnvironment::default()
            .with_pr_exists(true)
            .with_pr_merged(true)
            .with_checks_green(false),
        &sample_pr(),
    );

    let evidence = report.to_builtin_evidence();

    assert_eq!(evidence.result_for("builtin.github.pr_exists"), Some(true));
    assert_eq!(evidence.result_for("builtin.github.pr_merged"), Some(true));
    assert_eq!(
        evidence.result_for("builtin.github.pr_checks_green"),
        Some(false)
    );
}

#[test]
fn gh_pr_view_parser_maps_merge_and_check_state() {
    let status = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "OPEN",
  "mergedAt": null,
  "statusCheckRollup": [
    { "status": "COMPLETED", "conclusion": "SUCCESS" },
    { "status": "COMPLETED", "conclusion": "NEUTRAL" }
  ]
}"#,
    )
    .expect("json should parse");

    assert!(status.exists);
    assert!(!status.merged);
    assert!(status.checks_green);
}

#[test]
fn gh_pr_view_parser_marks_pending_or_failed_checks_as_not_green() {
    let pending = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "OPEN",
  "mergedAt": null,
  "statusCheckRollup": [
    { "status": "IN_PROGRESS", "conclusion": null }
  ]
}"#,
    )
    .expect("json should parse");
    assert!(!pending.checks_green);

    let failing = parse_pull_request_view_json(
        r#"{
  "number": 231,
  "state": "MERGED",
  "mergedAt": "2026-03-13T00:00:00Z",
  "statusCheckRollup": [
    { "status": "COMPLETED", "conclusion": "FAILURE" }
  ]
}"#,
    )
    .expect("json should parse");
    assert!(failing.merged);
    assert!(!failing.checks_green);
}
