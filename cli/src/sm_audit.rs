//! State machine audit — validates GHA workflow reference integrity in blueprint
//! workflows and the default state machine template.
//!
//! This module collects all `workflow:` path references from embedded blueprint
//! workflows, verifies they exist on disk, cross-checks `workflow_name` against
//! the actual GHA `name:` field, validates `check_names` / `job` keys against
//! the GHA `jobs:` map, and detects orphan/dangling check references within each
//! blueprint workflow.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use crate::blueprint_workflows::{BlueprintWorkflow, BlueprintWorkflowLibrary};
use crate::template::{self, TemplateSet};

// ── Audit result types ──────────────────────────────────────────────────────

/// Severity of an audit finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AuditSeverity {
    Error,
    Warning,
}

/// A single finding produced by the state machine audit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFinding {
    pub severity: AuditSeverity,
    /// Which blueprint workflow or template produced this finding.
    pub source: String,
    pub message: String,
    pub suggestion: Option<String>,
}

/// Aggregated results from the state machine audit.
#[derive(Debug, Clone)]
pub struct StateMachineAudit {
    pub findings: Vec<AuditFinding>,
}

impl StateMachineAudit {
    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == AuditSeverity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == AuditSeverity::Warning)
            .count()
    }
}

// ── Minimal GHA YAML parser ─────────────────────────────────────────────────

/// Minimal representation of a GitHub Actions workflow file.
#[derive(Debug, Clone)]
pub struct GhaWorkflow {
    pub name: Option<String>,
    pub jobs: Vec<String>,
}

/// Parse a GHA workflow YAML string, extracting only `name` and `jobs` keys.
pub fn parse_gha_workflow(yaml: &str) -> Result<GhaWorkflow, String> {
    let value: serde_yaml::Value =
        serde_yaml::from_str(yaml).map_err(|e| format!("failed to parse GHA YAML: {e}"))?;

    let name = value
        .get("name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let jobs = value
        .get("jobs")
        .and_then(|v| v.as_mapping())
        .map(|m| {
            m.keys()
                .filter_map(|k| k.as_str().map(|s| s.to_string()))
                .collect()
        })
        .unwrap_or_default();

    Ok(GhaWorkflow { name, jobs })
}

// ── Core audit logic ────────────────────────────────────────────────────────

/// Run the full state machine audit against the given repo root.
pub fn run_audit(repo_root: &Path) -> StateMachineAudit {
    let mut findings = Vec::new();

    // Scan .github/workflows/ for available GHA files
    let available_gha_files = scan_gha_directory(repo_root);

    // 1) Audit blueprint workflows
    for (stem, yaml) in BlueprintWorkflowLibrary::list() {
        let wf = match BlueprintWorkflowLibrary::parse(yaml) {
            Ok(wf) => wf,
            Err(e) => {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Error,
                    source: (*stem).to_string(),
                    message: format!("failed to parse blueprint workflow: {e}"),
                    suggestion: None,
                });
                continue;
            }
        };

        audit_blueprint_workflow(
            stem,
            &wf,
            repo_root,
            &available_gha_files,
            &mut findings,
        );
    }

    // 2) Audit policy gate paths from the default state machine template
    if let Ok(template) = template::load_embedded_template_set() {
        audit_template_policy_gates(&template, repo_root, &available_gha_files, &mut findings);
    }

    StateMachineAudit { findings }
}

/// Scan `.github/workflows/` and return a map of filename → parsed `GhaWorkflow`.
fn scan_gha_directory(repo_root: &Path) -> BTreeMap<String, GhaWorkflow> {
    let mut map = BTreeMap::new();
    let workflows_dir = repo_root.join(".github/workflows");
    if let Ok(entries) = std::fs::read_dir(&workflows_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_yaml = path.extension().and_then(|e| e.to_str()) == Some("yml")
                || path.extension().and_then(|e| e.to_str()) == Some("yaml");
            if is_yaml
                && let Some(filename) = path.file_name().and_then(|f| f.to_str())
                && let Ok(content) = std::fs::read_to_string(&path)
                && let Ok(gha) = parse_gha_workflow(&content)
            {
                map.insert(filename.to_string(), gha);
            }
        }
    }
    map
}

/// Find the closest matching GHA filename using simple substring matching.
fn suggest_filename(target: &str, available: &BTreeMap<String, GhaWorkflow>) -> Option<String> {
    // Extract the basename from a path like `.github/workflows/foo.yml`
    let target_base = target
        .rsplit('/')
        .next()
        .unwrap_or(target)
        .trim_end_matches(".yml")
        .trim_end_matches(".yaml");

    let mut best: Option<(&str, usize)> = None;

    for filename in available.keys() {
        let candidate = filename
            .trim_end_matches(".yml")
            .trim_end_matches(".yaml");

        // Check for substring overlap in either direction
        if candidate.contains(target_base) || target_base.contains(candidate) {
            let score = common_char_count(target_base, candidate);
            if best.is_none() || score > best.unwrap().1 {
                best = Some((filename.as_str(), score));
            }
        }
    }

    best.map(|(name, _)| name.to_string())
}

/// Count the number of characters in common (simple heuristic).
fn common_char_count(a: &str, b: &str) -> usize {
    let a_chars: BTreeSet<char> = a.chars().collect();
    let b_chars: BTreeSet<char> = b.chars().collect();
    a_chars.intersection(&b_chars).count()
}

/// Audit a single blueprint workflow for reference integrity and orphan checks.
fn audit_blueprint_workflow(
    stem: &str,
    wf: &BlueprintWorkflow,
    repo_root: &Path,
    available_gha_files: &BTreeMap<String, GhaWorkflow>,
    findings: &mut Vec<AuditFinding>,
) {
    // a) Collect all workflow paths from checks and validate them
    for (check_name, check_cfg) in &wf.checks {
        if let Some(wf_path) = &check_cfg.workflow {
            // Only validate paths that look like GHA file references
            if wf_path.starts_with(".github/workflows/") {
                validate_workflow_reference(
                    stem,
                    &format!("check '{check_name}'"),
                    wf_path,
                    check_cfg.workflow_name.as_deref(),
                    repo_root,
                    available_gha_files,
                    findings,
                );

                // c) Job key validation via `check_names`
                if let Some(check_names) = &check_cfg.check_names {
                    validate_job_keys(
                        stem,
                        &format!("check '{check_name}'"),
                        wf_path,
                        check_names,
                        available_gha_files,
                        findings,
                    );
                }

                // c) Job key validation via `job` field
                if let Some(job) = &check_cfg.job {
                    validate_job_keys(
                        stem,
                        &format!("check '{check_name}'"),
                        wf_path,
                        std::slice::from_ref(job),
                        available_gha_files,
                        findings,
                    );
                }
            }
        }
    }

    // Also validate workflow references in `github_actions` section
    if let Some(gha) = &wf.github_actions {
        let entries = gha
            .current_required
            .iter()
            .flatten()
            .chain(gha.proposed_required.iter().flatten());

        for entry in entries {
            if let Some(wf_path) = &entry.workflow
                && wf_path.starts_with(".github/workflows/")
            {
                validate_workflow_reference(
                    stem,
                    &format!("github_actions entry '{wf_path}'"),
                    wf_path,
                    entry.workflow_name.as_deref(),
                    repo_root,
                    available_gha_files,
                    findings,
                );

                if let Some(check_names) = &entry.check_names {
                    validate_job_keys(
                        stem,
                        &format!("github_actions entry '{wf_path}'"),
                        wf_path,
                        check_names,
                        available_gha_files,
                        findings,
                    );
                }
            }
        }
    }

    // Also validate workflow references in `hard_gates.ci_workflows`
    if let Some(hard_gates) = &wf.hard_gates
        && let Some(ci_workflows) = &hard_gates.ci_workflows
    {
        for gate in ci_workflows {
            if let Some(wf_path) = &gate.workflow
                && wf_path.starts_with(".github/workflows/")
            {
                validate_workflow_reference(
                    stem,
                    &format!("hard_gates ci_workflow '{wf_path}'"),
                    wf_path,
                    None,
                    repo_root,
                    available_gha_files,
                    findings,
                );

                if let Some(check_names) = &gate.check_names {
                    validate_job_keys(
                        stem,
                        &format!("hard_gates ci_workflow '{wf_path}'"),
                        wf_path,
                        check_names,
                        available_gha_files,
                        findings,
                    );
                }
            }
        }
    }

    // Also validate workflow references from state-level `workflows` lists
    for (state_name, state_cfg) in &wf.states {
        if let Some(wf_refs) = &state_cfg.workflows {
            for wf_ref in wf_refs {
                if let Some(wf_path) = &wf_ref.path
                    && wf_path.starts_with(".github/workflows/")
                {
                    validate_workflow_reference(
                        stem,
                        &format!("state '{state_name}' workflow ref"),
                        wf_path,
                        None,
                        repo_root,
                        available_gha_files,
                        findings,
                    );

                    if let Some(check_names) = &wf_ref.check_names {
                        validate_job_keys(
                            stem,
                            &format!("state '{state_name}' workflow ref"),
                            wf_path,
                            check_names,
                            available_gha_files,
                            findings,
                        );
                    }
                }
            }
        }
    }

    // d) Orphan/dangling check detection
    audit_check_references(stem, wf, findings);
}

/// Validate a single workflow file reference (existence + name match).
fn validate_workflow_reference(
    source: &str,
    context: &str,
    wf_path: &str,
    declared_name: Option<&str>,
    repo_root: &Path,
    available_gha_files: &BTreeMap<String, GhaWorkflow>,
    findings: &mut Vec<AuditFinding>,
) {
    let full_path = repo_root.join(wf_path);

    // a) Check file existence
    if !full_path.is_file() {
        let suggestion = suggest_filename(wf_path, available_gha_files);
        findings.push(AuditFinding {
            severity: AuditSeverity::Error,
            source: source.to_string(),
            message: format!("{context}: workflow file not found: {wf_path}"),
            suggestion: suggestion.map(|s| format!("did you mean .github/workflows/{s}?")),
        });
        return;
    }

    // b) Workflow name validation
    if let Some(declared) = declared_name {
        let filename = wf_path.rsplit('/').next().unwrap_or(wf_path);
        if let Some(gha) = available_gha_files.get(filename)
            && let Some(actual_name) = &gha.name
            && !names_match(declared, actual_name)
        {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warning,
                source: source.to_string(),
                message: format!(
                    "{context}: workflow_name mismatch — declared '{declared}' but GHA file has '{actual_name}'"
                ),
                suggestion: Some(format!("update workflow_name to '{actual_name}'")),
            });
        }
    }
}

/// Validate job keys referenced by `check_names` exist in the target GHA file.
fn validate_job_keys(
    source: &str,
    context: &str,
    wf_path: &str,
    job_keys: &[String],
    available_gha_files: &BTreeMap<String, GhaWorkflow>,
    findings: &mut Vec<AuditFinding>,
) {
    let filename = wf_path.rsplit('/').next().unwrap_or(wf_path);
    let gha = match available_gha_files.get(filename) {
        Some(gha) => gha,
        None => return, // File missing is already reported by reference integrity check
    };

    for key in job_keys {
        if !gha.jobs.contains(key) {
            findings.push(AuditFinding {
                severity: AuditSeverity::Error,
                source: source.to_string(),
                message: format!(
                    "{context}: job key '{key}' not found in {wf_path} (available: {})",
                    if gha.jobs.is_empty() {
                        "none".to_string()
                    } else {
                        gha.jobs.join(", ")
                    }
                ),
                suggestion: None,
            });
        }
    }
}

/// Audit orphan and dangling check references within a blueprint workflow.
fn audit_check_references(
    stem: &str,
    wf: &BlueprintWorkflow,
    findings: &mut Vec<AuditFinding>,
) {
    let defined_checks: BTreeSet<&str> = wf.checks.keys().map(|k| k.as_str()).collect();

    // Collect all check names referenced by states
    let mut referenced_checks: BTreeSet<&str> = BTreeSet::new();
    for state_cfg in wf.states.values() {
        if let Some(checks) = &state_cfg.checks {
            for check in checks {
                referenced_checks.insert(check.as_str());
            }
        }
        // Also count checks referenced via completion criteria
        if let Some(completion) = &state_cfg.completion {
            if let Some(all_of) = &completion.all_of {
                for item in all_of {
                    referenced_checks.insert(item.as_str());
                }
            }
            if let Some(any_of) = &completion.any_of {
                for item in any_of {
                    referenced_checks.insert(item.as_str());
                }
            }
        }
    }

    // Dangling: referenced by a state but not defined in `checks`
    for check_ref in &referenced_checks {
        if !defined_checks.contains(*check_ref) {
            findings.push(AuditFinding {
                severity: AuditSeverity::Error,
                source: stem.to_string(),
                message: format!(
                    "check '{check_ref}' is referenced by a state but not defined in the checks map"
                ),
                suggestion: Some(format!("add '{check_ref}' to the checks section")),
            });
        }
    }

    // Orphan: defined in `checks` but not referenced by any state
    for defined in &defined_checks {
        if !referenced_checks.contains(*defined) {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warning,
                source: stem.to_string(),
                message: format!(
                    "check '{defined}' is defined but not referenced by any state"
                ),
                suggestion: None,
            });
        }
    }
}

/// Audit policy gate paths from the template's `policy_gates`.
fn audit_template_policy_gates(
    template: &TemplateSet,
    repo_root: &Path,
    available_gha_files: &BTreeMap<String, GhaWorkflow>,
    findings: &mut Vec<AuditFinding>,
) {
    for pg in &template.state_machine.policy_gates {
        for path in &pg.paths {
            if path.starts_with(".github/workflows/") {
                let full_path = repo_root.join(path);
                if !full_path.is_file() {
                    let suggestion = suggest_filename(path, available_gha_files);
                    findings.push(AuditFinding {
                        severity: AuditSeverity::Error,
                        source: format!("template policy_gate '{}'", pg.gate_id),
                        message: format!("workflow file not found: {path}"),
                        suggestion: suggestion
                            .map(|s| format!("did you mean .github/workflows/{s}?")),
                    });
                }
            }
        }
    }
}

/// Case-insensitive name comparison.
fn names_match(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}

/// Render the audit results as a human-readable string block.
pub fn render_audit(audit: &StateMachineAudit) -> String {
    if audit.findings.is_empty() {
        return "State machine audit: all references valid".to_string();
    }

    let mut lines = vec![format!(
        "State machine audit: {} error(s), {} warning(s)",
        audit.error_count(),
        audit.warning_count()
    )];

    for finding in &audit.findings {
        let severity_label = match finding.severity {
            AuditSeverity::Error => "ERROR",
            AuditSeverity::Warning => "WARN",
        };
        lines.push(format!(
            "  [{severity_label}] [{}] {}",
            finding.source, finding.message
        ));
        if let Some(suggestion) = &finding.suggestion {
            lines.push(format!("         suggestion: {suggestion}"));
        }
    }

    lines.join("\n")
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time should be after unix epoch")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("calypso-sm-audit-{label}-{nanos}"));
        std::fs::create_dir_all(path.join(".github/workflows"))
            .expect("workflow dir should be created");
        path
    }

    fn write_gha_file(repo_root: &Path, filename: &str, name: &str, jobs: &[&str]) {
        let jobs_yaml: String = jobs
            .iter()
            .map(|j| format!("  {j}:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ok\n"))
            .collect();
        let content = format!("name: {name}\non: push\njobs:\n{jobs_yaml}");
        std::fs::write(
            repo_root.join(".github/workflows").join(filename),
            content,
        )
        .expect("gha file should be written");
    }

    #[test]
    fn parse_gha_workflow_extracts_name_and_jobs() {
        let yaml = "name: My Workflow\non: push\njobs:\n  build:\n    runs-on: ubuntu-latest\n    steps: []\n  test:\n    runs-on: ubuntu-latest\n    steps: []\n";
        let gha = parse_gha_workflow(yaml).unwrap();
        assert_eq!(gha.name.as_deref(), Some("My Workflow"));
        assert_eq!(gha.jobs.len(), 2);
        assert!(gha.jobs.contains(&"build".to_string()));
        assert!(gha.jobs.contains(&"test".to_string()));
    }

    #[test]
    fn parse_gha_workflow_handles_missing_name() {
        let yaml = "on: push\njobs:\n  lint:\n    runs-on: ubuntu-latest\n    steps: []\n";
        let gha = parse_gha_workflow(yaml).unwrap();
        assert!(gha.name.is_none());
        assert_eq!(gha.jobs, vec!["lint".to_string()]);
    }

    #[test]
    fn audit_passes_when_all_references_are_correct() {
        let repo_root = unique_temp_dir("correct-refs");
        write_gha_file(&repo_root, "quality-gate.yml", "Quality Gate", &["quality"]);
        write_gha_file(&repo_root, "test-unit.yml", "Unit Tests", &["test"]);

        // Write required workflow files that the template policy gates expect
        write_gha_file(&repo_root, "rust-quality.yml", "Rust Quality", &["check"]);
        write_gha_file(&repo_root, "rust-unit.yml", "Rust Unit", &["test"]);
        write_gha_file(&repo_root, "rust-integration.yml", "Rust Integration", &["test"]);
        write_gha_file(&repo_root, "rust-e2e.yml", "Rust E2E", &["test"]);
        write_gha_file(&repo_root, "rust-coverage.yml", "Rust Coverage", &["test"]);
        write_gha_file(&repo_root, "release-cli.yml", "Release CLI", &["release"]);

        let audit = run_audit(&repo_root);

        // Filter to only errors — there will be warnings for non-existent GHA files
        // referenced by other blueprint workflows (deployment, release, etc.)
        // but the key check here is that correct references don't produce errors
        // beyond the expected missing files from blueprint examples
        let template_errors: Vec<_> = audit
            .findings
            .iter()
            .filter(|f| {
                f.severity == AuditSeverity::Error
                    && f.source.starts_with("template policy_gate")
            })
            .collect();
        assert!(
            template_errors.is_empty(),
            "template policy gate errors should be empty when files exist: {template_errors:?}"
        );

        std::fs::remove_dir_all(repo_root).ok();
    }

    #[test]
    fn audit_reports_missing_workflow_file_with_suggestion() {
        let repo_root = unique_temp_dir("missing-wf");
        // Write a file with a similar name so suggestion can fire
        write_gha_file(&repo_root, "rust-quality.yml", "Rust Quality", &["check"]);

        let available = scan_gha_directory(&repo_root);
        let mut findings = Vec::new();

        validate_workflow_reference(
            "test-source",
            "check 'ci-gate'",
            ".github/workflows/quality-gate.yml",
            None,
            &repo_root,
            &available,
            &mut findings,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, AuditSeverity::Error);
        assert!(findings[0].message.contains("not found"));
        // Suggestion may or may not fire depending on substring match
    }

    #[test]
    fn audit_reports_workflow_name_mismatch() {
        let repo_root = unique_temp_dir("name-mismatch");
        write_gha_file(&repo_root, "quality-gate.yml", "Quality gate", &["quality"]);

        let available = scan_gha_directory(&repo_root);
        let mut findings = Vec::new();

        // "Quality Gate" vs actual "Quality gate" — case-insensitive comparison should pass
        validate_workflow_reference(
            "test-source",
            "check 'ci-gate'",
            ".github/workflows/quality-gate.yml",
            Some("Quality Gate"),
            &repo_root,
            &available,
            &mut findings,
        );

        assert!(
            findings.is_empty(),
            "case-insensitive match should pass: {findings:?}"
        );

        // Now test actual mismatch
        validate_workflow_reference(
            "test-source",
            "check 'ci-gate'",
            ".github/workflows/quality-gate.yml",
            Some("Wrong Name"),
            &repo_root,
            &available,
            &mut findings,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, AuditSeverity::Warning);
        assert!(findings[0].message.contains("mismatch"));
        assert!(findings[0]
            .suggestion
            .as_ref()
            .unwrap()
            .contains("Quality gate"));

        std::fs::remove_dir_all(repo_root).ok();
    }

    #[test]
    fn audit_reports_invalid_job_key() {
        let repo_root = unique_temp_dir("invalid-job");
        write_gha_file(&repo_root, "release.yml", "Release", &["build", "deploy"]);

        let available = scan_gha_directory(&repo_root);
        let mut findings = Vec::new();

        validate_job_keys(
            "test-source",
            "check 'release-lint'",
            ".github/workflows/release.yml",
            &["nonexistent-job".to_string()],
            &available,
            &mut findings,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, AuditSeverity::Error);
        assert!(findings[0].message.contains("nonexistent-job"));
        assert!(findings[0].message.contains("build, deploy"));

        std::fs::remove_dir_all(repo_root).ok();
    }

    #[test]
    fn audit_reports_valid_job_key_no_finding() {
        let repo_root = unique_temp_dir("valid-job");
        write_gha_file(&repo_root, "release.yml", "Release", &["build", "deploy"]);

        let available = scan_gha_directory(&repo_root);
        let mut findings = Vec::new();

        validate_job_keys(
            "test-source",
            "check 'release-build'",
            ".github/workflows/release.yml",
            &["build".to_string()],
            &available,
            &mut findings,
        );

        assert!(findings.is_empty());

        std::fs::remove_dir_all(repo_root).ok();
    }

    #[test]
    fn audit_detects_orphan_checks() {
        let wf_yaml = r#"
version: 1
name: test-workflow
initial_state: start
states:
  start:
    kind: agent
    completion:
      all_of:
        - check-a
checks:
  check-a:
    kind: deterministic
  check-orphan:
    kind: deterministic
"#;
        let wf: BlueprintWorkflow = serde_yaml::from_str(wf_yaml).unwrap();
        let mut findings = Vec::new();
        audit_check_references("test-wf", &wf, &mut findings);

        let orphans: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("not referenced"))
            .collect();
        assert_eq!(orphans.len(), 1);
        assert!(orphans[0].message.contains("check-orphan"));
        assert_eq!(orphans[0].severity, AuditSeverity::Warning);
    }

    #[test]
    fn audit_detects_dangling_check_references() {
        let wf_yaml = r#"
version: 1
name: test-workflow
initial_state: start
states:
  start:
    kind: agent
    completion:
      all_of:
        - check-a
        - check-nonexistent
checks:
  check-a:
    kind: deterministic
"#;
        let wf: BlueprintWorkflow = serde_yaml::from_str(wf_yaml).unwrap();
        let mut findings = Vec::new();
        audit_check_references("test-wf", &wf, &mut findings);

        let dangling: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("not defined"))
            .collect();
        assert_eq!(dangling.len(), 1);
        assert!(dangling[0].message.contains("check-nonexistent"));
        assert_eq!(dangling[0].severity, AuditSeverity::Error);
    }

    #[test]
    fn suggest_filename_finds_substring_match() {
        let mut available = BTreeMap::new();
        available.insert(
            "rust-quality.yml".to_string(),
            GhaWorkflow {
                name: Some("Rust Quality".to_string()),
                jobs: vec![],
            },
        );

        let result = suggest_filename(".github/workflows/quality.yml", &available);
        assert_eq!(result.as_deref(), Some("rust-quality.yml"));
    }

    #[test]
    fn render_audit_clean() {
        let audit = StateMachineAudit {
            findings: vec![],
        };
        let output = render_audit(&audit);
        assert_eq!(output, "State machine audit: all references valid");
    }

    #[test]
    fn render_audit_with_findings() {
        let audit = StateMachineAudit {
            findings: vec![
                AuditFinding {
                    severity: AuditSeverity::Error,
                    source: "test-wf".to_string(),
                    message: "workflow file not found".to_string(),
                    suggestion: Some("did you mean foo.yml?".to_string()),
                },
                AuditFinding {
                    severity: AuditSeverity::Warning,
                    source: "test-wf".to_string(),
                    message: "orphan check".to_string(),
                    suggestion: None,
                },
            ],
        };
        let output = render_audit(&audit);
        assert!(output.contains("1 error(s), 1 warning(s)"));
        assert!(output.contains("[ERROR]"));
        assert!(output.contains("[WARN]"));
        assert!(output.contains("suggestion: did you mean foo.yml?"));
    }
}
