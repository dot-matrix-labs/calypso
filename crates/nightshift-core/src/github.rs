use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::Deserialize;

use crate::state::{BuiltinEvidence, EvidenceStatus, PullRequestRef};

pub use crate::state::{GithubMergeability, GithubPullRequestSnapshot, GithubReviewStatus};

// ---------------------------------------------------------------------------
// Owner/repo resolution from git remote
// ---------------------------------------------------------------------------

/// Parse the origin remote URL to extract `(owner, repo)`.
///
/// Handles:
///   - `https://github.com/owner/repo.git`
///   - `https://github.com/owner/repo`
///   - `git@github.com:owner/repo.git`
pub fn resolve_owner_repo(repo_root: &Path) -> Result<(String, String), GithubSnapshotError> {
    let output = Command::new("git")
        .args([
            "-C",
            &repo_root.to_string_lossy(),
            "remote",
            "get-url",
            "origin",
        ])
        .env_remove("GIT_DIR")
        .env_remove("GIT_WORK_TREE")
        .output()
        .map_err(|_| {
            GithubSnapshotError::MissingField("git remote get-url origin failed to spawn")
        })?;

    if !output.status.success() {
        return Err(GithubSnapshotError::MissingField(
            "git remote get-url origin returned a non-zero exit status",
        ));
    }

    let url = String::from_utf8_lossy(&output.stdout).trim().to_string();
    parse_owner_repo_from_url(&url)
}

/// Extract `(owner, repo)` from a GitHub remote URL.
pub fn parse_owner_repo_from_url(url: &str) -> Result<(String, String), GithubSnapshotError> {
    let trimmed = url.trim_end_matches(".git");

    // Try HTTPS: https://github.com/owner/repo
    if let Some(rest) = trimmed
        .strip_prefix("https://github.com/")
        .or_else(|| trimmed.strip_prefix("http://github.com/"))
    {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    // Try SSH: git@github.com:owner/repo
    if let Some(rest) = trimmed.strip_prefix("git@github.com:") {
        let parts: Vec<&str> = rest.splitn(3, '/').collect();
        if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
            return Ok((parts[0].to_string(), parts[1].to_string()));
        }
    }

    Err(GithubSnapshotError::MissingField(
        "could not parse owner/repo from remote URL",
    ))
}

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

#[derive(Debug, Default, Clone)]
pub struct HostGithubEnvironment {
    /// Pre-resolved `(owner, repo)` pair. When `None`, will be resolved from
    /// the current working directory's git remote at call time.
    pub owner_repo: Option<(String, String)>,
}

impl GithubEnvironment for HostGithubEnvironment {
    fn pull_request_snapshot(
        &self,
        pull_request: &PullRequestRef,
    ) -> Result<GithubPullRequestSnapshot, GithubSnapshotError> {
        let (owner, repo) = match &self.owner_repo {
            Some(pair) => pair.clone(),
            None => {
                // Fall back to resolving from the PR URL.
                parse_owner_repo_from_pr_url(&pull_request.url)?
            }
        };
        fetch_pull_request_snapshot(&owner, &repo, pull_request)
    }
}

// ---------------------------------------------------------------------------
// TTL cache for GitHub PR snapshots
// ---------------------------------------------------------------------------

/// Default TTL for cached [`GithubPullRequestSnapshot`] entries (15 seconds).
pub const GITHUB_SNAPSHOT_TTL: Duration = Duration::from_secs(15);

/// Cache key: `(owner, repo, pr_number)`.
type CacheKey = (String, String, u64);

/// In-memory TTL cache that wraps any [`GithubEnvironment`] and deduplicates
/// repeated `pull_request_snapshot` calls within the TTL window.
///
/// Two scheduling passes within the TTL for the same PR share one subprocess
/// call. A pass after TTL expiry issues a fresh call and refreshes the entry.
/// Explicit [`CachedGithubEnvironment::invalidate`] drops a cached entry
/// immediately, so steering actions that mutate PR state see fresh data.
pub struct CachedGithubEnvironment<E: GithubEnvironment> {
    inner: E,
    ttl: Duration,
    cache: Mutex<HashMap<CacheKey, (Instant, GithubPullRequestSnapshot)>>,
}

impl<E: GithubEnvironment> CachedGithubEnvironment<E> {
    /// Wrap `inner` with the default 15-second TTL.
    pub fn new(inner: E) -> Self {
        Self::with_ttl(inner, GITHUB_SNAPSHOT_TTL)
    }

    /// Wrap `inner` with an explicit TTL (useful for tests).
    pub fn with_ttl(inner: E, ttl: Duration) -> Self {
        Self {
            inner,
            ttl,
            cache: Mutex::new(HashMap::new()),
        }
    }

    /// Resolve `(owner, repo)` from a [`PullRequestRef`] URL.
    fn owner_repo(pull_request: &PullRequestRef) -> Result<(String, String), GithubSnapshotError> {
        parse_owner_repo_from_pr_url(&pull_request.url)
    }

    /// Invalidate the cached entry for the given PR, forcing the next call to
    /// issue a fresh subprocess. Call this after any steering action that
    /// mutates PR state (e.g. marking ready, merging).
    pub fn invalidate(&self, owner: &str, repo: &str, pr_number: u64) {
        let key = (owner.to_string(), repo.to_string(), pr_number);
        self.cache
            .lock()
            .expect("github snapshot cache lock should not be poisoned")
            .remove(&key);
    }
}

impl<E: GithubEnvironment> GithubEnvironment for CachedGithubEnvironment<E> {
    fn pull_request_snapshot(
        &self,
        pull_request: &PullRequestRef,
    ) -> Result<GithubPullRequestSnapshot, GithubSnapshotError> {
        let (owner, repo) = Self::owner_repo(pull_request)?;
        let key: CacheKey = (owner, repo, pull_request.number);

        // Check the cache first.
        {
            let cache = self
                .cache
                .lock()
                .expect("github snapshot cache lock should not be poisoned");
            if let Some((fetched_at, snapshot)) = cache.get(&key) {
                if fetched_at.elapsed() < self.ttl {
                    return Ok(snapshot.clone());
                }
            }
        }

        // Cache miss or TTL expired — call the inner environment.
        let snapshot = self.inner.pull_request_snapshot(pull_request)?;

        self.cache
            .lock()
            .expect("github snapshot cache lock should not be poisoned")
            .insert(key, (Instant::now(), snapshot.clone()));

        Ok(snapshot)
    }
}

/// Extract `(owner, repo)` from a pull request URL like
/// `https://github.com/owner/repo/pull/123`.
fn parse_owner_repo_from_pr_url(url: &str) -> Result<(String, String), GithubSnapshotError> {
    let rest = url
        .strip_prefix("https://github.com/")
        .or_else(|| url.strip_prefix("http://github.com/"))
        .ok_or(GithubSnapshotError::MissingField(
            "pull request URL is not a GitHub URL",
        ))?;
    let parts: Vec<&str> = rest.splitn(4, '/').collect();
    if parts.len() >= 2 && !parts[0].is_empty() && !parts[1].is_empty() {
        Ok((parts[0].to_string(), parts[1].to_string()))
    } else {
        Err(GithubSnapshotError::MissingField(
            "could not parse owner/repo from pull request URL",
        ))
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

// ---------------------------------------------------------------------------
// REST API deserialization types
// ---------------------------------------------------------------------------

/// Partial pull request object from `GET /repos/{o}/{r}/pulls/{n}`.
#[derive(Deserialize)]
struct RestPullRequest {
    state: String,
    draft: Option<bool>,
    mergeable_state: Option<String>,
    head: RestPullRequestHead,
}

#[derive(Deserialize)]
struct RestPullRequestHead {
    sha: String,
}

/// A single review from `GET /repos/{o}/{r}/pulls/{n}/reviews`.
#[derive(Deserialize)]
struct RestReview {
    user: RestUser,
    state: String,
}

#[derive(Deserialize)]
struct RestUser {
    login: String,
}

/// A single check run from `GET /repos/{o}/{r}/commits/{sha}/check-runs`.
#[derive(Deserialize)]
struct RestCheckRunsResponse {
    check_runs: Vec<RestCheckRun>,
}

#[derive(Deserialize)]
struct RestCheckRun {
    status: String,
    conclusion: Option<String>,
}

// ---------------------------------------------------------------------------
// Parsing helpers — kept public for tests
// ---------------------------------------------------------------------------

/// Parse the REST pull request JSON from `gh api repos/{o}/{r}/pulls/{n}`.
pub fn parse_pull_request_view_json(
    json: &str,
) -> Result<GithubPullRequestSnapshot, GithubSnapshotError> {
    // Support both the old GraphQL format (camelCase) and the new REST format.
    // Try REST first (has `draft` field), fall back to GraphQL (has `isDraft`).
    if let Ok(rest) = serde_json::from_str::<RestPullRequest>(json) {
        return parse_rest_pull_request(&rest, &[], &[]);
    }

    // Legacy GraphQL format (for tests that still pass camelCase JSON).
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct PrView {
        state: String,
        is_draft: bool,
        review_decision: Option<String>,
        merge_state_status: String,
        status_check_rollup: Vec<LegacyCheckRollup>,
    }

    #[derive(Deserialize)]
    struct LegacyCheckRollup {
        status: Option<String>,
        conclusion: Option<String>,
    }

    impl AsCheckRollup for LegacyCheckRollup {
        fn status_field(&self) -> Option<&str> {
            self.status.as_deref()
        }
        fn conclusion_field(&self) -> Option<&str> {
            self.conclusion.as_deref()
        }
    }

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
        checks: parse_legacy_checks_status(&view.status_check_rollup)?,
        mergeability: parse_mergeability_graphql(view.merge_state_status.as_str())?,
    })
}

/// Build a snapshot from REST API data (PR + reviews + check runs).
fn parse_rest_pull_request(
    pr: &RestPullRequest,
    reviews: &[RestReview],
    check_runs: &[RestCheckRun],
) -> Result<GithubPullRequestSnapshot, GithubSnapshotError> {
    let state_lower = pr.state.to_lowercase();
    if state_lower != "open" && state_lower != "closed" {
        return Err(GithubSnapshotError::UnsupportedValue {
            field: "state",
            value: pr.state.clone(),
        });
    }

    let is_draft = pr.draft.unwrap_or(false);
    let review_status = derive_review_decision(reviews);
    let checks = parse_rest_check_runs(check_runs);
    let mergeability = parse_mergeability_rest(pr.mergeable_state.as_deref())?;

    Ok(GithubPullRequestSnapshot {
        is_draft,
        review_status,
        checks,
        mergeability,
    })
}

/// Derive the review decision from the list of reviews.
///
/// Algorithm: take the latest review per user. If any is `CHANGES_REQUESTED`,
/// the decision is `ChangesRequested`. If any is `APPROVED` (and none are
/// `CHANGES_REQUESTED`), the decision is `Approved`. Otherwise `ReviewRequired`.
fn derive_review_decision(reviews: &[RestReview]) -> GithubReviewStatus {
    use std::collections::HashMap;

    let mut latest_by_user: HashMap<&str, &str> = HashMap::new();
    for review in reviews {
        // Only consider actionable review states.
        match review.state.as_str() {
            "APPROVED" | "CHANGES_REQUESTED" | "DISMISSED" => {
                latest_by_user.insert(&review.user.login, &review.state);
            }
            _ => {}
        }
    }

    if latest_by_user.is_empty() {
        return GithubReviewStatus::ReviewRequired;
    }

    let has_changes_requested = latest_by_user.values().any(|s| *s == "CHANGES_REQUESTED");
    let has_approved = latest_by_user.values().any(|s| *s == "APPROVED");

    if has_changes_requested {
        GithubReviewStatus::ChangesRequested
    } else if has_approved {
        GithubReviewStatus::Approved
    } else {
        GithubReviewStatus::ReviewRequired
    }
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

fn parse_rest_check_runs(check_runs: &[RestCheckRun]) -> EvidenceStatus {
    if check_runs.is_empty() {
        return EvidenceStatus::Pending;
    }

    let mut saw_pending = false;
    for run in check_runs {
        match run.status.as_str() {
            "queued" | "in_progress" => {
                saw_pending = true;
            }
            "completed" => {
                let conclusion = run.conclusion.as_deref().unwrap_or("");
                if !matches!(conclusion, "success" | "neutral" | "skipped") {
                    return EvidenceStatus::Failing;
                }
            }
            _ => {
                saw_pending = true;
            }
        }
    }

    if saw_pending {
        EvidenceStatus::Pending
    } else {
        EvidenceStatus::Passing
    }
}

/// Legacy: parse check rollup from GraphQL-style JSON.
fn parse_legacy_checks_status(
    checks: &[impl AsCheckRollup],
) -> Result<EvidenceStatus, GithubSnapshotError> {
    if checks.is_empty() {
        return Ok(EvidenceStatus::Pending);
    }

    let mut saw_pending = false;
    for check in checks {
        let status = check
            .status_field()
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
                        .conclusion_field()
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

trait AsCheckRollup {
    fn status_field(&self) -> Option<&str>;
    fn conclusion_field(&self) -> Option<&str>;
}

/// Map REST `mergeable_state` values to our enum.
fn parse_mergeability_rest(value: Option<&str>) -> Result<GithubMergeability, GithubSnapshotError> {
    match value {
        Some("clean") | Some("has_hooks") | Some("unstable") => Ok(GithubMergeability::Mergeable),
        Some("dirty") => Ok(GithubMergeability::Conflicting),
        Some("blocked") | Some("behind") | Some("draft") => Ok(GithubMergeability::Blocked),
        Some("unknown") | None => Ok(GithubMergeability::Unknown),
        Some(other) => Err(GithubSnapshotError::UnsupportedValue {
            field: "mergeable_state",
            value: other.to_string(),
        }),
    }
}

/// Map GraphQL `mergeStateStatus` values to our enum (kept for backward compat).
fn parse_mergeability_graphql(value: &str) -> Result<GithubMergeability, GithubSnapshotError> {
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

// ---------------------------------------------------------------------------
// REST fetch implementation
// ---------------------------------------------------------------------------

fn run_gh_api(args: &[&str]) -> Result<String, GithubSnapshotError> {
    let mut cmd = Command::new("gh");
    cmd.arg("api");
    cmd.args(args);

    let output = cmd
        .output()
        .map_err(|_| GithubSnapshotError::MissingField("gh command failed to spawn"))?;

    if !output.status.success() {
        return Err(GithubSnapshotError::MissingField(
            "gh command returned a non-zero exit status",
        ));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn fetch_pull_request_snapshot(
    owner: &str,
    repo: &str,
    pull_request: &PullRequestRef,
) -> Result<GithubPullRequestSnapshot, GithubSnapshotError> {
    // 1. Get PR details
    let pr_endpoint = format!("repos/{owner}/{repo}/pulls/{}", pull_request.number);
    let pr_json = run_gh_api(&[&pr_endpoint])?;
    let pr: RestPullRequest = serde_json::from_str(&pr_json).map_err(GithubSnapshotError::Json)?;

    // 2. Get reviews
    let reviews_endpoint = format!("repos/{owner}/{repo}/pulls/{}/reviews", pull_request.number);
    let reviews_json = run_gh_api(&[&reviews_endpoint])?;
    let reviews: Vec<RestReview> =
        serde_json::from_str(&reviews_json).map_err(GithubSnapshotError::Json)?;

    // 3. Get check runs for head SHA
    let checks_endpoint = format!("repos/{owner}/{repo}/commits/{}/check-runs", pr.head.sha);
    let checks_json = run_gh_api(&[&checks_endpoint])?;
    let checks_response: RestCheckRunsResponse =
        serde_json::from_str(&checks_json).map_err(GithubSnapshotError::Json)?;

    parse_rest_pull_request(&pr, &reviews, &checks_response.check_runs)
}

/// Fetch a pull request snapshot using an arbitrary command (for tests).
#[cfg(test)]
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
    fn fetch_pull_request_status_uses_gh_api_for_pr_number() {
        // This fake gh script handles the three REST API calls.
        let script = r#"
if echo "$*" | grep -q "pulls/42/reviews"; then
  printf '[]'
  exit 0
fi
if echo "$*" | grep -q "check-runs"; then
  printf '{"check_runs":[]}'
  exit 0
fi
if echo "$*" | grep -q "pulls/42"; then
  printf '{"state":"open","draft":false,"mergeable_state":"unknown","head":{"sha":"abc123"}}'
  exit 0
fi
exit 1
"#;
        with_fake_gh(script, || {
            let status = fetch_pull_request_snapshot(
                "dot-matrix-labs",
                "calypso",
                &PullRequestRef {
                    number: 42,
                    url: "https://github.com/dot-matrix-labs/calypso/pull/42".to_string(),
                },
            )
            .expect("status should parse");

            assert!(!status.is_draft);
            assert_eq!(status.review_status, GithubReviewStatus::ReviewRequired);
            assert_eq!(status.checks, EvidenceStatus::Pending);
        });
    }

    #[test]
    fn host_github_environment_reports_missing_pull_request_as_failing() {
        with_fake_gh("exit 1", || {
            let environment = HostGithubEnvironment::default();
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

    #[test]
    fn parse_owner_repo_from_url_handles_https() {
        let (owner, repo) = parse_owner_repo_from_url("https://github.com/org/repo.git").unwrap();
        assert_eq!(owner, "org");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn parse_owner_repo_from_url_handles_ssh() {
        let (owner, repo) = parse_owner_repo_from_url("git@github.com:org/repo.git").unwrap();
        assert_eq!(owner, "org");
        assert_eq!(repo, "repo");
    }

    #[test]
    fn derive_review_decision_with_no_reviews_is_review_required() {
        assert_eq!(
            derive_review_decision(&[]),
            GithubReviewStatus::ReviewRequired
        );
    }

    #[test]
    fn derive_review_decision_approved() {
        let reviews = vec![RestReview {
            user: RestUser {
                login: "alice".to_string(),
            },
            state: "APPROVED".to_string(),
        }];
        assert_eq!(
            derive_review_decision(&reviews),
            GithubReviewStatus::Approved
        );
    }

    #[test]
    fn derive_review_decision_changes_requested_overrides_approval() {
        let reviews = vec![
            RestReview {
                user: RestUser {
                    login: "alice".to_string(),
                },
                state: "APPROVED".to_string(),
            },
            RestReview {
                user: RestUser {
                    login: "bob".to_string(),
                },
                state: "CHANGES_REQUESTED".to_string(),
            },
        ];
        assert_eq!(
            derive_review_decision(&reviews),
            GithubReviewStatus::ChangesRequested
        );
    }

    #[test]
    fn parse_rest_check_runs_empty_is_pending() {
        assert_eq!(parse_rest_check_runs(&[]), EvidenceStatus::Pending);
    }

    #[test]
    fn parse_rest_check_runs_all_success_is_passing() {
        let runs = vec![RestCheckRun {
            status: "completed".to_string(),
            conclusion: Some("success".to_string()),
        }];
        assert_eq!(parse_rest_check_runs(&runs), EvidenceStatus::Passing);
    }

    #[test]
    fn parse_rest_check_runs_failure_is_failing() {
        let runs = vec![RestCheckRun {
            status: "completed".to_string(),
            conclusion: Some("failure".to_string()),
        }];
        assert_eq!(parse_rest_check_runs(&runs), EvidenceStatus::Failing);
    }

    #[test]
    fn parse_owner_repo_from_url_falls_through_when_https_missing_repo() {
        // URL has owner but no repo — should fall through to the error.
        let result = parse_owner_repo_from_url("https://github.com/owner-only");
        assert!(result.is_err(), "expected error for missing repo component");
    }

    #[test]
    fn parse_owner_repo_from_url_falls_through_when_ssh_missing_repo() {
        // SSH URL with only owner — falls through the SSH if block.
        let result = parse_owner_repo_from_url("git@github.com:owner-only");
        assert!(
            result.is_err(),
            "expected error for missing repo in SSH URL"
        );
    }

    #[test]
    fn parse_rest_pull_request_errors_on_unknown_state() {
        let pr = RestPullRequest {
            state: "MERGED".to_string(),
            draft: None,
            mergeable_state: None,
            head: RestPullRequestHead {
                sha: "abc123".to_string(),
            },
        };
        let result = parse_rest_pull_request(&pr, &[], &[]);
        assert!(
            result.is_err(),
            "expected error for unknown PR state 'MERGED'"
        );
    }

    #[test]
    fn derive_review_decision_dismissed_without_approved_is_review_required() {
        let reviews = vec![RestReview {
            user: RestUser {
                login: "alice".to_string(),
            },
            state: "DISMISSED".to_string(),
        }];
        assert_eq!(
            derive_review_decision(&reviews),
            GithubReviewStatus::ReviewRequired
        );
    }

    #[test]
    fn parse_rest_check_runs_pending_is_pending() {
        let runs = vec![RestCheckRun {
            status: "queued".to_string(),
            conclusion: None,
        }];
        assert_eq!(parse_rest_check_runs(&runs), EvidenceStatus::Pending);
    }

    #[test]
    fn parse_rest_check_runs_unknown_status_is_pending() {
        let runs = vec![RestCheckRun {
            status: "waiting".to_string(),
            conclusion: None,
        }];
        assert_eq!(parse_rest_check_runs(&runs), EvidenceStatus::Pending);
    }

    #[test]
    fn parse_mergeability_rest_maps_correctly() {
        assert_eq!(
            parse_mergeability_rest(Some("clean")).unwrap(),
            GithubMergeability::Mergeable
        );
        assert_eq!(
            parse_mergeability_rest(Some("dirty")).unwrap(),
            GithubMergeability::Conflicting
        );
        assert_eq!(
            parse_mergeability_rest(Some("blocked")).unwrap(),
            GithubMergeability::Blocked
        );
        assert_eq!(
            parse_mergeability_rest(None).unwrap(),
            GithubMergeability::Unknown
        );
    }

    // ── CachedGithubEnvironment tests ────────────────────────────────────────

    /// A stub `GithubEnvironment` that counts how many times it has been called
    /// and always returns a fixed snapshot.
    struct CountingGithubEnvironment {
        call_count: std::sync::Arc<std::sync::atomic::AtomicUsize>,
        snapshot: GithubPullRequestSnapshot,
    }

    impl CountingGithubEnvironment {
        fn new(snapshot: GithubPullRequestSnapshot) -> (Self, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
            let counter = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
            (
                Self {
                    call_count: std::sync::Arc::clone(&counter),
                    snapshot,
                },
                counter,
            )
        }
    }

    impl GithubEnvironment for CountingGithubEnvironment {
        fn pull_request_snapshot(
            &self,
            _pull_request: &PullRequestRef,
        ) -> Result<GithubPullRequestSnapshot, GithubSnapshotError> {
            self.call_count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            Ok(self.snapshot.clone())
        }
    }

    fn dummy_snapshot() -> GithubPullRequestSnapshot {
        GithubPullRequestSnapshot {
            is_draft: false,
            review_status: GithubReviewStatus::ReviewRequired,
            checks: crate::state::EvidenceStatus::Pending,
            mergeability: GithubMergeability::Unknown,
        }
    }

    fn dummy_pr(number: u64) -> PullRequestRef {
        PullRequestRef {
            number,
            url: format!("https://github.com/owner/repo/pull/{number}"),
        }
    }

    /// Two fetches within the TTL window produce exactly one subprocess call.
    #[test]
    fn cached_env_returns_cached_snapshot_within_ttl() {
        let (inner, counter) = CountingGithubEnvironment::new(dummy_snapshot());
        // Use a long TTL so the cache never expires within this test.
        let env = CachedGithubEnvironment::with_ttl(inner, Duration::from_secs(60));
        let pr = dummy_pr(1);

        let first = env.pull_request_snapshot(&pr).expect("first fetch should succeed");
        let second = env.pull_request_snapshot(&pr).expect("second fetch should succeed");

        assert_eq!(first, second, "both fetches should return the same snapshot");
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            1,
            "inner environment should be called exactly once within TTL"
        );
    }

    /// A fetch after TTL expiry issues a fresh subprocess call and updates the cache.
    #[test]
    fn cached_env_refetches_after_ttl_expiry() {
        let (inner, counter) = CountingGithubEnvironment::new(dummy_snapshot());
        // Use a zero-duration TTL so every access is expired.
        let env = CachedGithubEnvironment::with_ttl(inner, Duration::ZERO);
        let pr = dummy_pr(2);

        env.pull_request_snapshot(&pr).expect("first fetch should succeed");
        // Yield briefly so elapsed() > Duration::ZERO.
        std::thread::sleep(Duration::from_millis(1));
        env.pull_request_snapshot(&pr).expect("second fetch should succeed");

        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "inner environment should be called twice after TTL expiry"
        );
    }

    /// After a steering invalidation the cache entry is absent.
    #[test]
    fn cached_env_invalidation_removes_entry() {
        let (inner, counter) = CountingGithubEnvironment::new(dummy_snapshot());
        let env = CachedGithubEnvironment::with_ttl(inner, Duration::from_secs(60));
        let pr = dummy_pr(3);

        // Populate the cache.
        env.pull_request_snapshot(&pr).expect("first fetch should succeed");
        assert_eq!(counter.load(std::sync::atomic::Ordering::SeqCst), 1);

        // Invalidate simulating a steering action.
        env.invalidate("owner", "repo", 3);

        // Next fetch must issue a new subprocess call.
        env.pull_request_snapshot(&pr).expect("post-invalidation fetch should succeed");
        assert_eq!(
            counter.load(std::sync::atomic::Ordering::SeqCst),
            2,
            "inner environment should be called again after invalidation"
        );
    }
}
