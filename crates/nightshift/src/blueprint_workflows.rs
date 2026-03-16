//! Embedded blueprint workflow YAML files from the calypso-blueprint submodule.
//!
//! This module provides compile-time access to all `calypso-*.yaml` workflow files.
//! Use [`BlueprintWorkflowLibrary`] to enumerate, look up, and parse them.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

// ── Embedded YAML content ────────────────────────────────────────────────────

const CALYPSO_DEFAULT_DEPLOYMENT_WORKFLOW: &str = include_str!(
    "../../../calypso-blueprint/examples/workflows/calypso-default-deployment-workflow.yaml"
);
const CALYPSO_DEFAULT_FEATURE_WORKFLOW: &str = include_str!(
    "../../../calypso-blueprint/examples/workflows/calypso-default-feature-workflow.yaml"
);
const CALYPSO_DEPLOYMENT_REQUEST: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-deployment-request.yaml");
const CALYPSO_FEATURE_REQUEST: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-feature-request.yaml");
const CALYPSO_IMPLEMENTATION_LOOP: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-implementation-loop.yaml");
const CALYPSO_ORCHESTRATOR_STARTUP: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-orchestrator-startup.yaml");
const CALYPSO_PLANNING: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-planning.yaml");
const CALYPSO_PR_REVIEW_MERGE: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-pr-review-merge.yaml");
const CALYPSO_RELEASE_REQUEST: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-release-request.yaml");
const CALYPSO_SAVE_STATE: &str =
    include_str!("../../../calypso-blueprint/examples/workflows/calypso-save-state.yaml");

// ── Library ──────────────────────────────────────────────────────────────────

/// Static registry of all embedded `calypso-*.yaml` blueprint workflow files.
pub struct BlueprintWorkflowLibrary;

impl BlueprintWorkflowLibrary {
    /// Returns all embedded workflows as `(filename_stem, raw_yaml)` pairs.
    pub fn list() -> &'static [(&'static str, &'static str)] {
        &[
            (
                "calypso-default-deployment-workflow",
                CALYPSO_DEFAULT_DEPLOYMENT_WORKFLOW,
            ),
            (
                "calypso-default-feature-workflow",
                CALYPSO_DEFAULT_FEATURE_WORKFLOW,
            ),
            ("calypso-deployment-request", CALYPSO_DEPLOYMENT_REQUEST),
            ("calypso-feature-request", CALYPSO_FEATURE_REQUEST),
            ("calypso-implementation-loop", CALYPSO_IMPLEMENTATION_LOOP),
            ("calypso-orchestrator-startup", CALYPSO_ORCHESTRATOR_STARTUP),
            ("calypso-planning", CALYPSO_PLANNING),
            ("calypso-pr-review-merge", CALYPSO_PR_REVIEW_MERGE),
            ("calypso-release-request", CALYPSO_RELEASE_REQUEST),
            ("calypso-save-state", CALYPSO_SAVE_STATE),
        ]
    }

    /// Look up a workflow by its filename stem (e.g. `"calypso-planning"`).
    pub fn get(name: &str) -> Option<&'static str> {
        Self::list()
            .iter()
            .find(|(stem, _)| *stem == name)
            .map(|(_, yaml)| *yaml)
    }

    /// Parse a raw YAML string into a [`BlueprintWorkflow`].
    pub fn parse(yaml: &str) -> Result<BlueprintWorkflow, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }
}

// ── Top-level document ───────────────────────────────────────────────────────

/// A blueprint workflow document (one `calypso-*.yaml` file).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintWorkflow {
    pub version: Option<u32>,
    pub name: Option<String>,
    pub initial_state: Option<String>,

    // Optional top-level blocks — only some workflows define each one.
    pub feature_unit: Option<FeatureUnit>,
    pub prd_requirements: Option<PrdRequirements>,
    pub pull_request_template: Option<PullRequestTemplate>,
    pub plan: Option<PlanConfig>,
    pub trigger: Option<TriggerConfig>,
    pub schedule: Option<ScheduleConfig>,
    pub release_requirements: Option<ReleaseRequirements>,
    pub artifact_requirements: Option<ArtifactRequirements>,
    pub rollout_order: Option<Vec<String>>,

    /// States keyed by state name.
    #[serde(default)]
    pub states: HashMap<String, StateConfig>,

    /// Checks keyed by check name.
    #[serde(default)]
    pub checks: HashMap<String, CheckConfig>,

    /// Agent role prompts keyed by role name.
    #[serde(default)]
    pub agent_prompts: HashMap<String, String>,

    pub hard_gates: Option<HardGates>,
    pub github_actions: Option<GitHubActions>,
}

// ── feature_unit ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureUnit {
    pub invariant: Option<String>,
    pub branch_from: Option<String>,
    pub branch_required: Option<bool>,
    pub worktree_required: Option<bool>,
    pub push_to_origin_required: Option<bool>,
    pub pull_request_required: Option<bool>,
}

// ── prd_requirements ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrdRequirements {
    pub read_before_feature_definition: Option<bool>,
    pub source_documents: Option<Vec<String>>,
}

// ── pull_request_template ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullRequestTemplate {
    pub required_sections: Option<Vec<PrSection>>,
    pub completion_checks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrSection {
    pub id: Option<String>,
    pub label: Option<String>,
    #[serde(rename = "type")]
    pub section_type: Option<String>,
}

// ── plan ─────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlanConfig {
    pub document: Option<String>,
    pub issue_tracker: Option<String>,
    pub priority_labels: Option<Vec<String>>,
}

// ── trigger ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    pub event: Option<String>,
    pub pattern: Option<String>,
    pub branch_constraint: Option<String>,
    pub ci_entry: Option<String>,
}

// ── schedule ─────────────────────────────────────────────────────────────────

/// Cron-based schedule for entry-point workflows. The orchestrator evaluates
/// these expressions and dispatches the workflow on match.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    /// Cron expression with second-level granularity: "sec min hour day month weekday".
    /// Examples: `"*/30 * * * * *"` (every 30s), `"0 */5 * * * *"` (every 5 min),
    /// `"0 0 2 * * *"` (daily at 02:00 UTC).
    pub cron: String,
    /// Human-readable description of what this schedule triggers.
    pub description: Option<String>,
}

// ── release_requirements ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseRequirements {
    pub source_branch: Option<String>,
    pub tag_required: Option<bool>,
    pub tag_format: Option<String>,
    pub tag_format_examples: Option<Vec<String>>,
    pub tag_constraints: Option<Vec<String>>,
}

// ── artifact_requirements ────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactRequirements {
    pub github_release_required: Option<bool>,
    pub github_release_asset_required: Option<bool>,
    pub ghcr_images_required: Option<bool>,
    #[serde(rename = "kubernetes-image-ready-signal_required")]
    pub kubernetes_image_ready_signal_required: Option<bool>,
}

// ── states ───────────────────────────────────────────────────────────────────

/// Configuration for a single state in the workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateConfig {
    pub kind: Option<StateKind>,
    pub role: Option<String>,
    pub cost: Option<AgentCost>,
    pub description: Option<String>,
    pub prompt: Option<String>,

    /// For `kind: function` states.
    pub function: Option<String>,
    /// For `kind: deterministic` states with a shell command.
    pub command: Option<String>,
    /// For `kind: workflow` states.
    pub workflow: Option<String>,

    /// For `kind: github` states.
    pub actor: Option<String>,
    pub trigger: Option<String>,
    pub workflows: Option<Vec<WorkflowRef>>,
    pub poll_cmd: Option<String>,

    /// CI job specification (used in calypso-release-request).
    pub ci_job: Option<serde_yaml::Value>,

    /// Checks that must pass to complete a deterministic or github state.
    pub checks: Option<Vec<String>>,

    /// Completion criteria for agent/human states.
    pub completion: Option<CompletionCriteria>,

    /// Cleanup commands run after the state exits.
    pub cleanup: Option<Vec<CleanupStep>>,

    /// Inline gates attached to this state (used in calypso-save-state).
    pub gates: Option<Vec<serde_yaml::Value>>,

    /// Transition spec — multiple YAML shapes are handled via `serde_yaml::Value`.
    pub next: Option<NextSpec>,
}

/// The kind of actor or evaluation strategy for a state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StateKind {
    Deterministic,
    Agent,
    Human,
    Github,
    Function,
    Workflow,
    Terminal,
    #[serde(rename = "git-hook")]
    GitHook,
    Ci,
}

/// Cost tier for agent states.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AgentCost {
    Guru,
    Default,
    Cheap,
}

/// Reference to a GitHub Actions workflow path and check names.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkflowRef {
    pub path: Option<String>,
    pub check_names: Option<Vec<String>>,
}

/// Completion criteria: `all_of`, `any_of`, or both.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionCriteria {
    pub all_of: Option<Vec<String>>,
    pub any_of: Option<Vec<String>>,
}

/// A cleanup command run after a state exits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CleanupStep {
    pub cmd: Option<String>,
    pub purpose: Option<String>,
}

// ── next (transition spec) ───────────────────────────────────────────────────
//
// The `next` field appears in several incompatible shapes across workflow files:
//
//   { on: { event: target, ... } }           — map of event → target state
//   { on_success: state, on_failure: state }  — binary success/failure routing
//   { on_pass: state, on_fail: state }        — deterministic pass/fail routing
//   { pass: state, fail: state }              — github state pass/fail routing
//   { on_complete: state }                    — single terminal transition
//   { on_success: state }                     — one-sided (human states)
//   { on_rejection: state, on_failure: state} — commit state
//
// Rather than fighting the serde untagged enum machinery for mutually-ambiguous
// maps, we store the raw YAML value and expose typed accessors.

/// Raw transition specification — parsed from whatever shape appears in YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextSpec(pub serde_yaml::Value);

impl NextSpec {
    /// Returns the target state for a named transition outcome, e.g. `"on_success"`,
    /// `"pass"`, `"on_complete"`, or an arbitrary `on:` event name.
    pub fn target_for(&self, outcome: &str) -> Option<&str> {
        let map = self.0.as_mapping()?;

        // Direct top-level key (on_success, on_failure, pass, fail, on_complete, …)
        let key = serde_yaml::Value::String(outcome.to_owned());
        if let Some(v) = map.get(&key) {
            return v.as_str();
        }

        // Nested under `on:` (the event-dispatch shape)
        let on_key = serde_yaml::Value::String("on".to_owned());
        if let Some(serde_yaml::Value::Mapping(on_map)) = map.get(&on_key) {
            let ev_key = serde_yaml::Value::String(outcome.to_owned());
            if let Some(v) = on_map.get(&ev_key) {
                return v.as_str();
            }
        }

        None
    }

    /// Returns all event key names from this transition spec.
    ///
    /// For `{ on: { event: target } }` → returns the event names.
    /// For `{ on_success: s, on_failure: s }` → returns `["on_success", "on_failure"]`.
    /// This is useful for cross-workflow handoff validation where the event keys
    /// should match terminal state names in the sub-workflow.
    pub fn all_event_keys(&self) -> Vec<&str> {
        let mut keys = Vec::new();
        let Some(map) = self.0.as_mapping() else {
            return keys;
        };

        for (key, value) in map {
            if let Some(key_str) = key.as_str() {
                if key_str == "on" {
                    // Nested event dispatch: { on: { event: target, ... } }
                    if let Some(on_map) = value.as_mapping() {
                        for (k, _) in on_map {
                            if let Some(s) = k.as_str() {
                                keys.push(s);
                            }
                        }
                    }
                } else {
                    // Top-level key: on_success, on_failure, etc.
                    keys.push(key_str);
                }
            }
        }

        keys
    }

    /// Returns all target state names reachable from this transition spec.
    ///
    /// Handles all YAML shapes:
    /// - `{ on_success: s, on_failure: s, on_rejection: s, ... }` — top-level string values
    /// - `{ on: { event: target, ... } }` — nested event dispatch map
    /// - `{ pass: s, fail: s }` — github routing
    /// - `{ on_complete: s }` — terminal transition
    pub fn all_targets(&self) -> Vec<&str> {
        let mut targets = Vec::new();
        let Some(map) = self.0.as_mapping() else {
            return targets;
        };

        for (key, value) in map {
            if let Some(key_str) = key.as_str() {
                if key_str == "on" {
                    // Nested event dispatch: { on: { event: target, ... } }
                    if let Some(on_map) = value.as_mapping() {
                        for (_, v) in on_map {
                            if let Some(s) = v.as_str() {
                                targets.push(s);
                            }
                        }
                    }
                } else if let Some(s) = value.as_str() {
                    // Top-level key: on_success, on_failure, pass, fail, on_complete, on_rejection, etc.
                    targets.push(s);
                }
            }
        }

        targets
    }
}

// ── checks ───────────────────────────────────────────────────────────────────

/// Configuration for a single named check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckConfig {
    pub kind: Option<CheckKind>,
    pub role: Option<String>,
    pub description: Option<String>,

    /// Lifecycle status — `"proposed"` checks are not yet implemented and are
    /// skipped by the audit validator.
    pub status: Option<String>,

    // Deterministic check fields
    pub cmd: Option<String>,
    pub source: Option<CheckSource>,
    pub hook: Option<String>,
    pub workflow: Option<String>,
    pub workflow_name: Option<String>,
    pub check_names: Option<Vec<String>>,
    pub builtin: Option<String>,

    // CI job reference (calypso-release-request)
    pub job: Option<String>,
    pub step: Option<String>,

    // Git-hook check fields (calypso-save-state)
    pub blocking: Option<bool>,
    pub behavior: Option<String>,
}

/// The kind of evaluator for a check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CheckKind {
    Deterministic,
    Agent,
    Human,
    Github,
    #[serde(rename = "git-hook")]
    GitHook,
    Ci,
}

/// Where a deterministic check is enforced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CheckSource {
    GitHook,
    Ci,
    Builtin,
}

// ── hard_gates ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HardGates {
    pub git_hooks: Option<Vec<GitHookGate>>,
    pub ci_workflows: Option<Vec<CiWorkflowGate>>,
    pub merge_compatibility: Option<MergeCompatibility>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHookGate {
    pub hook: Option<String>,
    pub blocking: Option<bool>,
    pub checks: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CiWorkflowGate {
    pub workflow: Option<String>,
    pub blocking: Option<bool>,
    pub check_names: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MergeCompatibility {
    pub blocking: Option<bool>,
    pub check: Option<String>,
    pub remediation_state: Option<String>,
}

// ── github_actions ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubActions {
    /// Workflows that are currently required and exist in the repository.
    pub current_required: Option<Vec<GitHubActionEntry>>,
    /// Workflows that are proposed / not yet created.
    pub proposed_required: Option<Vec<GitHubActionEntry>>,
    /// Alternate key used in calypso-release-request.
    pub current: Option<Vec<serde_yaml::Value>>,
    /// Alternate proposed key used in calypso-release-request.
    pub proposed: Option<Vec<serde_yaml::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitHubActionEntry {
    pub workflow: Option<String>,
    pub workflow_name: Option<String>,
    pub check_names: Option<Vec<String>>,
    pub used_for: Option<Vec<String>>,
    pub purpose: Option<String>,
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_embedded_workflows_parse_successfully() {
        for (stem, yaml) in BlueprintWorkflowLibrary::list() {
            let result = BlueprintWorkflowLibrary::parse(yaml);
            assert!(
                result.is_ok(),
                "failed to parse workflow '{stem}': {:?}",
                result.err()
            );
        }
    }

    #[test]
    fn get_returns_yaml_for_known_stem() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-planning");
        assert!(yaml.is_some(), "expected to find calypso-planning");
        assert!(yaml.unwrap().contains("calypso-planning"));
    }

    #[test]
    fn get_returns_none_for_unknown_stem() {
        assert!(BlueprintWorkflowLibrary::get("does-not-exist").is_none());
    }

    #[test]
    fn list_contains_all_ten_workflows() {
        assert_eq!(BlueprintWorkflowLibrary::list().len(), 10);
    }

    #[test]
    fn default_feature_workflow_has_expected_initial_state() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-default-feature-workflow").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        assert_eq!(
            wf.initial_state.as_deref(),
            Some("create-worktree"),
            "unexpected initial_state"
        );
    }

    #[test]
    fn default_feature_workflow_states_are_populated() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-default-feature-workflow").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        assert!(
            !wf.states.is_empty(),
            "expected at least one state in the feature workflow"
        );
    }

    #[test]
    fn default_feature_workflow_checks_are_populated() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-default-feature-workflow").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        assert!(
            !wf.checks.is_empty(),
            "expected at least one check in the feature workflow"
        );
    }

    #[test]
    fn next_spec_target_for_resolves_on_success() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-default-feature-workflow").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        let state = wf.states.get("write-failing-tests").unwrap();
        let next = state.next.as_ref().unwrap();
        assert_eq!(
            next.target_for("on_success"),
            Some("implementation-loop"),
            "expected on_success → implementation-loop"
        );
    }

    #[test]
    fn next_spec_target_for_resolves_on_event() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-default-feature-workflow").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        let state = wf.states.get("write-failing-tests").unwrap();
        let next = state.next.as_ref().unwrap();
        assert_eq!(
            next.target_for("on_failure"),
            Some("blocked"),
            "expected on_failure → blocked"
        );
    }
}
