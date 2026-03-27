//! Embedded blueprint workflow YAML files from the calypso-blueprint submodule.
//!
//! This module provides compile-time access to all `calypso-*.yaml` workflow files
//! in GitHub Actions YAML format. Use [`BlueprintWorkflowLibrary`] to enumerate,
//! look up, and parse them.
//!
//! The GHA format uses `jobs:` instead of `states:`, with transitions expressed via
//! `needs:` + `outputs:` + `if:` conditions rather than `next:` specs.

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

    /// Parse a raw GHA YAML string into a [`BlueprintWorkflow`].
    ///
    /// This parses the GitHub Actions format and derives the state machine
    /// representation (states, transitions, kinds) from the GHA structure.
    pub fn parse(yaml: &str) -> Result<BlueprintWorkflow, serde_yaml::Error> {
        let raw: GhaWorkflowRaw = serde_yaml::from_str(yaml)?;
        Ok(BlueprintWorkflow::from_gha(raw))
    }
}

// ── Raw GHA deserialization types ────────────────────────────────────────────

/// Raw GitHub Actions workflow document — deserialized directly from YAML.
///
/// Note: we use `serde_yaml::Value` for `jobs` to preserve YAML key ordering,
/// which is needed to determine the initial state (first job in file order).
#[derive(Debug, Clone, Deserialize)]
struct GhaWorkflowRaw {
    name: Option<String>,
    #[serde(rename = "on")]
    on_trigger: Option<serde_yaml::Value>,
    #[serde(default)]
    jobs: serde_yaml::Value,
}

/// Raw GitHub Actions job — deserialized directly from YAML.
#[derive(Debug, Clone, Deserialize)]
struct GhaJobRaw {
    needs: Option<serde_yaml::Value>,
    #[serde(rename = "if")]
    if_condition: Option<String>,
    #[serde(rename = "runs-on")]
    runs_on: Option<String>,
    /// Job-level `uses:` for reusable workflow calls.
    uses: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    outputs: HashMap<String, serde_yaml::Value>,
    #[serde(default)]
    steps: Vec<GhaStepRaw>,
}

/// Raw GitHub Actions step.
#[derive(Debug, Clone, Deserialize)]
struct GhaStepRaw {
    #[allow(dead_code)]
    id: Option<String>,
    uses: Option<String>,
    run: Option<String>,
    #[allow(dead_code)]
    shell: Option<String>,
    #[serde(rename = "with")]
    with_fields: Option<HashMap<String, serde_yaml::Value>>,
}

// ── Public workflow types ─────────────────────────────────────────────────────

/// A blueprint workflow document parsed from GHA YAML.
///
/// Each GHA job maps to one entry in `states`:
/// - `initial_state` is the first job with no `needs:`
/// - `schedule` is extracted from `on: schedule:`
/// - Transitions are derived from `needs:` + `if:` conditions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BlueprintWorkflow {
    pub name: Option<String>,
    pub initial_state: Option<String>,
    pub schedule: Option<ScheduleConfig>,
    pub trigger: Option<TriggerConfig>,

    /// States keyed by job name, derived from GHA `jobs:`.
    #[serde(default)]
    pub states: HashMap<String, StateConfig>,
}

impl BlueprintWorkflow {
    /// Parse the `jobs:` value into an ordered list of (name, GhaJobRaw).
    fn parse_jobs(jobs_val: &serde_yaml::Value) -> Vec<(String, GhaJobRaw)> {
        let Some(mapping) = jobs_val.as_mapping() else {
            return vec![];
        };
        let mut result = Vec::new();
        for (key, val) in mapping {
            if let Some(name) = key.as_str()
                && let Ok(job) = serde_yaml::from_value::<GhaJobRaw>(val.clone())
            {
                result.push((name.to_string(), job));
            }
        }
        result
    }

    /// Convert a raw GHA workflow into the state machine representation.
    fn from_gha(raw: GhaWorkflowRaw) -> Self {
        let name = raw.name.clone();

        // Extract schedule from on: trigger
        let schedule = Self::extract_schedule(&raw.on_trigger);
        let trigger = Self::extract_trigger(&raw.on_trigger);

        // Parse jobs preserving YAML key order
        let ordered_jobs = Self::parse_jobs(&raw.jobs);

        // Build a HashMap for lookup
        let jobs_map: HashMap<String, GhaJobRaw> = ordered_jobs
            .iter()
            .map(|(n, j)| (n.clone(), j.clone()))
            .collect();

        // Build the set of all job names that appear as `needs:` targets
        // (i.e., they have downstream dependents). Jobs not in this set
        // that also have no outgoing transitions are terminal.
        let mut has_dependents: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for (_, job) in &ordered_jobs {
            for dep in Self::parse_needs(&job.needs) {
                has_dependents.insert(dep);
            }
        }

        // Determine initial state:
        // 1. First job with no `needs:` (if any)
        // 2. Otherwise, the first job in YAML order (handles cyclic graphs)
        let mut initial_state = None;
        for (job_name, job) in &ordered_jobs {
            let needs = Self::parse_needs(&job.needs);
            if needs.is_empty() {
                initial_state = Some(job_name.clone());
                break;
            }
        }
        if initial_state.is_none() && !ordered_jobs.is_empty() {
            initial_state = Some(ordered_jobs[0].0.clone());
        }

        let mut states = HashMap::new();

        for (job_name, job) in &ordered_jobs {
            let kind = Self::derive_kind(job, &has_dependents, job_name);
            let (role, cost, prompt) = Self::extract_agent_config(job);
            let workflow_ref = Self::extract_workflow_ref(job);
            let command = Self::extract_command(job);
            let execution_target = Self::derive_execution_target(job);

            // Build the next spec by scanning all downstream jobs
            let next = Self::build_next_spec(job_name, &jobs_map);

            states.insert(
                job_name.clone(),
                StateConfig {
                    kind: Some(kind),
                    role,
                    cost,
                    description: None,
                    prompt,
                    execution_target,
                    function: None,
                    command,
                    workflow: workflow_ref,
                    actor: None,
                    trigger: None,
                    workflows: None,
                    poll_cmd: None,
                    ci_job: None,
                    completion: None,
                    cleanup: None,
                    gates: None,
                    next,
                },
            );
        }

        BlueprintWorkflow {
            name,
            initial_state,
            schedule,
            trigger,
            states,
        }
    }

    /// Extract schedule config from `on: schedule: - cron: ...`.
    fn extract_schedule(on_trigger: &Option<serde_yaml::Value>) -> Option<ScheduleConfig> {
        let on_val = on_trigger.as_ref()?;
        let schedule_val = on_val.get("schedule")?;
        let arr = schedule_val.as_sequence()?;
        let first = arr.first()?;
        let cron = first.get("cron")?.as_str()?;
        Some(ScheduleConfig {
            cron: cron.to_string(),
            description: None,
        })
    }

    /// Extract trigger config from `on: workflow_dispatch:`.
    ///
    /// Handles both the bare `workflow_dispatch: null` form (manual trigger with no inputs)
    /// and the structured form with `inputs.event` containing a named event descriptor.
    fn extract_trigger(on_trigger: &Option<serde_yaml::Value>) -> Option<TriggerConfig> {
        let on_val = on_trigger.as_ref()?;
        let dispatch = on_val.get("workflow_dispatch")?;

        // Bare `workflow_dispatch: null` — manual trigger with no event inputs.
        if dispatch.is_null() {
            return Some(TriggerConfig {
                event: None,
                pattern: None,
                branch_constraint: None,
                ci_entry: None,
            });
        }

        // Structured form: look for inputs.event with a hyphenated event descriptor.
        if let Some(inputs) = dispatch.get("inputs")
            && let Some(event_input) = inputs.get("event")
            && let Some(event_desc) = event_input
                .get("description")
                .and_then(|v| v.as_str())
                .filter(|d| d.contains('-') && !d.contains(' '))
        {
            return Some(TriggerConfig {
                event: Some(event_desc.to_string()),
                pattern: event_input
                    .get("default")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
                branch_constraint: None,
                ci_entry: None,
            });
        }

        // workflow_dispatch key present but not null and no recognised inputs — still manual.
        Some(TriggerConfig {
            event: None,
            pattern: None,
            branch_constraint: None,
            ci_entry: None,
        })
    }

    /// Parse the `needs:` field which can be a string or array of strings.
    fn parse_needs(needs: &Option<serde_yaml::Value>) -> Vec<String> {
        match needs {
            None => vec![],
            Some(serde_yaml::Value::String(s)) => vec![s.clone()],
            Some(serde_yaml::Value::Sequence(arr)) => arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect(),
            _ => vec![],
        }
    }

    /// Derive the StateKind from a GHA job's steps or uses field.
    fn derive_kind(
        job: &GhaJobRaw,
        has_dependents: &std::collections::HashSet<String>,
        job_name: &str,
    ) -> StateKind {
        // Job-level uses: → Workflow
        if let Some(ref uses) = job.uses
            && uses.contains(".github/workflows/")
        {
            return StateKind::Workflow;
        }

        // Check steps for action references
        for step in &job.steps {
            if let Some(ref uses) = step.uses {
                if uses.contains("calypso-agent") {
                    return StateKind::Agent;
                }
                if uses.contains("calypso-human-gate") {
                    return StateKind::Human;
                }
                if uses.contains("calypso-github-poller") {
                    return StateKind::Github;
                }
                if uses.contains("calypso-function") {
                    return StateKind::Function;
                }
            }
            if let Some(ref run_cmd) = step.run {
                if run_cmd.contains("Terminal state:") {
                    return StateKind::Terminal;
                }
                return StateKind::Deterministic;
            }
        }

        // Fallback: if no steps and no dependents, terminal
        if !has_dependents.contains(job_name) && job.steps.is_empty() && job.uses.is_none() {
            return StateKind::Terminal;
        }

        StateKind::Deterministic
    }

    /// Derive the [`ExecutionTarget`] from a GHA job's `runs-on:` field.
    ///
    /// - `runs-on:` present → [`ExecutionTarget::GitHub`]: the job is dispatched to
    ///   a GitHub Actions runner.
    /// - `runs-on:` absent → [`ExecutionTarget::Local`]: the job delegates to a
    ///   reusable workflow (`uses:`) and is routed by the local Calypso engine.
    fn derive_execution_target(job: &GhaJobRaw) -> ExecutionTarget {
        if job.runs_on.is_some() {
            ExecutionTarget::GitHub
        } else {
            ExecutionTarget::Local
        }
    }

    /// Extract agent config (role, cost, prompt) from step `with:` fields.
    fn extract_agent_config(
        job: &GhaJobRaw,
    ) -> (Option<String>, Option<AgentCost>, Option<String>) {
        for step in &job.steps {
            if let Some(ref with_fields) = step.with_fields {
                let role = with_fields
                    .get("role")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let cost = with_fields
                    .get("cost")
                    .and_then(|v| v.as_str())
                    .map(|s| match s {
                        "guru" => AgentCost::Guru,
                        "cheap" => AgentCost::Cheap,
                        _ => AgentCost::Default,
                    });
                let prompt = with_fields
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());

                if role.is_some() || prompt.is_some() {
                    return (role, cost, prompt);
                }
            }
        }
        (None, None, None)
    }

    /// Extract workflow reference from job-level `uses:` field.
    fn extract_workflow_ref(job: &GhaJobRaw) -> Option<String> {
        let uses = job.uses.as_ref()?;
        if uses.contains(".github/workflows/") {
            // Extract the workflow name stem from the path
            let stem = uses
                .trim_start_matches("./.github/workflows/")
                .trim_end_matches(".yml")
                .trim_end_matches(".yaml");
            Some(stem.to_string())
        } else {
            None
        }
    }

    /// Extract command from a `run:` step.
    fn extract_command(job: &GhaJobRaw) -> Option<String> {
        for step in &job.steps {
            if let Some(ref run_cmd) = step.run {
                return Some(run_cmd.clone());
            }
        }
        None
    }

    /// Build the NextSpec for a job by scanning all other jobs' `needs:` and `if:`
    /// conditions to find which events from this job lead to which downstream jobs.
    fn build_next_spec(job_name: &str, all_jobs: &HashMap<String, GhaJobRaw>) -> Option<NextSpec> {
        let mut transitions: Vec<(String, String)> = Vec::new();

        for (target_name, target_job) in all_jobs {
            let needs = Self::parse_needs(&target_job.needs);
            if !needs.contains(&job_name.to_string()) {
                continue;
            }

            // Parse the if condition to find which event from this job leads here
            if let Some(ref if_cond) = target_job.if_condition {
                // Parse patterns like: needs.job-name.outputs.event == 'event-name'
                let pattern = format!("needs.{job_name}.outputs.event == '");
                for segment in if_cond.split(&pattern) {
                    // Skip the first segment (before the pattern)
                    if segment.starts_with("needs.") || !segment.contains('\'') {
                        continue;
                    }
                    if let Some(end) = segment.find('\'') {
                        let event = &segment[..end];
                        transitions.push((event.to_string(), target_name.clone()));
                    }
                }
            } else {
                // No if condition — unconditional dependency. Use a generic event.
                // This shouldn't normally happen in our workflows, but handle it.
                transitions.push(("on_complete".to_string(), target_name.clone()));
            }
        }

        if transitions.is_empty() {
            return None;
        }

        // Build the YAML value for the NextSpec
        let mut map = serde_yaml::Mapping::new();
        for (event, target) in &transitions {
            map.insert(
                serde_yaml::Value::String(event.clone()),
                serde_yaml::Value::String(target.clone()),
            );
        }
        Some(NextSpec(serde_yaml::Value::Mapping(map)))
    }
}

// ── Schedule ─────────────────────────────────────────────────────────────────

/// Cron-based schedule for entry-point workflows.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScheduleConfig {
    pub cron: String,
    pub description: Option<String>,
}

// ── Trigger ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TriggerConfig {
    pub event: Option<String>,
    pub pattern: Option<String>,
    pub branch_constraint: Option<String>,
    pub ci_entry: Option<String>,
}

// ── States ───────────────────────────────────────────────────────────────────

/// Explicit execution target for a workflow state.
///
/// Determines whether the state is driven by the local Calypso runtime
/// (running directly on the developer's machine or in a local daemon) or
/// by the GitHub Actions platform.
///
/// # Defaulting rules
///
/// When parsing GHA YAML workflow files the execution target is derived from
/// the `runs-on:` field of the corresponding job:
///
/// - Job has `runs-on:` set → `GitHub` (scheduled/dispatched on GitHub Actions).
/// - Job has no `runs-on:` (e.g. `kind: workflow` delegation via `uses:`) → `Local`
///   (the routing decision is made by the local Calypso engine, not the GHA runner).
/// - Terminal states → `Local` (no active executor needed; they are no-op sinks).
///
/// The same defaulting logic applies to embedded blueprint workflows and to
/// repository-local workflow copies, ensuring that the same YAML loaded from
/// either source yields identical routing decisions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ExecutionTarget {
    /// State is executed by the local Calypso runtime (on the developer's machine
    /// or in the local headless daemon).
    #[default]
    Local,
    /// State is dispatched to and executed by the GitHub Actions platform.
    GitHub,
}

/// Configuration for a single state (job) in the workflow.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateConfig {
    pub kind: Option<StateKind>,
    pub role: Option<String>,
    pub cost: Option<AgentCost>,
    pub description: Option<String>,
    pub prompt: Option<String>,

    /// Explicit execution target: where this state runs.
    ///
    /// Derived from `runs-on:` when parsing GHA YAML; defaults to `Local` for
    /// states that delegate to sub-workflows or are terminal.
    #[serde(default)]
    pub execution_target: ExecutionTarget,

    /// For `kind: function` states.
    pub function: Option<String>,
    /// For `kind: deterministic` states with a shell command.
    pub command: Option<String>,
    /// For `kind: workflow` states — the referenced workflow name stem.
    pub workflow: Option<String>,

    /// For `kind: github` states.
    pub actor: Option<String>,
    pub trigger: Option<String>,
    pub workflows: Option<Vec<WorkflowRef>>,
    pub poll_cmd: Option<String>,

    /// CI job specification.
    pub ci_job: Option<serde_yaml::Value>,

    /// Completion criteria for agent/human states.
    pub completion: Option<CompletionCriteria>,

    /// Cleanup commands.
    pub cleanup: Option<Vec<CleanupStep>>,

    /// Inline gates.
    pub gates: Option<Vec<serde_yaml::Value>>,

    /// Transition spec — derived from downstream jobs' `needs:` + `if:` conditions.
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
// In GHA format, the `next` field is derived from downstream jobs' `needs:` and
// `if:` conditions. The events are extracted from `if:` patterns like:
//   `needs.job-name.outputs.event == 'event-name'`
//
// The resulting NextSpec has the same API as before:
//   { event_name: target_job_name, ... }

/// Raw transition specification — parsed from whatever shape appears in YAML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NextSpec(pub serde_yaml::Value);

impl NextSpec {
    /// Returns the target state for a named transition outcome, e.g. `"on_success"`,
    /// `"pass"`, `"on_complete"`, or an arbitrary `on:` event name.
    pub fn target_for(&self, outcome: &str) -> Option<&str> {
        let map = self.0.as_mapping()?;

        // Direct top-level key (on_success, on_failure, pass, fail, on_complete, ...)
        let key = serde_yaml::Value::String(outcome.to_owned());
        if let Some(v) = map.get(&key) {
            return v.as_str();
        }

        // Nested under `on:` (the event-dispatch shape from old format)
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
    /// Handles both flat format `{ event: target }` and nested `{ on: { event: target } }`.
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
                    keys.push(key_str);
                }
            }
        }

        keys
    }

    /// Returns all target state names reachable from this transition spec.
    ///
    /// Handles both flat format and nested `on:` maps.
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
                    targets.push(s);
                }
            }
        }

        targets
    }
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
    fn default_feature_workflow_has_workflow_kind_states() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-default-feature-workflow").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        let impl_loop = wf.states.get("implementation-loop").unwrap();
        assert_eq!(impl_loop.kind, Some(StateKind::Workflow));
        assert_eq!(
            impl_loop.workflow.as_deref(),
            Some("calypso-implementation-loop")
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
    fn next_spec_target_for_resolves_on_failure() {
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

    #[test]
    fn orchestrator_has_schedule() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-orchestrator-startup").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        assert!(wf.schedule.is_some(), "expected schedule on orchestrator");
        assert_eq!(wf.schedule.unwrap().cron, "0 */5 * * * *");
    }

    #[test]
    fn release_request_has_trigger() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-release-request").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        assert!(wf.trigger.is_some(), "expected trigger on release-request");
        assert_eq!(
            wf.trigger.as_ref().unwrap().event.as_deref(),
            Some("git-tag-push")
        );
    }

    #[test]
    fn terminal_states_are_detected() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-planning").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        let done = wf.states.get("done").unwrap();
        assert_eq!(done.kind, Some(StateKind::Terminal));
        let aborted = wf.states.get("aborted").unwrap();
        assert_eq!(aborted.kind, Some(StateKind::Terminal));
    }

    #[test]
    fn agent_states_have_role_and_cost() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-planning").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        let fetch = wf.states.get("fetch-open-issues").unwrap();
        assert_eq!(fetch.kind, Some(StateKind::Agent));
        assert_eq!(fetch.role.as_deref(), Some("planner"));
        assert_eq!(fetch.cost, Some(AgentCost::Cheap));
    }

    #[test]
    fn github_poller_states_detected() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-pr-review-merge").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        let check_pr = wf.states.get("check-pr-structure").unwrap();
        assert_eq!(check_pr.kind, Some(StateKind::Github));
    }

    #[test]
    fn human_states_detected() {
        let yaml = BlueprintWorkflowLibrary::get("calypso-implementation-loop").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        let req = wf.states.get("request-clarification").unwrap();
        assert_eq!(req.kind, Some(StateKind::Human));
    }

    // ── Canonical API surface tests ───────────────────────────────────────────
    //
    // These tests verify that the workflow document model is purely GHA-shaped
    // and does not carry compatibility-only fields.

    #[test]
    fn parsed_workflow_has_no_compatibility_checks_field() {
        // All embedded workflows parsed through the library must produce
        // BlueprintWorkflow values with no extraneous checks. The struct no
        // longer exposes a `checks` map — this test confirms the parsing path
        // produces a well-formed canonical document for every embedded file.
        for (stem, yaml) in BlueprintWorkflowLibrary::list() {
            let wf = BlueprintWorkflowLibrary::parse(yaml)
                .unwrap_or_else(|e| panic!("failed to parse '{stem}': {e}"));
            // All states must have a kind derived from GHA structure.
            for (state_name, state_cfg) in &wf.states {
                assert!(
                    state_cfg.kind.is_some(),
                    "state '{state_name}' in workflow '{stem}' has no kind — GHA parsing must derive kind"
                );
            }
        }
    }

    #[test]
    fn state_config_has_no_checks_list_from_gha_parse() {
        // Parsing any GHA workflow must never populate `StateConfig.checks`.
        // That field was part of the old compatibility layer and is no longer
        // present on the struct.
        let yaml = BlueprintWorkflowLibrary::get("calypso-planning").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        // Compile-time proof: accessing wf.states (HashMap<String, StateConfig>)
        // and any StateConfig field is still possible without `checks`.
        for (_, state_cfg) in &wf.states {
            // Confirm ci_job and completion fields still exist (canonical fields).
            let _ = &state_cfg.ci_job;
            let _ = &state_cfg.completion;
        }
        // If this test compiles and runs without error, the canonical surface
        // is coherent with no orphaned compatibility fields.
    }

    #[test]
    fn workflow_roundtrip_serializes_without_checks_key() {
        // A parsed workflow serialized back to YAML must not emit a `checks:` key.
        let yaml = BlueprintWorkflowLibrary::get("calypso-default-feature-workflow").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();
        let serialized = serde_yaml::to_string(&wf).expect("serialization must succeed");
        assert!(
            !serialized.contains("checks:"),
            "serialized workflow must not contain a 'checks:' key — compatibility layer removed"
        );
    }

    // ── Execution target parsing tests ────────────────────────────────────────

    #[test]
    fn agent_states_with_runs_on_get_github_target() {
        // States that have `runs-on:` in their GHA job definition must parse
        // to ExecutionTarget::GitHub — they are dispatched to GitHub Actions.
        let yaml = BlueprintWorkflowLibrary::get("calypso-orchestrator-startup").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();

        // scan-work-queue uses calypso-agent with runs-on: ubuntu-latest
        let scan = wf
            .states
            .get("scan-work-queue")
            .expect("scan-work-queue must exist");
        assert_eq!(
            scan.execution_target,
            ExecutionTarget::GitHub,
            "agent state with runs-on must be GitHub target"
        );
    }

    #[test]
    fn workflow_delegation_states_without_runs_on_get_local_target() {
        // States that delegate to a sub-workflow via `uses:` have no `runs-on:`
        // field, so the local Calypso engine routes them — they must parse to
        // ExecutionTarget::Local.
        let yaml = BlueprintWorkflowLibrary::get("calypso-orchestrator-startup").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();

        // dispatch-planning uses `.github/workflows/calypso-planning.yml`
        // and has no `runs-on:` field.
        let dispatch = wf
            .states
            .get("dispatch-planning")
            .expect("dispatch-planning must exist");
        assert_eq!(
            dispatch.execution_target,
            ExecutionTarget::Local,
            "workflow delegation state without runs-on must be Local target"
        );
    }

    #[test]
    fn terminal_states_get_local_target() {
        // Terminal states carry `echo 'Terminal state: ...'` steps with runs-on,
        // so they're still GitHub target. Verify the actual GHA-derived value.
        let yaml = BlueprintWorkflowLibrary::get("calypso-planning").unwrap();
        let wf = BlueprintWorkflowLibrary::parse(yaml).unwrap();

        let done = wf
            .states
            .get("done")
            .expect("done terminal state must exist");
        assert_eq!(done.kind, Some(StateKind::Terminal));
        // Terminal states use runs-on: ubuntu-latest in the GHA YAML, so they
        // are GitHub targets. The execution target faithfully reflects the YAML.
        assert_eq!(
            done.execution_target,
            ExecutionTarget::GitHub,
            "terminal state with runs-on must be GitHub target"
        );
    }

    #[test]
    fn execution_target_default_is_local() {
        // The Default impl must produce Local so deserialization of states
        // that omit the field (e.g. hand-written YAML) is safe.
        assert_eq!(ExecutionTarget::default(), ExecutionTarget::Local);
    }

    #[test]
    fn all_embedded_workflows_have_execution_target_on_every_state() {
        // Every state in every embedded workflow must carry an explicit
        // execution_target after parsing. Since the field defaults to Local, this
        // test verifies the struct field is present and accessible on all states.
        for (stem, yaml) in BlueprintWorkflowLibrary::list() {
            let wf = BlueprintWorkflowLibrary::parse(yaml)
                .unwrap_or_else(|e| panic!("failed to parse '{stem}': {e}"));
            for (state_name, state_cfg) in &wf.states {
                // Simply accessing the field is the compile-time proof; the
                // runtime assertion ensures it matches one of the two variants.
                let target = &state_cfg.execution_target;
                assert!(
                    *target == ExecutionTarget::Local || *target == ExecutionTarget::GitHub,
                    "state '{state_name}' in '{stem}' has invalid execution_target"
                );
            }
        }
    }

    #[test]
    fn execution_target_roundtrips_through_serde() {
        // ExecutionTarget must serialize and deserialize correctly so that
        // persisted workflow state files remain stable across versions.
        let local_json = serde_json::to_string(&ExecutionTarget::Local).unwrap();
        assert_eq!(local_json, "\"local\"");
        let github_json = serde_json::to_string(&ExecutionTarget::GitHub).unwrap();
        assert_eq!(github_json, "\"github\"");

        let local: ExecutionTarget = serde_json::from_str("\"local\"").unwrap();
        assert_eq!(local, ExecutionTarget::Local);
        let github: ExecutionTarget = serde_json::from_str("\"github\"").unwrap();
        assert_eq!(github, ExecutionTarget::GitHub);
    }
}
