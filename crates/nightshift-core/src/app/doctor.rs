use std::path::Path;

use crate::doctor::{
    DoctorReport, DoctorStatus, HostDoctorEnvironment, apply_fix, collect_doctor_report,
    render_doctor_report, render_doctor_report_verbose,
};
use crate::report::{DoctorJsonCheck, DoctorJsonReport, DoctorJsonSummary};

use super::helpers::resolve_repo_root;

pub fn run_doctor(cwd: &Path) -> String {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);

    render_doctor_report(&report)
}

pub fn run_doctor_verbose(cwd: &Path) -> String {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);

    render_doctor_report_verbose(&report)
}

/// Result of attempting to fix a single doctor check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FixAttemptResult {
    pub check_label: String,
    pub applied: bool,
    pub output: String,
    /// Whether re-validation after the fix showed the check passing.
    pub validated: Option<bool>,
}

/// Apply the fix for a single failing check, then re-run validation.
///
/// Returns `Ok(result)` with the fix output on success, `Err(message)` on failure.
pub fn run_doctor_fix_single(cwd: &Path, check_id: &str) -> Result<FixAttemptResult, String> {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);

    let check = report
        .checks
        .iter()
        .find(|c| c.id.label() == check_id)
        .ok_or_else(|| format!("unknown check id '{check_id}'"))?;

    if check.status == DoctorStatus::Passing {
        return Ok(FixAttemptResult {
            check_label: check_id.to_string(),
            applied: false,
            output: "already passing".to_string(),
            validated: Some(true),
        });
    }

    let fix = check
        .fix
        .as_ref()
        .ok_or_else(|| format!("no fix available for '{check_id}'"))?;

    let output = apply_fix(fix, &repo_root)?;
    let is_manual = !fix.is_automatic();

    // Re-run the doctor check to validate the fix worked.
    let validated = if is_manual {
        None
    } else {
        let post_report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);
        post_report
            .checks
            .iter()
            .find(|c| c.id.label() == check_id)
            .map(|c| c.status == DoctorStatus::Passing)
    };

    Ok(FixAttemptResult {
        check_label: check_id.to_string(),
        applied: true,
        output,
        validated,
    })
}

/// Apply fixes for all failing checks that have auto-fixes.
///
/// Returns a list of results, one per failing check that was attempted.
pub fn run_doctor_fix_all(cwd: &Path) -> Vec<FixAttemptResult> {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);

    let mut results = Vec::new();

    for check in &report.checks {
        if check.status == DoctorStatus::Passing {
            continue;
        }

        let label = check.id.label().to_string();

        match &check.fix {
            None => {
                results.push(FixAttemptResult {
                    check_label: label,
                    applied: false,
                    output: "no fix available".to_string(),
                    validated: None,
                });
            }
            Some(fix) if !fix.is_automatic() => {
                let instructions = match fix {
                    crate::doctor::DoctorFix::Manual { instructions } => instructions.clone(),
                    _ => "manual action required".to_string(),
                };
                results.push(FixAttemptResult {
                    check_label: label,
                    applied: false,
                    output: format!("manual fix: {instructions}"),
                    validated: None,
                });
            }
            Some(fix) => match apply_fix(fix, &repo_root) {
                Ok(output) => {
                    // Re-run check to validate the fix.
                    let post_report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);
                    let validated = post_report
                        .checks
                        .iter()
                        .find(|c| c.id.label() == label)
                        .map(|c| c.status == DoctorStatus::Passing);

                    results.push(FixAttemptResult {
                        check_label: label,
                        applied: true,
                        output,
                        validated,
                    });
                }
                Err(error) => {
                    results.push(FixAttemptResult {
                        check_label: label,
                        applied: false,
                        output: format!("fix failed: {error}"),
                        validated: Some(false),
                    });
                }
            },
        }
    }

    results
}

/// Render fix results into human-readable CLI output.
pub fn render_fix_results(results: &[FixAttemptResult]) -> String {
    let mut lines = Vec::new();

    if results.is_empty() {
        lines.push("All checks passing — nothing to fix.".to_string());
        return lines.join("\n");
    }

    lines.push("Doctor fix results".to_string());
    lines.push("─".repeat(42));

    for result in results {
        let status = if result.applied {
            match result.validated {
                Some(true) => "FIXED",
                Some(false) => "FAILED",
                None => "APPLIED",
            }
        } else if result.output == "already passing" {
            "PASS"
        } else {
            "SKIP"
        };

        lines.push(format!("- [{status}] {}", result.check_label));

        if !result.output.is_empty() {
            for line in result.output.lines() {
                lines.push(format!("  {line}"));
            }
        }
    }

    lines.join("\n")
}

/// Build a `DoctorJsonReport` from a `DoctorReport`.
pub fn doctor_json_report(report: &DoctorReport) -> DoctorJsonReport {
    let checks: Vec<DoctorJsonCheck> = report
        .checks
        .iter()
        .map(|check| DoctorJsonCheck {
            id: check.id.label().to_string(),
            status: match check.status {
                DoctorStatus::Passing => "passing",
                DoctorStatus::Warning => "warning",
                DoctorStatus::Failing => "failing",
            },
            detail: check.detail.clone(),
            remediation: check.remediation.clone(),
            has_auto_fix: check.fix.as_ref().is_some_and(|f| f.is_automatic()),
        })
        .collect();

    let total = checks.len();
    let passing = checks.iter().filter(|c| c.status == "passing").count();
    let warnings = checks.iter().filter(|c| c.status == "warning").count();
    let failing = total - passing - warnings;

    DoctorJsonReport {
        checks,
        summary: DoctorJsonSummary {
            total,
            passing,
            warnings,
            failing,
        },
    }
}

/// Run the doctor check and return the JSON report as a pretty-printed string.
/// Returns `Ok(json)` when all checks pass, `Err(json)` when any fail.
pub fn run_doctor_json(cwd: &Path) -> Result<String, String> {
    let repo_root = resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&HostDoctorEnvironment, &repo_root);
    let json_report = doctor_json_report(&report);
    let json = serde_json::to_string_pretty(&json_report).expect("DoctorJsonReport must serialize");
    if json_report.summary.failing == 0 {
        Ok(json)
    } else {
        Err(json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_fix_results_empty_shows_all_passing() {
        let output = render_fix_results(&[]);
        assert!(
            output.contains("nothing to fix"),
            "expected 'nothing to fix' in output: {output}"
        );
    }

    #[test]
    fn render_fix_results_shows_fixed_status() {
        let results = vec![FixAttemptResult {
            check_label: "git-initialized".to_string(),
            applied: true,
            output: "Initialized empty Git repository".to_string(),
            validated: Some(true),
        }];
        let output = render_fix_results(&results);
        assert!(output.contains("[FIXED]"), "expected FIXED tag: {output}");
        assert!(
            output.contains("git-initialized"),
            "expected check label: {output}"
        );
    }

    #[test]
    fn render_fix_results_shows_failed_status() {
        let results = vec![FixAttemptResult {
            check_label: "gh-authenticated".to_string(),
            applied: true,
            output: "attempted auth".to_string(),
            validated: Some(false),
        }];
        let output = render_fix_results(&results);
        assert!(output.contains("[FAILED]"), "expected FAILED tag: {output}");
    }

    #[test]
    fn render_fix_results_shows_skip_for_manual() {
        let results = vec![FixAttemptResult {
            check_label: "gh-installed".to_string(),
            applied: false,
            output: "manual fix: Install gh from https://cli.github.com".to_string(),
            validated: None,
        }];
        let output = render_fix_results(&results);
        assert!(output.contains("[SKIP]"), "expected SKIP tag: {output}");
    }

    #[test]
    fn render_fix_results_shows_pass_for_already_passing() {
        let results = vec![FixAttemptResult {
            check_label: "git-initialized".to_string(),
            applied: false,
            output: "already passing".to_string(),
            validated: Some(true),
        }];
        let output = render_fix_results(&results);
        assert!(output.contains("[PASS]"), "expected PASS tag: {output}");
    }

    #[test]
    fn render_fix_results_shows_applied_when_no_validation() {
        let results = vec![FixAttemptResult {
            check_label: "some-check".to_string(),
            applied: true,
            output: "did something".to_string(),
            validated: None,
        }];
        let output = render_fix_results(&results);
        assert!(
            output.contains("[APPLIED]"),
            "expected APPLIED tag: {output}"
        );
    }
}
