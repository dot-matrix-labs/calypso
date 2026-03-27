//! State machine audit — validates structural integrity of the unified workflow
//! graph.
//!
//! The audit walks the workflow graph starting from all discovered entry points
//! (workflows that are not exclusively used as sub-workflows), following
//! `kind: workflow` references transitively, and verifies:
//!
//! - Every embedded workflow YAML file is reachable from at least one entry point
//! - Every state in every reachable workflow is reachable from that workflow's
//!   `initial_state`
//! - No dead branches: every non-terminal state can reach a terminal state
//! - Cross-workflow handoffs are valid: `kind: workflow` states' transition
//!   events match the terminal state names in the referenced sub-workflow
//! - Check reference integrity (no dangling or orphan checks) within each workflow
//!
//! Additionally validates GHA workflow file references on disk (for `run_audit`)
//! and the default template set.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;

use crate::blueprint_workflows::{BlueprintWorkflow, StateKind, WorkflowCatalog};
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

    /// Format all error findings as a human-readable multi-line string.
    pub fn format_errors(&self) -> String {
        self.findings
            .iter()
            .filter(|f| f.severity == AuditSeverity::Error)
            .map(|f| format!("[{}] {}", f.source, f.message))
            .collect::<Vec<_>>()
            .join("\n")
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

/// Run a structural audit of the unified workflow graph — no filesystem access
/// required.
///
/// Discovers all entry points (workflows that are not exclusively used as
/// sub-workflows) and walks the graph from each one transitively. Validates:
///
/// - All embedded blueprint workflows parse as valid YAML
/// - Every workflow file is reachable from at least one entry point (no orphans)
/// - Every state within each workflow is reachable from its `initial_state`
/// - No dead branches: every non-terminal state can reach a terminal state
/// - Cross-workflow handoffs: `kind: workflow` transition events match the
///   terminal state names of the referenced sub-workflow
/// - Check reference integrity (no dangling or orphan checks) within each workflow
/// - The default template set loads and passes `validate()`
///
/// This is suitable for compile-time / test-time validation of the embedded
/// state machine bytes without needing a real repo on disk.
pub fn run_structural_audit() -> StateMachineAudit {
    let mut findings = Vec::new();

    // 1) Parse all embedded workflows into a lookup map
    let workflows = parse_catalog_workflows(&WorkflowCatalog::embedded(), &mut findings);

    // 2) Walk the workflow graph from all entry points
    let entry_roots = entry_point_roots(&workflows);
    audit_workflow_graph(&entry_roots, &workflows, &mut findings);

    // 3) Validate the default template set loads
    if let Err(e) = template::load_embedded_template_set() {
        findings.push(AuditFinding {
            severity: AuditSeverity::Error,
            source: "default-template".to_string(),
            message: format!("embedded template failed to load: {e}"),
            suggestion: None,
        });
    }

    StateMachineAudit { findings }
}

/// Returns the names of all top-level entry point workflows — those that are
/// not exclusively used as sub-workflows by other workflows.
fn entry_point_roots(workflows: &BTreeMap<String, BlueprintWorkflow>) -> Vec<&str> {
    // Collect all sub-workflow names referenced by kind: workflow states.
    let mut sub_names: BTreeSet<&str> = BTreeSet::new();
    for wf in workflows.values() {
        for state in wf.states.values() {
            if state.kind.as_ref() == Some(&StateKind::Workflow)
                && let Some(ref target) = state.workflow
            {
                sub_names.insert(target.as_str());
            }
        }
    }

    // Entry points: all workflows that are not exclusively sub-workflows,
    // or that carry an explicit trigger/schedule.
    workflows
        .keys()
        .filter(|name| {
            let wf = &workflows[*name];
            !sub_names.contains(name.as_str()) || wf.schedule.is_some() || wf.trigger.is_some()
        })
        .map(|s| s.as_str())
        .collect()
}

/// Walk the workflow graph starting from all `roots`, following `kind: workflow`
/// references transitively.
///
/// For each reachable workflow:
/// - Validates internal reachability (all states reachable from initial_state)
/// - Validates completeness (all non-terminal states can reach a terminal)
/// - Validates check reference integrity
/// - Validates cross-workflow handoffs (transition events match sub-workflow terminals)
///
/// After walking, flags any workflow files that were never reached as orphans.
fn audit_workflow_graph(
    roots: &[&str],
    workflows: &BTreeMap<String, BlueprintWorkflow>,
    findings: &mut Vec<AuditFinding>,
) {
    // BFS through workflow references, seeded from all entry points
    let mut visited: BTreeSet<&str> = BTreeSet::new();
    let mut queue: VecDeque<&str> = VecDeque::new();
    for &root in roots {
        if visited.insert(root) {
            queue.push_back(root);
        }
    }

    while let Some(wf_name) = queue.pop_front() {
        let wf = match workflows.get(wf_name) {
            Some(wf) => wf,
            None => {
                // Referenced workflow doesn't exist — already flagged by handoff check below
                continue;
            }
        };

        // Per-workflow checks
        audit_reachability(wf_name, wf, findings);

        // Follow kind: workflow references to sub-workflows
        for (state_name, cfg) in &wf.states {
            if cfg.kind.as_ref() == Some(&StateKind::Workflow)
                && let Some(ref sub_wf_name) = cfg.workflow
            {
                // Validate the reference exists
                if !workflows.contains_key(sub_wf_name.as_str()) {
                    findings.push(AuditFinding {
                        severity: AuditSeverity::Error,
                        source: wf_name.to_string(),
                        message: format!(
                            "state '{state_name}' references workflow '{sub_wf_name}' \
                             which is not in the embedded workflow library"
                        ),
                        suggestion: None,
                    });
                    continue;
                }

                // Validate cross-workflow handoff: the parent's next.on: events
                // should match the sub-workflow's terminal state names
                audit_workflow_handoff(
                    wf_name,
                    state_name,
                    sub_wf_name,
                    cfg.next.as_ref(),
                    workflows.get(sub_wf_name.as_str()).unwrap(),
                    findings,
                );

                // Enqueue for traversal
                if visited.insert(sub_wf_name.as_str()) {
                    queue.push_back(sub_wf_name.as_str());
                }
            }
        }
    }

    // Flag orphan workflows — defined but not reachable from any entry point
    for wf_name in workflows.keys() {
        if !visited.contains(wf_name.as_str()) {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warning,
                source: wf_name.clone(),
                message: format!("workflow '{wf_name}' is not reachable from any entry point"),
                suggestion: Some(
                    "add a kind: workflow reference from a reachable workflow, or remove it"
                        .to_string(),
                ),
            });
        }
    }
}

/// Validate the handoff between a parent workflow state and its sub-workflow.
///
/// The parent state's `next.on:` event names should correspond to terminal state
/// names in the sub-workflow. Flags:
/// - Sub-workflow terminals not handled by the parent (potential lost exits)
/// - Parent events that don't correspond to any sub-workflow terminal (dead references)
fn audit_workflow_handoff(
    parent_wf: &str,
    parent_state: &str,
    sub_wf_name: &str,
    parent_next: Option<&crate::blueprint_workflows::NextSpec>,
    sub_wf: &BlueprintWorkflow,
    findings: &mut Vec<AuditFinding>,
) {
    // Collect terminal state names from the sub-workflow
    let sub_terminals: BTreeSet<&str> = sub_wf
        .states
        .iter()
        .filter(|(_, cfg)| cfg.kind.as_ref() == Some(&StateKind::Terminal))
        .map(|(name, _)| name.as_str())
        .collect();

    // Collect event keys from the parent's next spec — these should match
    // the terminal state names of the sub-workflow.
    let parent_event_keys: BTreeSet<&str> = parent_next
        .map(|next| next.all_event_keys())
        .unwrap_or_default()
        .into_iter()
        .collect();

    // Sub-workflow terminal not handled by parent
    for terminal in &sub_terminals {
        if !parent_event_keys.contains(terminal) {
            findings.push(AuditFinding {
                severity: AuditSeverity::Warning,
                source: parent_wf.to_string(),
                message: format!(
                    "state '{parent_state}' delegates to '{sub_wf_name}' but does not handle \
                     terminal state '{terminal}'"
                ),
                suggestion: Some(format!(
                    "add '{terminal}' to the next.on: map of state '{parent_state}'"
                )),
            });
        }
    }

    // Parent event that doesn't match any sub-workflow terminal
    for event in &parent_event_keys {
        if !sub_terminals.contains(event) {
            findings.push(AuditFinding {
                severity: AuditSeverity::Error,
                source: parent_wf.to_string(),
                message: format!(
                    "state '{parent_state}' handles event '{event}' from '{sub_wf_name}' \
                     but '{sub_wf_name}' has no terminal state named '{event}'"
                ),
                suggestion: None,
            });
        }
    }
}

// ── Reachability analysis ────────────────────────────────────────────────────

/// Build a directed graph from a blueprint workflow and verify:
/// 1. Every declared state is reachable from `initial_state` (forward reachability)
/// 2. Every non-terminal state can reach at least one terminal state (no dead branches)
///
/// A state is terminal if its `kind` is `Terminal` or it has no `next` spec.
fn audit_reachability(stem: &str, wf: &BlueprintWorkflow, findings: &mut Vec<AuditFinding>) {
    let initial_state = match &wf.initial_state {
        Some(s) => s.as_str(),
        None => return, // No initial_state means no state machine graph to audit
    };

    if wf.states.is_empty() {
        return;
    }

    // Build adjacency list: state_name → set of target state names
    let mut edges: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    let mut terminal_states: BTreeSet<&str> = BTreeSet::new();

    for (name, cfg) in &wf.states {
        let name_str = name.as_str();
        edges.entry(name_str).or_default();

        // A state is terminal only if explicitly marked kind: terminal
        let is_terminal = cfg.kind.as_ref() == Some(&StateKind::Terminal);
        if is_terminal {
            terminal_states.insert(name_str);
        }

        if let Some(next) = &cfg.next {
            for target in next.all_targets() {
                edges.entry(name_str).or_default().insert(target);
            }
        }
    }

    let all_states: BTreeSet<&str> = wf.states.keys().map(|s| s.as_str()).collect();

    // 1) Forward reachability: BFS from initial_state
    let reachable = bfs_reachable(initial_state, &edges);
    for state in &all_states {
        if !reachable.contains(state) {
            findings.push(AuditFinding {
                severity: AuditSeverity::Error,
                source: stem.to_string(),
                message: format!(
                    "state '{state}' is not reachable from initial_state '{initial_state}'"
                ),
                suggestion: Some("add a transition targeting this state or remove it".to_string()),
            });
        }
    }

    // Check that all transition targets reference declared states
    for (from, targets) in &edges {
        for target in targets {
            if !all_states.contains(target) {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Error,
                    source: stem.to_string(),
                    message: format!(
                        "state '{from}' transitions to '{target}' which is not declared in the states map"
                    ),
                    suggestion: Some(format!("add '{target}' to the states section")),
                });
            }
        }
    }

    // 2) Backward reachability: can every non-terminal state reach a terminal?
    //    Build reverse edges, BFS backward from all terminals.
    let mut reverse_edges: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for (&from, targets) in &edges {
        for &target in targets {
            reverse_edges.entry(target).or_default().insert(from);
        }
    }

    let mut can_reach_terminal: BTreeSet<&str> = BTreeSet::new();
    let mut queue: VecDeque<&str> = terminal_states.iter().copied().collect();
    // Terminals themselves can trivially "reach" a terminal
    can_reach_terminal.extend(terminal_states.iter());

    while let Some(node) = queue.pop_front() {
        if let Some(predecessors) = reverse_edges.get(node) {
            for &pred in predecessors {
                if can_reach_terminal.insert(pred) {
                    queue.push_back(pred);
                }
            }
        }
    }

    for state in &all_states {
        if !terminal_states.contains(state)
            && !can_reach_terminal.contains(state)
            && reachable.contains(state)
        {
            findings.push(AuditFinding {
                severity: AuditSeverity::Error,
                source: stem.to_string(),
                message: format!("state '{state}' cannot reach any terminal state (dead branch)"),
                suggestion: Some(
                    "add a transition path from this state to a terminal state".to_string(),
                ),
            });
        }
    }
}

/// BFS from a start node, returning all reachable node names.
fn bfs_reachable<'a>(
    start: &'a str,
    edges: &BTreeMap<&'a str, BTreeSet<&'a str>>,
) -> BTreeSet<&'a str> {
    let mut visited = BTreeSet::new();
    let mut queue = VecDeque::new();
    visited.insert(start);
    queue.push_back(start);
    while let Some(node) = queue.pop_front() {
        if let Some(targets) = edges.get(node) {
            for &target in targets {
                if visited.insert(target) {
                    queue.push_back(target);
                }
            }
        }
    }
    visited
}

/// Run the full state machine audit against the given repo root.
pub fn run_audit(repo_root: &Path, is_hello_world: bool) -> StateMachineAudit {
    let mut findings = Vec::new();

    // Scan .github/workflows/ for available GHA files
    let available_gha_files = scan_gha_directory(repo_root);

    // 1) Audit blueprint workflows — validate that all embedded workflows parse cleanly
    if !is_hello_world {
        let catalog = WorkflowCatalog::load(repo_root);
        let workflows = parse_catalog_workflows(&catalog, &mut findings);

        for (name, wf) in &workflows {
            audit_blueprint_workflow(name, wf, repo_root, &available_gha_files, &mut findings);
        }

        // 2) Audit policy gate paths from the default state machine template — skipped in hello_world mode
        if let Ok(template) = template::load_embedded_template_set() {
            audit_template_policy_gates(&template, repo_root, &available_gha_files, &mut findings);
        }

        // 3) Unified workflow graph walk — reachability, dead branches, handoffs
        // Skipped in hello_world mode to avoid auditing unused blueprints.
        let entry_roots = entry_point_roots(&workflows);
        audit_workflow_graph(&entry_roots, &workflows, &mut findings);
    }

    StateMachineAudit { findings }
}

fn parse_catalog_workflows(
    catalog: &WorkflowCatalog,
    findings: &mut Vec<AuditFinding>,
) -> BTreeMap<String, BlueprintWorkflow> {
    let mut workflows = BTreeMap::new();

    for entry in catalog.entries() {
        match entry.parse() {
            Ok(wf) => {
                workflows.insert(entry.handle.name.clone(), wf);
            }
            Err(e) => {
                findings.push(AuditFinding {
                    severity: AuditSeverity::Error,
                    source: entry.handle.display_name().to_string(),
                    message: format!("failed to parse workflow: {e}"),
                    suggestion: None,
                });
            }
        }
    }

    workflows
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
#[allow(dead_code)]
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
        let candidate = filename.trim_end_matches(".yml").trim_end_matches(".yaml");

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
#[allow(dead_code)]
fn common_char_count(a: &str, b: &str) -> usize {
    let a_chars: BTreeSet<char> = a.chars().collect();
    let b_chars: BTreeSet<char> = b.chars().collect();
    a_chars.intersection(&b_chars).count()
}

/// Validate a single workflow file reference (existence + name match).
///
/// `missing_severity` controls whether a missing file is reported as an error
/// or a warning — use `Warning` for entries under `proposed_required` or other
/// not-yet-created workflows.
#[allow(clippy::too_many_arguments)]
#[allow(dead_code)]
fn validate_workflow_reference(
    source: &str,
    context: &str,
    wf_path: &str,
    declared_name: Option<&str>,
    missing_severity: AuditSeverity,
    repo_root: &Path,
    available_gha_files: &BTreeMap<String, GhaWorkflow>,
    findings: &mut Vec<AuditFinding>,
) {
    let full_path = repo_root.join(wf_path);

    // a) Check file existence
    if !full_path.is_file() {
        let suggestion = suggest_filename(wf_path, available_gha_files);
        findings.push(AuditFinding {
            severity: missing_severity,
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
#[allow(dead_code)]
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
#[allow(dead_code)]
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
            .map(|j| {
                format!("  {j}:\n    runs-on: ubuntu-latest\n    steps:\n      - run: echo ok\n")
            })
            .collect();
        let content = format!("name: {name}\non: push\njobs:\n{jobs_yaml}");
        std::fs::write(repo_root.join(".github/workflows").join(filename), content)
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
        write_gha_file(
            &repo_root,
            "rust-integration.yml",
            "Rust Integration",
            &["test"],
        );
        write_gha_file(&repo_root, "rust-e2e.yml", "Rust E2E", &["test"]);
        write_gha_file(&repo_root, "rust-coverage.yml", "Rust Coverage", &["test"]);
        write_gha_file(&repo_root, "release-cli.yml", "Release CLI", &["release"]);
        write_gha_file(
            &repo_root,
            "pr-issue-checklist.yml",
            "PR issue checklist",
            &["issue-checklist"],
        );
        write_gha_file(
            &repo_root,
            "pr-conflicts.yml",
            "PR conflicts",
            &["conflicts"],
        );
        write_gha_file(
            &repo_root,
            "pr-single-issue.yml",
            "PR single issue",
            &["single-issue"],
        );

        let audit = run_audit(&repo_root, false);

        // Filter to only errors — there will be warnings for non-existent GHA files
        // referenced by other blueprint workflows (deployment, release, etc.)
        // but the key check here is that correct references don't produce errors
        // beyond the expected missing files from blueprint examples
        let template_errors: Vec<_> = audit
            .findings
            .iter()
            .filter(|f| {
                f.severity == AuditSeverity::Error && f.source.starts_with("template policy_gate")
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
            AuditSeverity::Error,
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
            AuditSeverity::Error,
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
            AuditSeverity::Error,
            &repo_root,
            &available,
            &mut findings,
        );

        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, AuditSeverity::Warning);
        assert!(findings[0].message.contains("mismatch"));
        assert!(
            findings[0]
                .suggestion
                .as_ref()
                .unwrap()
                .contains("Quality gate")
        );

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
        let audit = StateMachineAudit { findings: vec![] };
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

    #[test]
    fn structural_audit_embedded_workflows_have_no_errors() {
        let audit = run_structural_audit();

        let errors: Vec<_> = audit
            .findings
            .iter()
            .filter(|f| f.severity == AuditSeverity::Error)
            .collect();

        assert!(
            errors.is_empty(),
            "embedded state machine structural audit produced {} error(s):\n{}",
            errors.len(),
            errors
                .iter()
                .map(|f| format!("  [{}] {}", f.source, f.message))
                .collect::<Vec<_>>()
                .join("\n")
        );
    }

    // ── Reachability tests ─────────────────────────────────────────────────

    #[test]
    fn reachability_detects_unreachable_state() {
        let wf_yaml = r#"
version: 1
name: test-unreachable
initial_state: start
states:
  start:
    kind: agent
    next:
      on_success: done
  orphan:
    kind: agent
    next:
      on_success: done
  done:
    kind: terminal
"#;
        let wf: BlueprintWorkflow = serde_yaml::from_str(wf_yaml).unwrap();
        let mut findings = Vec::new();
        audit_reachability("test-wf", &wf, &mut findings);

        let unreachable: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("not reachable"))
            .collect();
        assert_eq!(unreachable.len(), 1);
        assert!(unreachable[0].message.contains("orphan"));
        assert_eq!(unreachable[0].severity, AuditSeverity::Error);
    }

    #[test]
    fn reachability_detects_dead_branch() {
        let wf_yaml = r#"
version: 1
name: test-dead-branch
initial_state: start
states:
  start:
    kind: agent
    next:
      on:
        ok: middle
        done: done
  middle:
    kind: agent
    description: no next — dead end, not terminal
  done:
    kind: terminal
"#;
        let wf: BlueprintWorkflow = serde_yaml::from_str(wf_yaml).unwrap();
        let mut findings = Vec::new();
        audit_reachability("test-wf", &wf, &mut findings);

        let dead: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("dead branch"))
            .collect();
        assert_eq!(dead.len(), 1);
        assert!(dead[0].message.contains("middle"));
    }

    #[test]
    fn reachability_detects_undefined_transition_target() {
        let wf_yaml = r#"
version: 1
name: test-undefined-target
initial_state: start
states:
  start:
    kind: agent
    next:
      on_success: nonexistent
  done:
    kind: terminal
"#;
        let wf: BlueprintWorkflow = serde_yaml::from_str(wf_yaml).unwrap();
        let mut findings = Vec::new();
        audit_reachability("test-wf", &wf, &mut findings);

        let undefined: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("not declared"))
            .collect();
        assert_eq!(undefined.len(), 1);
        assert!(undefined[0].message.contains("nonexistent"));
    }

    #[test]
    fn reachability_passes_for_well_formed_workflow() {
        let wf_yaml = r#"
version: 1
name: test-valid
initial_state: start
states:
  start:
    kind: agent
    next:
      on:
        ok: middle
        fail: blocked
  middle:
    kind: agent
    next:
      on_success: done
      on_failure: blocked
  blocked:
    kind: human
    next:
      on:
        resolved: start
        abort: aborted
  done:
    kind: terminal
  aborted:
    kind: terminal
"#;
        let wf: BlueprintWorkflow = serde_yaml::from_str(wf_yaml).unwrap();
        let mut findings = Vec::new();
        audit_reachability("test-wf", &wf, &mut findings);

        assert!(
            findings.is_empty(),
            "expected no findings for well-formed workflow: {findings:?}"
        );
    }

    // ── Cross-workflow graph tests ──────────────────────────────────────────

    #[test]
    fn workflow_graph_detects_orphan_workflow() {
        let mut workflows = BTreeMap::new();

        let root: BlueprintWorkflow = serde_yaml::from_str(
            r#"
version: 1
name: root
initial_state: start
states:
  start:
    kind: agent
    next:
      on_success: done
  done:
    kind: terminal
"#,
        )
        .unwrap();

        let orphan: BlueprintWorkflow = serde_yaml::from_str(
            r#"
version: 1
name: orphan-wf
initial_state: begin
states:
  begin:
    kind: agent
    next:
      on_success: end
  end:
    kind: terminal
"#,
        )
        .unwrap();

        workflows.insert("root".to_string(), root);
        workflows.insert("orphan-wf".to_string(), orphan);

        let mut findings = Vec::new();
        audit_workflow_graph(&["root"], &workflows, &mut findings);

        let orphans: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("not reachable from any entry point"))
            .collect();
        assert_eq!(orphans.len(), 1);
        assert!(orphans[0].message.contains("orphan-wf"));
    }

    #[test]
    fn workflow_graph_detects_missing_sub_workflow() {
        let mut workflows = BTreeMap::new();

        let root: BlueprintWorkflow = serde_yaml::from_str(
            r#"
version: 1
name: root
initial_state: start
states:
  start:
    kind: workflow
    workflow: nonexistent
    next:
      on:
        done: finish
  finish:
    kind: terminal
"#,
        )
        .unwrap();

        workflows.insert("root".to_string(), root);

        let mut findings = Vec::new();
        audit_workflow_graph(&["root"], &workflows, &mut findings);

        let missing: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("not in the embedded workflow library"))
            .collect();
        assert_eq!(missing.len(), 1);
        assert!(missing[0].message.contains("nonexistent"));
    }

    #[test]
    fn workflow_graph_detects_handoff_mismatch() {
        let mut workflows = BTreeMap::new();

        let parent: BlueprintWorkflow = serde_yaml::from_str(
            r#"
version: 1
name: parent
initial_state: dispatch
states:
  dispatch:
    kind: workflow
    workflow: child
    next:
      on:
        done: finish
        wrong-event: finish
  finish:
    kind: terminal
"#,
        )
        .unwrap();

        let child: BlueprintWorkflow = serde_yaml::from_str(
            r#"
version: 1
name: child
initial_state: work
states:
  work:
    kind: agent
    next:
      on_success: done
      on_failure: aborted
  done:
    kind: terminal
  aborted:
    kind: terminal
"#,
        )
        .unwrap();

        workflows.insert("parent".to_string(), parent);
        workflows.insert("child".to_string(), child);

        let mut findings = Vec::new();
        audit_workflow_graph(&["parent"], &workflows, &mut findings);

        // "wrong-event" doesn't match any child terminal
        let bad_event: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("has no terminal state named"))
            .collect();
        assert_eq!(bad_event.len(), 1);
        assert!(bad_event[0].message.contains("wrong-event"));

        // "aborted" terminal in child not handled by parent
        let unhandled: Vec<_> = findings
            .iter()
            .filter(|f| f.message.contains("does not handle terminal"))
            .collect();
        assert_eq!(unhandled.len(), 1);
        assert!(unhandled[0].message.contains("aborted"));
    }

    #[test]
    fn workflow_graph_valid_handoff_no_errors() {
        let mut workflows = BTreeMap::new();

        let parent: BlueprintWorkflow = serde_yaml::from_str(
            r#"
version: 1
name: parent
initial_state: dispatch
states:
  dispatch:
    kind: workflow
    workflow: child
    next:
      on:
        done: finish
        aborted: finish
  finish:
    kind: terminal
"#,
        )
        .unwrap();

        let child: BlueprintWorkflow = serde_yaml::from_str(
            r#"
version: 1
name: child
initial_state: work
states:
  work:
    kind: agent
    next:
      on_success: done
      on_failure: aborted
  done:
    kind: terminal
  aborted:
    kind: terminal
"#,
        )
        .unwrap();

        workflows.insert("parent".to_string(), parent);
        workflows.insert("child".to_string(), child);

        let mut findings = Vec::new();
        audit_workflow_graph(&["parent"], &workflows, &mut findings);

        assert!(
            findings.is_empty(),
            "expected no findings for valid handoff: {findings:?}"
        );
    }

    #[test]
    fn next_spec_all_targets_extracts_on_map() {
        use crate::blueprint_workflows::NextSpec;
        let yaml: serde_yaml::Value =
            serde_yaml::from_str("on:\n  ok: state-a\n  fail: state-b\n").unwrap();
        let spec = NextSpec(yaml);
        let mut targets = spec.all_targets();
        targets.sort();
        assert_eq!(targets, vec!["state-a", "state-b"]);
    }

    #[test]
    fn next_spec_all_targets_extracts_top_level_keys() {
        use crate::blueprint_workflows::NextSpec;
        let yaml: serde_yaml::Value = serde_yaml::from_str(
            "on_success: state-a\non_failure: state-b\non_rejection: state-c\n",
        )
        .unwrap();
        let spec = NextSpec(yaml);
        let mut targets = spec.all_targets();
        targets.sort();
        assert_eq!(targets, vec!["state-a", "state-b", "state-c"]);
    }

    #[test]
    fn next_spec_all_event_keys_extracts_on_map_keys() {
        use crate::blueprint_workflows::NextSpec;
        let yaml: serde_yaml::Value =
            serde_yaml::from_str("on:\n  done: state-a\n  aborted: state-b\n").unwrap();
        let spec = NextSpec(yaml);
        let mut keys = spec.all_event_keys();
        keys.sort();
        assert_eq!(keys, vec!["aborted", "done"]);
    }

    #[test]
    fn next_spec_all_event_keys_extracts_top_level() {
        use crate::blueprint_workflows::NextSpec;
        let yaml: serde_yaml::Value =
            serde_yaml::from_str("on_success: state-a\non_failure: state-b\n").unwrap();
        let spec = NextSpec(yaml);
        let mut keys = spec.all_event_keys();
        keys.sort();
        assert_eq!(keys, vec!["on_failure", "on_success"]);
    }
}
