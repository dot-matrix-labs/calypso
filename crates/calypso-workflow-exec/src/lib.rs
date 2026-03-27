//! Workflow interpreter — drives execution through the unified YAML workflow graph.
//!
//! The interpreter loads workflows from a shared catalog, resolves `kind: workflow`
//! references via a call stack, and tracks execution position as
//! `(workflow_name, state_name)` pairs.
//!
//! At startup, callers should:
//! 1. Call [`WorkflowInterpreter::entry_points`] to discover what can be launched.
//! 2. Present [`EntryPoint::UserAction`] entries as a menu of available actions.
//! 3. Schedule [`EntryPoint::CronScheduled`] entries using [`next_fire_in`].
//! 4. Register listeners for [`EntryPoint::EventTriggered`] entries.
//! 5. Auto-launch [`EntryPoint::AutoStart`] entries immediately.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use calypso_workflows::{StateConfig, StateKind, Workflow, WorkflowCatalog};

// ── Entry points ─────────────────────────────────────────────────────────────

/// A classified entry point discovered by scanning all workflows in the registry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryPoint {
    /// Initial state is `kind: human` — user must act to start this workflow.
    UserAction {
        workflow: String,
        description: Option<String>,
        prompt: Option<String>,
    },
    /// Has a `trigger:` block — started by an external event (e.g. `git-tag-push`).
    EventTriggered {
        workflow: String,
        event: String,
        pattern: Option<String>,
    },
    /// Has a `schedule:` block — started automatically on a cron schedule.
    CronScheduled {
        workflow: String,
        cron: String,
        description: Option<String>,
    },
    /// No explicit trigger — starts automatically (initial state is agent- or function-driven).
    AutoStart {
        workflow: String,
        description: Option<String>,
    },
}

/// Compute the [`std::time::Duration`] until the next fire time for a cron expression.
///
/// The expression uses 6-field second-level granularity:
/// `"sec min hour day month weekday"`.
///
/// Examples:
/// - `"*/1 * * * * *"` — every second
/// - `"0 */5 * * * *"` — every 5 minutes
/// - `"0 0 2 * * *"` — daily at 02:00 UTC
pub fn next_fire_in(cron_expr: &str) -> Result<std::time::Duration, String> {
    use chrono::Utc;
    let schedule: cron::Schedule = cron_expr
        .parse()
        .map_err(|e| format!("invalid cron expression '{cron_expr}': {e}"))?;
    let now = Utc::now();
    schedule
        .upcoming(Utc)
        .next()
        .map(|t| {
            let delta = t - now;
            delta.to_std().unwrap_or(std::time::Duration::ZERO)
        })
        .ok_or_else(|| format!("cron expression '{cron_expr}' has no upcoming fires"))
}

// ── Position & call stack ────────────────────────────────────────────────────

/// Position in the workflow graph — a workflow name and state name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowPosition {
    pub workflow: String,
    pub state: String,
}

/// A frame in the workflow call stack. When a `kind: workflow` state delegates
/// to a sub-workflow, a frame is pushed recording where to return to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallFrame {
    /// The parent workflow name.
    pub workflow: String,
    /// The parent state that delegated (kind: workflow).
    pub state: String,
}

/// Persistent execution state for the workflow interpreter.
/// Stored in `.calypso/workflow-state.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowExecutionState {
    /// Current position in the workflow graph.
    pub position: WorkflowPosition,
    /// Call stack for nested workflow delegation.
    pub call_stack: Vec<CallFrame>,
}

// ── Step outcome ─────────────────────────────────────────────────────────────

/// The result of advancing one step in the workflow.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StepOutcome {
    /// Moved to a new state within the current workflow.
    Advanced(WorkflowPosition),
    /// Entered a sub-workflow via `kind: workflow` delegation.
    EnteredSubWorkflow {
        parent: WorkflowPosition,
        child: WorkflowPosition,
    },
    /// A sub-workflow reached a terminal state, popped back to parent.
    ReturnedToParent {
        terminal_state: String,
        parent: WorkflowPosition,
    },
    /// Reached a terminal state at the root level — execution complete.
    Terminal(WorkflowPosition),
    /// Error — invalid state, missing workflow, etc.
    Error(String),
}

// ── Registry ─────────────────────────────────────────────────────────────────

/// Registry of all loaded workflow definitions.
pub struct WorkflowRegistry {
    workflows: BTreeMap<String, Workflow>,
}

impl WorkflowRegistry {
    /// Load workflows from a shared catalog into the registry.
    pub fn from_catalog(catalog: &WorkflowCatalog) -> Result<Self, String> {
        let mut workflows = BTreeMap::new();
        for entry in catalog.entries() {
            let wf = entry
                .parse()
                .map_err(|e| format!("failed to parse workflow '{}': {e}", entry.handle.name))?;
            workflows.insert(entry.handle.name.clone(), wf);
        }
        Ok(Self { workflows })
    }

    /// Look up a workflow by name.
    pub fn get(&self, name: &str) -> Option<&Workflow> {
        self.workflows.get(name)
    }

    /// Returns all workflow names.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.workflows.keys().map(|s| s.as_str())
    }

    /// Look up a state config within a workflow.
    pub fn get_state(&self, workflow: &str, state: &str) -> Option<&StateConfig> {
        self.workflows.get(workflow)?.states.get(state)
    }

    /// Return the [`ExecutionTarget`] for the named state within the named workflow.
    ///
    /// Routes execution based on the explicit `execution_target` field embedded in
    /// the state's [`StateConfig`], which is derived from the `runs-on:` field of
    /// the corresponding GHA job during workflow parsing.
    ///
    /// Returns [`ExecutionTarget::Local`] when the workflow or state is not found,
    /// matching the safe-default documented in [`ExecutionTarget`].
    pub fn execution_target_for(&self, workflow: &str, state: &str) -> ExecutionTarget {
        self.get_state(workflow, state)
            .map(|s| s.execution_target.clone())
            .unwrap_or_default()
    }

    /// Returns the set of workflow names referenced as `workflow:` targets in
    /// `kind: workflow` states across all loaded workflows.
    ///
    /// These are pure sub-workflows — they are only ever called by the workflow
    /// engine, not directly by a user or scheduler.
    pub fn sub_workflow_names(&self) -> BTreeSet<String> {
        let mut names = BTreeSet::new();
        for wf in self.workflows.values() {
            for state in wf.states.values() {
                if matches!(state.kind, Some(StateKind::Workflow))
                    && let Some(ref target) = state.workflow
                {
                    names.insert(target.clone());
                }
            }
        }
        names
    }

    /// Scan all workflows and return classified [`EntryPoint`]s.
    ///
    /// Workflows that are exclusively used as sub-workflows (referenced via
    /// `kind: workflow` states) are excluded unless they carry an explicit
    /// `trigger` or `schedule` block.
    pub fn entry_points(&self) -> Vec<EntryPoint> {
        let sub_names = self.sub_workflow_names();
        let mut entries = Vec::new();

        for (name, wf) in &self.workflows {
            // Skip pure sub-workflows unless they carry explicit trigger/schedule.
            let is_sub = sub_names.contains(name);
            if is_sub && wf.schedule.is_none() && wf.trigger.is_none() {
                continue;
            }

            if let Some(schedule) = &wf.schedule {
                entries.push(EntryPoint::CronScheduled {
                    workflow: name.clone(),
                    cron: schedule.cron.clone(),
                    description: schedule.description.clone(),
                });
            } else if let Some(trigger) = &wf.trigger {
                if let Some(event) = &trigger.event {
                    entries.push(EntryPoint::EventTriggered {
                        workflow: name.clone(),
                        event: event.clone(),
                        pattern: trigger.pattern.clone(),
                    });
                } else {
                    entries.push(EntryPoint::UserAction {
                        workflow: name.clone(),
                        description: None,
                        prompt: None,
                    });
                }
            } else {
                let initial_cfg = wf.initial_state.as_deref().and_then(|s| wf.states.get(s));
                let initial_kind = initial_cfg.and_then(|c| c.kind.clone());

                if matches!(initial_kind, Some(StateKind::Human)) {
                    entries.push(EntryPoint::UserAction {
                        workflow: name.clone(),
                        description: initial_cfg.and_then(|c| c.description.clone()),
                        prompt: initial_cfg.and_then(|c| c.prompt.clone()),
                    });
                } else {
                    entries.push(EntryPoint::AutoStart {
                        workflow: name.clone(),
                        description: initial_cfg.and_then(|c| c.description.clone()),
                    });
                }
            }
        }

        entries
    }
}

// ── Interpreter ──────────────────────────────────────────────────────────────

/// The workflow interpreter. Drives execution through the YAML workflow graph.
pub struct WorkflowInterpreter {
    pub registry: WorkflowRegistry,
}

impl WorkflowInterpreter {
    /// Create an interpreter from a shared workflow catalog.
    pub fn from_catalog(catalog: &WorkflowCatalog) -> Result<Self, String> {
        Ok(Self {
            registry: WorkflowRegistry::from_catalog(catalog)?,
        })
    }

    /// Return all entry points discovered across all loaded workflows.
    pub fn entry_points(&self) -> Vec<EntryPoint> {
        self.registry.entry_points()
    }

    /// Create an execution state starting at the initial state of the named workflow.
    pub fn start(&self, workflow_name: &str) -> Result<WorkflowExecutionState, String> {
        let wf = self
            .registry
            .get(workflow_name)
            .ok_or_else(|| format!("workflow '{workflow_name}' not found"))?;
        let initial = wf
            .initial_state
            .as_deref()
            .ok_or_else(|| format!("workflow '{workflow_name}' has no initial_state"))?;
        Ok(WorkflowExecutionState {
            position: WorkflowPosition {
                workflow: workflow_name.to_string(),
                state: initial.to_string(),
            },
            call_stack: Vec::new(),
        })
    }

    /// Get the StateConfig for the current position.
    pub fn current_state_config<'a>(
        &'a self,
        exec: &WorkflowExecutionState,
    ) -> Option<&'a StateConfig> {
        self.registry
            .get_state(&exec.position.workflow, &exec.position.state)
    }

    /// Get the StateKind of the current position.
    pub fn current_kind(&self, exec: &WorkflowExecutionState) -> Option<StateKind> {
        self.current_state_config(exec)?.kind.clone()
    }

    /// Advance the execution state given an outcome event name.
    ///
    /// The `event` is the outcome of the current state's execution:
    /// - For `kind: agent`: `"on_success"` or `"on_failure"`
    /// - For `kind: deterministic`: `"on_pass"`, `"on_fail"`, or an `on:` event key
    /// - For `kind: human`: an `on:` event key like `"issue-cleared"`, `"abort"`
    /// - For `kind: github`: `"on_pass"`, `"on_fail"`, `"pass"`, `"fail"`
    /// - For `kind: terminal`: should not be called (already terminal)
    /// - For `kind: workflow`: should not be called directly — handled automatically
    pub fn advance(&self, exec: &mut WorkflowExecutionState, event: &str) -> StepOutcome {
        let cfg = match self.current_state_config(exec) {
            Some(cfg) => cfg,
            None => {
                return StepOutcome::Error(format!(
                    "state '{}' not found in workflow '{}'",
                    exec.position.state, exec.position.workflow
                ));
            }
        };

        let next = match &cfg.next {
            Some(next) => next,
            None => {
                return StepOutcome::Error(format!(
                    "state '{}' in '{}' has no next transitions",
                    exec.position.state, exec.position.workflow
                ));
            }
        };

        let target = match next.target_for(event) {
            Some(t) => t.to_string(),
            None => {
                return StepOutcome::Error(format!(
                    "state '{}' in '{}' has no transition for event '{event}'",
                    exec.position.state, exec.position.workflow
                ));
            }
        };

        // Look up the target state config
        let target_cfg = match self.registry.get_state(&exec.position.workflow, &target) {
            Some(cfg) => cfg,
            None => {
                return StepOutcome::Error(format!(
                    "transition target '{target}' not found in workflow '{}'",
                    exec.position.workflow
                ));
            }
        };

        let target_kind = target_cfg.kind.clone();

        // Update position to the target state
        exec.position.state = target.clone();
        let new_pos = exec.position.clone();

        match target_kind {
            Some(StateKind::Terminal) => self.handle_terminal(exec, target, new_pos),
            Some(StateKind::Workflow) => self.handle_workflow_entry(exec, target, target_cfg),
            _ => StepOutcome::Advanced(new_pos),
        }
    }

    /// Handle arriving at a terminal state — pop call stack or finish.
    fn handle_terminal(
        &self,
        exec: &mut WorkflowExecutionState,
        terminal_name: String,
        terminal_pos: WorkflowPosition,
    ) -> StepOutcome {
        let frame = match exec.call_stack.pop() {
            Some(f) => f,
            None => return StepOutcome::Terminal(terminal_pos),
        };

        // Pop back to parent — use the terminal state name as the event
        let parent_cfg = match self.registry.get_state(&frame.workflow, &frame.state) {
            Some(cfg) => cfg,
            None => {
                return StepOutcome::Error(format!(
                    "parent state '{}' in '{}' not found when popping call stack",
                    frame.state, frame.workflow
                ));
            }
        };

        let parent_next = match &parent_cfg.next {
            Some(n) => n,
            None => {
                return StepOutcome::Error(format!(
                    "parent state '{}' in '{}' has no next transitions",
                    frame.state, frame.workflow
                ));
            }
        };

        let parent_target = match parent_next.target_for(&terminal_name) {
            Some(t) => t.to_string(),
            None => {
                return StepOutcome::Error(format!(
                    "parent state '{}' in '{}' has no handler for terminal '{terminal_name}'",
                    frame.state, frame.workflow
                ));
            }
        };

        exec.position = WorkflowPosition {
            workflow: frame.workflow,
            state: parent_target,
        };

        StepOutcome::ReturnedToParent {
            terminal_state: terminal_name,
            parent: exec.position.clone(),
        }
    }

    /// Handle arriving at a `kind: workflow` state — push frame, enter sub-workflow.
    fn handle_workflow_entry(
        &self,
        exec: &mut WorkflowExecutionState,
        delegating_state: String,
        target_cfg: &StateConfig,
    ) -> StepOutcome {
        let sub_wf_name = match &target_cfg.workflow {
            Some(name) => name.clone(),
            None => {
                return StepOutcome::Error(format!(
                    "state '{delegating_state}' is kind: workflow but has no workflow field"
                ));
            }
        };

        let sub_wf = match self.registry.get(&sub_wf_name) {
            Some(wf) => wf,
            None => {
                return StepOutcome::Error(format!(
                    "sub-workflow '{sub_wf_name}' not found in registry"
                ));
            }
        };

        let sub_initial = match &sub_wf.initial_state {
            Some(s) => s.clone(),
            None => {
                return StepOutcome::Error(format!(
                    "sub-workflow '{sub_wf_name}' has no initial_state"
                ));
            }
        };

        // Push call frame
        let parent_pos = exec.position.clone();
        exec.call_stack.push(CallFrame {
            workflow: exec.position.workflow.clone(),
            state: delegating_state,
        });

        exec.position = WorkflowPosition {
            workflow: sub_wf_name,
            state: sub_initial,
        };

        StepOutcome::EnteredSubWorkflow {
            parent: parent_pos,
            child: exec.position.clone(),
        }
    }
}

// ── WorkflowCatalog ──────────────────────────────────────────────────────────

/// The canonical names of `.calypso/` configuration files that are NOT workflow
/// definitions.  These are excluded from legacy layout detection so they never
/// get misidentified as workflow YAML files.
const NON_WORKFLOW_CALYPSO_FILES: &[&str] = &[
    "state-machine.yml",
    "state-machine.yaml",
    "agents.yml",
    "agents.yaml",
    "prompts.yml",
    "prompts.yaml",
    "headless-state.json",
    "init-state.json",
    "repository-state.json",
    "workflow-state.json",
    "dev-state.json",
    "keys.json",
    "pending-event.json",
    "pending-cron.json",
];

/// A workflow catalog that loads repository-local workflows strictly from
/// `.calypso/workflows/`.
///
/// # Discovery contract
///
/// - Local workflows are loaded from `<repo_root>/.calypso/workflows/*.yml|*.yaml`.
/// - The root `.calypso/` directory is **never** scanned for workflow files.
///   Files placed at `.calypso/*.yml` (the legacy layout) are silently ignored by
///   the runtime.  Use [`detect_legacy_local_workflows`] to surface migration
///   guidance when a legacy layout is detected.
/// - Embedded blueprint workflows (from `BlueprintWorkflowLibrary`) are always
///   available as a fallback when a name is not found in local files.
pub struct WorkflowCatalog {
    /// Workflows loaded from `.calypso/workflows/`.
    pub local: BTreeMap<String, BlueprintWorkflow>,
    /// The embedded registry — provides all blueprint workflows.
    pub embedded: WorkflowRegistry,
}

impl WorkflowCatalog {
    /// Load local workflows from `<repo_root>/.calypso/workflows/` and combine
    /// them with the embedded blueprint registry.
    ///
    /// YAML files in `.calypso/` root are **not** loaded — only
    /// `.calypso/workflows/*.yml|*.yaml` is considered.
    pub fn load(repo_root: &Path) -> Result<Self, String> {
        let embedded = WorkflowRegistry::from_embedded()?;
        let workflows_dir = repo_root.join(".calypso").join("workflows");
        let mut local = BTreeMap::new();

        if let Ok(entries) = std::fs::read_dir(&workflows_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let is_yaml = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|e| e == "yml" || e == "yaml")
                    .unwrap_or(false);
                if !is_yaml {
                    continue;
                }
                let yaml = match std::fs::read_to_string(&path) {
                    Ok(y) => y,
                    Err(_) => continue,
                };
                let wf = match BlueprintWorkflowLibrary::parse(&yaml) {
                    Ok(w) => w,
                    Err(_) => continue,
                };
                let stem = path
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or_default()
                    .to_string();
                let name = wf.name.clone().unwrap_or(stem);
                local.insert(name, wf);
            }
        }

        Ok(Self { local, embedded })
    }

    /// Look up a workflow by name, preferring local definitions over embedded ones.
    pub fn get(&self, name: &str) -> Option<&BlueprintWorkflow> {
        self.local.get(name).or_else(|| self.embedded.get(name))
    }

    /// Return all workflow names visible through this catalog.
    ///
    /// Local workflow names shadow embedded ones of the same name.
    pub fn names(&self) -> impl Iterator<Item = &str> {
        let local_names: BTreeSet<&str> = self.local.keys().map(|s| s.as_str()).collect();
        let embedded_names = self
            .embedded
            .names()
            .filter(move |n| !local_names.contains(*n));
        self.local.keys().map(|s| s.as_str()).chain(embedded_names)
    }

    /// Return the [`ExecutionTarget`] for the named state within the named workflow.
    ///
    /// Prefers the local workflow definition over the embedded one, so a
    /// repository-local copy of a workflow and the embedded blueprint copy yield
    /// the same routing decision when they carry identical `runs-on:` metadata.
    ///
    /// Returns [`ExecutionTarget::Local`] when the workflow or state is not found.
    pub fn execution_target_for(&self, workflow: &str, state: &str) -> ExecutionTarget {
        self.get(workflow)
            .and_then(|wf| wf.states.get(state))
            .map(|s| s.execution_target.clone())
            .unwrap_or_default()
    }
}

/// Detect YAML files placed at `.calypso/*.yml|*.yaml` that look like legacy
/// local workflow definitions.
///
/// Returns the basenames of any YAML files found in the `.calypso/` root that
/// are not in the known list of non-workflow configuration files.  An empty
/// result means no legacy layout was detected.
///
/// These files are **never** loaded as workflows by the runtime.  When this
/// function returns a non-empty list the caller should surface migration
/// guidance to the repository owner:
///
/// ```text
/// Move .calypso/<file>.yml to .calypso/workflows/<file>.yml to register it
/// as a local workflow definition.
/// ```
pub fn detect_legacy_local_workflows(repo_root: &Path) -> Vec<String> {
    let calypso_dir = repo_root.join(".calypso");
    let mut found = Vec::new();

    let entries = match std::fs::read_dir(&calypso_dir) {
        Ok(e) => e,
        Err(_) => return found,
    };

    for entry in entries.flatten() {
        let path = entry.path();

        // Only files directly under `.calypso/` — not subdirectories.
        if !path.is_file() {
            continue;
        }

        let is_yaml = path
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e == "yml" || e == "yaml")
            .unwrap_or(false);
        if !is_yaml {
            continue;
        }

        let filename = match path.file_name().and_then(|f| f.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if NON_WORKFLOW_CALYPSO_FILES.contains(&filename) {
            continue;
        }

        found.push(filename.to_string());
    }

    found.sort();
    found
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use calypso_workflows::WorkflowCatalog;

    fn embedded_registry() -> WorkflowRegistry {
        WorkflowRegistry::from_catalog(&WorkflowCatalog::embedded())
            .expect("embedded workflow catalog should load")
    }

    fn embedded_interpreter() -> WorkflowInterpreter {
        WorkflowInterpreter::from_catalog(&WorkflowCatalog::embedded())
            .expect("embedded workflow catalog should load")
    }

    // ── Registry & startup ───────────────────────────────────────────────────

    #[test]
    fn registry_loads_all_embedded_workflows() {
        let registry = embedded_registry();
        assert!(registry.get("calypso-orchestrator-startup").is_some());
        assert!(registry.get("calypso-planning").is_some());
        assert!(registry.get("calypso-default-feature-workflow").is_some());
        assert!(registry.get("calypso-implementation-loop").is_some());
        assert!(registry.get("calypso-save-state").is_some());
    }

    #[test]
    fn start_orchestrator_at_initial_state() {
        let interp = embedded_interpreter();
        let exec = interp.start("calypso-orchestrator-startup").unwrap();
        assert_eq!(exec.position.workflow, "calypso-orchestrator-startup");
        assert_eq!(exec.position.state, "scan-work-queue");
        assert!(exec.call_stack.is_empty());
    }

    #[test]
    fn start_any_workflow_by_name() {
        let interp = embedded_interpreter();
        let exec = interp.start("calypso-planning").unwrap();
        assert_eq!(exec.position.workflow, "calypso-planning");
        assert!(!exec.position.state.is_empty());
        assert!(exec.call_stack.is_empty());
    }

    #[test]
    fn start_unknown_workflow_returns_error() {
        let interp = embedded_interpreter();
        assert!(interp.start("does-not-exist").is_err());
    }

    // ── Entry point discovery ────────────────────────────────────────────────

    #[test]
    fn sub_workflow_names_excludes_top_level_workflows() {
        let registry = embedded_registry();
        let subs = registry.sub_workflow_names();
        // These are called by other workflows — they are sub-workflows.
        assert!(subs.contains("calypso-planning"));
        assert!(subs.contains("calypso-pr-review-merge"));
        assert!(subs.contains("calypso-default-feature-workflow"));
        assert!(subs.contains("calypso-implementation-loop"));
        assert!(subs.contains("calypso-save-state"));
        // These are standalone — not referenced as sub-workflows.
        assert!(!subs.contains("calypso-orchestrator-startup"));
        assert!(!subs.contains("calypso-release-request"));
        assert!(!subs.contains("calypso-deployment-request"));
        assert!(!subs.contains("calypso-feature-request"));
    }

    #[test]
    fn entry_points_excludes_pure_sub_workflows() {
        let interp = embedded_interpreter();
        let entries = interp.entry_points();
        let names: Vec<&str> = entries
            .iter()
            .map(|e| match e {
                EntryPoint::UserAction { workflow, .. } => workflow.as_str(),
                EntryPoint::EventTriggered { workflow, .. } => workflow.as_str(),
                EntryPoint::CronScheduled { workflow, .. } => workflow.as_str(),
                EntryPoint::AutoStart { workflow, .. } => workflow.as_str(),
            })
            .collect();
        // Sub-workflows with explicit triggers remain valid entry points.
        assert!(names.contains(&"calypso-planning"));
        assert!(names.contains(&"calypso-implementation-loop"));
        assert!(names.contains(&"calypso-save-state"));
    }

    #[test]
    fn release_request_is_event_triggered() {
        let interp = embedded_interpreter();
        let entry = interp
            .entry_points()
            .into_iter()
            .find(|e| matches!(e, EntryPoint::EventTriggered { workflow, .. } if workflow == "calypso-release-request"));
        assert!(
            entry.is_some(),
            "expected calypso-release-request as EventTriggered"
        );
        if let Some(EntryPoint::EventTriggered { event, .. }) = entry {
            assert_eq!(event, "git-tag-push");
        }
    }

    #[test]
    fn deployment_request_is_user_action() {
        let interp = embedded_interpreter();
        let entry = interp
            .entry_points()
            .into_iter()
            .find(|e| matches!(e, EntryPoint::UserAction { workflow, .. } if workflow == "calypso-deployment-request"));
        assert!(
            entry.is_some(),
            "expected calypso-deployment-request as UserAction"
        );
    }

    #[test]
    fn feature_request_is_user_action() {
        let interp = embedded_interpreter();
        let entry = interp
            .entry_points()
            .into_iter()
            .find(|e| matches!(e, EntryPoint::UserAction { workflow, .. } if workflow == "calypso-feature-request"));
        assert!(
            entry.is_some(),
            "expected calypso-feature-request as UserAction"
        );
    }

    #[test]
    fn orchestrator_startup_is_cron_scheduled() {
        let interp = embedded_interpreter();
        let entry = interp
            .entry_points()
            .into_iter()
            .find(|e| matches!(e, EntryPoint::CronScheduled { workflow, .. } if workflow == "calypso-orchestrator-startup"));
        assert!(
            entry.is_some(),
            "expected calypso-orchestrator-startup as CronScheduled"
        );
    }

    // ── Cron scheduling ──────────────────────────────────────────────────────

    #[test]
    fn next_fire_in_every_second() {
        let dur = next_fire_in("*/1 * * * * *").unwrap();
        assert!(dur.as_secs() <= 1, "expected fire within 1s, got {dur:?}");
    }

    #[test]
    fn next_fire_in_invalid_expression_returns_error() {
        assert!(next_fire_in("not-a-cron").is_err());
    }

    // ── Advance / state machine execution ────────────────────────────────────

    #[test]
    fn advance_to_non_workflow_state() {
        let interp = embedded_interpreter();
        let mut exec = interp.start("calypso-orchestrator-startup").unwrap();

        // scan-work-queue → idle (on no-pending-tasks)
        let outcome = interp.advance(&mut exec, "no-pending-tasks");
        assert!(matches!(outcome, StepOutcome::Advanced(_)));
        assert_eq!(exec.position.state, "idle");
    }

    #[test]
    fn advance_enters_sub_workflow() {
        let interp = embedded_interpreter();
        let mut exec = interp.start("calypso-orchestrator-startup").unwrap();

        // scan-work-queue → dispatch-planning (enters calypso-planning)
        let outcome = interp.advance(&mut exec, "planning-task-identified");
        match outcome {
            StepOutcome::EnteredSubWorkflow { parent, child } => {
                assert_eq!(parent.state, "dispatch-planning");
                assert_eq!(child.workflow, "calypso-planning");
                assert_eq!(child.state, "fetch-open-issues");
            }
            other => panic!("expected EnteredSubWorkflow, got {other:?}"),
        }
        assert_eq!(exec.call_stack.len(), 1);
        assert_eq!(exec.call_stack[0].workflow, "calypso-orchestrator-startup");
        assert_eq!(exec.call_stack[0].state, "dispatch-planning");
    }

    #[test]
    fn advance_enters_development_sub_workflow() {
        let interp = embedded_interpreter();
        let mut exec = interp.start("calypso-orchestrator-startup").unwrap();

        // scan-work-queue → assign-phase-issues
        let outcome = interp.advance(&mut exec, "development-task-identified");
        assert!(matches!(outcome, StepOutcome::Advanced(_)));
        assert_eq!(exec.position.state, "assign-phase-issues");

        // assign-phase-issues → dispatch-development (enters calypso-default-feature-workflow)
        let outcome = interp.advance(&mut exec, "on_success");
        match outcome {
            StepOutcome::EnteredSubWorkflow { child, .. } => {
                assert_eq!(child.workflow, "calypso-default-feature-workflow");
                assert_eq!(child.state, "create-worktree");
            }
            other => panic!("expected EnteredSubWorkflow, got {other:?}"),
        }
    }

    #[test]
    fn terminal_at_root_returns_terminal() {
        let interp = embedded_interpreter();
        let mut exec = interp.start("calypso-orchestrator-startup").unwrap();

        // scan-work-queue → idle
        interp.advance(&mut exec, "no-pending-tasks");
        // idle → done (terminal at root)
        let outcome = interp.advance(&mut exec, "shutdown-requested");
        assert!(matches!(outcome, StepOutcome::Terminal(_)));
        assert_eq!(exec.position.state, "done");
    }

    #[test]
    fn invalid_event_returns_error() {
        let interp = embedded_interpreter();
        let mut exec = interp.start("calypso-orchestrator-startup").unwrap();
        let outcome = interp.advance(&mut exec, "nonexistent-event");
        assert!(matches!(outcome, StepOutcome::Error(_)));
    }

    #[test]
    fn sub_workflow_terminal_pops_call_stack() {
        let interp = embedded_interpreter();
        let mut exec = interp.start("calypso-orchestrator-startup").unwrap();

        // Enter planning sub-workflow
        interp.advance(&mut exec, "planning-task-identified");
        assert_eq!(exec.position.workflow, "calypso-planning");
        assert_eq!(exec.call_stack.len(), 1);

        // Walk planning: fetch → reconcile → check-orphans → reprioritize → validate → commit → done
        interp.advance(&mut exec, "on_success"); // → reconcile-plan
        interp.advance(&mut exec, "on_success"); // → check-orphans
        interp.advance(&mut exec, "on_pass"); // → reprioritize
        interp.advance(&mut exec, "on_success"); // → validate-plan
        interp.advance(&mut exec, "on_success"); // → commit-plan

        // commit-plan → done (terminal in sub-workflow → pop to parent)
        let outcome = interp.advance(&mut exec, "on_success");
        match outcome {
            StepOutcome::ReturnedToParent {
                terminal_state,
                parent,
            } => {
                assert_eq!(terminal_state, "done");
                assert_eq!(parent.workflow, "calypso-orchestrator-startup");
                assert_eq!(parent.state, "scan-work-queue");
            }
            other => panic!("expected ReturnedToParent, got {other:?}"),
        }
        assert!(exec.call_stack.is_empty());
    }

    #[test]
    fn current_kind_returns_state_kind() {
        let interp = embedded_interpreter();
        let exec = interp.start("calypso-orchestrator-startup").unwrap();
        assert_eq!(interp.current_kind(&exec), Some(StateKind::Agent));
    }

    #[test]
    fn execution_state_serializes_roundtrip() {
        let state = WorkflowExecutionState {
            position: WorkflowPosition {
                workflow: "calypso-orchestrator-startup".to_string(),
                state: "scan-work-queue".to_string(),
            },
            call_stack: vec![CallFrame {
                workflow: "root".to_string(),
                state: "dispatch".to_string(),
            }],
        };
        let json = serde_json::to_string(&state).unwrap();
        let round: WorkflowExecutionState = serde_json::from_str(&json).unwrap();
        assert_eq!(state, round);
    }

    #[test]
    fn nested_workflow_depth_two() {
        // orchestrator → feature-workflow → implementation-loop
        let interp = embedded_interpreter();
        let mut exec = interp.start("calypso-orchestrator-startup").unwrap();

        // Enter development path
        interp.advance(&mut exec, "development-task-identified"); // → assign-phase-issues
        interp.advance(&mut exec, "on_success"); // → enters calypso-default-feature-workflow
        assert_eq!(exec.position.workflow, "calypso-default-feature-workflow");
        assert_eq!(exec.call_stack.len(), 1);

        // Walk feature workflow to implementation-loop
        interp.advance(&mut exec, "on_success"); // create-worktree → review-issue
        interp.advance(&mut exec, "phase-implementation"); // review-issue → implementation-loop

        // implementation-loop is kind: workflow → enters calypso-implementation-loop
        assert_eq!(exec.position.workflow, "calypso-implementation-loop");
        assert_eq!(exec.position.state, "write-increment");
        assert_eq!(exec.call_stack.len(), 2); // orchestrator + feature-workflow
    }

    // ── WorkflowCatalog ──────────────────────────────────────────────────────

    fn unique_temp_dir(label: &str) -> std::path::PathBuf {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        std::env::temp_dir().join(format!("calypso-catalog-{label}-{nanos}"))
    }

    #[test]
    fn catalog_load_with_no_local_dir_falls_back_to_embedded() {
        let tmp = unique_temp_dir("no-local");
        // No .calypso/workflows/ directory — catalog should still return embedded workflows.
        let catalog = WorkflowCatalog::load(&tmp).unwrap();
        assert!(
            catalog.get("calypso-orchestrator-startup").is_some(),
            "expected embedded fallback for calypso-orchestrator-startup"
        );
        assert!(catalog.local.is_empty(), "expected no local workflows");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn catalog_load_reads_local_yml_from_workflows_subdir() {
        let tmp = unique_temp_dir("local-yml");
        let workflows_dir = tmp.join(".calypso").join("workflows");
        std::fs::create_dir_all(&workflows_dir).unwrap();

        // A minimal GHA-format local workflow.
        let yaml = "name: my-local-wf\non:\n  workflow_dispatch:\njobs:\n  start:\n    runs-on: ubuntu-latest\n    steps:\n      - id: run\n        uses: ./.github/actions/calypso-agent\n        with:\n          role: engineer\n          prompt: start work\n";
        std::fs::write(workflows_dir.join("my-local-wf.yml"), yaml).unwrap();

        let catalog = WorkflowCatalog::load(&tmp).unwrap();
        assert!(
            catalog.local.contains_key("my-local-wf"),
            "expected local workflow to be loaded"
        );
        assert!(
            catalog.get("my-local-wf").is_some(),
            "expected get() to return local workflow"
        );
        // Embedded workflows still accessible.
        assert!(
            catalog.get("calypso-planning").is_some(),
            "expected embedded fallback still accessible"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn catalog_load_ignores_non_yaml_files_in_workflows_subdir() {
        let tmp = unique_temp_dir("non-yaml");
        let workflows_dir = tmp.join(".calypso").join("workflows");
        std::fs::create_dir_all(&workflows_dir).unwrap();

        std::fs::write(workflows_dir.join("readme.txt"), "not yaml").unwrap();
        std::fs::write(workflows_dir.join("notes.md"), "## notes").unwrap();

        let catalog = WorkflowCatalog::load(&tmp).unwrap();
        assert!(
            catalog.local.is_empty(),
            "expected non-YAML files to be ignored"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn catalog_load_does_not_scan_calypso_root_for_workflows() {
        // Regression: YAML files placed directly under .calypso/ (legacy layout)
        // must never be picked up as workflow definitions.
        let tmp = unique_temp_dir("legacy-root");
        let calypso_dir = tmp.join(".calypso");
        std::fs::create_dir_all(&calypso_dir).unwrap();

        // Write a workflow-like YAML directly in .calypso/ root (legacy layout).
        let yaml = "name: legacy-workflow\non:\n  workflow_dispatch:\njobs:\n  start:\n    runs-on: ubuntu-latest\n    steps:\n      - id: run\n        run: echo start\n        shell: bash\n";
        std::fs::write(calypso_dir.join("legacy-workflow.yml"), yaml).unwrap();

        let catalog = WorkflowCatalog::load(&tmp).unwrap();
        assert!(
            !catalog.local.contains_key("legacy-workflow"),
            "legacy .calypso/*.yml must not be loaded as a workflow"
        );
        assert!(
            catalog.get("legacy-workflow").is_none(),
            "legacy .calypso/*.yml must not be reachable via get()"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── detect_legacy_local_workflows ────────────────────────────────────────

    #[test]
    fn detect_legacy_returns_empty_when_no_calypso_dir() {
        let tmp = unique_temp_dir("no-calypso");
        let result = detect_legacy_local_workflows(&tmp);
        assert!(result.is_empty(), "expected empty result for missing dir");
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn detect_legacy_returns_empty_when_calypso_dir_has_no_yml() {
        let tmp = unique_temp_dir("no-yml");
        std::fs::create_dir_all(tmp.join(".calypso")).unwrap();
        std::fs::write(tmp.join(".calypso").join("readme.txt"), "notes").unwrap();
        let result = detect_legacy_local_workflows(&tmp);
        assert!(result.is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn detect_legacy_ignores_known_non_workflow_calypso_files() {
        let tmp = unique_temp_dir("known-files");
        let calypso_dir = tmp.join(".calypso");
        std::fs::create_dir_all(&calypso_dir).unwrap();
        for name in NON_WORKFLOW_CALYPSO_FILES {
            std::fs::write(calypso_dir.join(name), "content").unwrap();
        }
        let result = detect_legacy_local_workflows(&tmp);
        assert!(
            result.is_empty(),
            "known config files must not be flagged: {result:?}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn detect_legacy_returns_unknown_yml_files_in_calypso_root() {
        let tmp = unique_temp_dir("has-legacy");
        let calypso_dir = tmp.join(".calypso");
        std::fs::create_dir_all(&calypso_dir).unwrap();

        std::fs::write(calypso_dir.join("my-workflow.yml"), "name: my-workflow").unwrap();
        std::fs::write(calypso_dir.join("other.yaml"), "name: other").unwrap();

        let result = detect_legacy_local_workflows(&tmp);
        assert!(
            result.contains(&"my-workflow.yml".to_string()),
            "expected my-workflow.yml in result: {result:?}"
        );
        assert!(
            result.contains(&"other.yaml".to_string()),
            "expected other.yaml in result: {result:?}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn detect_legacy_ignores_yml_in_workflows_subdir() {
        // Files under .calypso/workflows/ are in the correct location and must
        // NOT be flagged as legacy files.
        let tmp = unique_temp_dir("workflows-subdir");
        let workflows_dir = tmp.join(".calypso").join("workflows");
        std::fs::create_dir_all(&workflows_dir).unwrap();

        std::fs::write(workflows_dir.join("good-workflow.yml"), "name: good").unwrap();

        let result = detect_legacy_local_workflows(&tmp);
        assert!(
            result.is_empty(),
            ".calypso/workflows/ yml files must not be flagged: {result:?}"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── Execution-target routing tests ────────────────────────────────────────

    #[test]
    fn registry_execution_target_for_github_state() {
        // Agent states with runs-on in the GHA YAML must resolve to GitHub target.
        let registry = WorkflowRegistry::from_embedded().unwrap();
        let wf = "calypso-orchestrator-startup";
        let target = registry.execution_target_for(wf, "scan-work-queue");
        assert_eq!(
            target,
            ExecutionTarget::GitHub,
            "scan-work-queue uses calypso-agent with runs-on → GitHub"
        );
    }

    #[test]
    fn registry_execution_target_for_local_state() {
        // Workflow-delegation states without runs-on must resolve to Local target.
        let registry = WorkflowRegistry::from_embedded().unwrap();
        let wf = "calypso-orchestrator-startup";
        let target = registry.execution_target_for(wf, "dispatch-planning");
        assert_eq!(
            target,
            ExecutionTarget::Local,
            "dispatch-planning delegates to sub-workflow without runs-on → Local"
        );
    }

    #[test]
    fn registry_execution_target_for_unknown_workflow_defaults_local() {
        let registry = WorkflowRegistry::from_embedded().unwrap();
        let target = registry.execution_target_for("does-not-exist", "any-state");
        assert_eq!(
            target,
            ExecutionTarget::Local,
            "unknown workflow must default to Local"
        );
    }

    #[test]
    fn registry_execution_target_for_unknown_state_defaults_local() {
        let registry = WorkflowRegistry::from_embedded().unwrap();
        let wf = "calypso-orchestrator-startup";
        let target = registry.execution_target_for(wf, "no-such-state");
        assert_eq!(
            target,
            ExecutionTarget::Local,
            "unknown state must default to Local"
        );
    }

    #[test]
    fn embedded_and_local_copy_yield_same_execution_target() {
        // The same workflow YAML loaded as a local file via WorkflowCatalog must
        // produce the same execution_target for a given state as the embedded copy.
        // This proves that routing decisions are source-independent.
        let tmp = unique_temp_dir("exec-target-same");
        let workflows_dir = tmp.join(".calypso").join("workflows");
        std::fs::create_dir_all(&workflows_dir).unwrap();

        // Write the embedded orchestrator YAML as a local file.
        let wf_name = "calypso-orchestrator-startup";
        let yaml = crate::blueprint_workflows::BlueprintWorkflowLibrary::get(wf_name)
            .expect("embedded orchestrator must exist");
        let dest = workflows_dir.join("calypso-orchestrator-startup.yaml");
        std::fs::write(dest, yaml).unwrap();

        let catalog = WorkflowCatalog::load(&tmp).expect("catalog must load");
        let registry = WorkflowRegistry::from_embedded().unwrap();

        // A GitHub-target state: same result from both catalog and registry.
        let cat_gh = catalog.execution_target_for(wf_name, "scan-work-queue");
        let reg_gh = registry.execution_target_for(wf_name, "scan-work-queue");
        assert_eq!(
            cat_gh,
            ExecutionTarget::GitHub,
            "local copy: scan-work-queue must be GitHub"
        );
        // Same target from embedded registry — source-independent routing.
        assert_eq!(cat_gh, reg_gh);

        // A Local-target state: same result from both catalog and registry.
        let cat_lo = catalog.execution_target_for(wf_name, "dispatch-planning");
        let reg_lo = registry.execution_target_for(wf_name, "dispatch-planning");
        assert_eq!(
            cat_lo,
            ExecutionTarget::Local,
            "local copy: dispatch-planning must be Local"
        );
        // Same target from embedded registry — source-independent routing.
        assert_eq!(cat_lo, reg_lo);

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn catalog_execution_target_for_unknown_defaults_local() {
        let tmp = unique_temp_dir("catalog-exec-default");
        std::fs::create_dir_all(tmp.join(".calypso").join("workflows")).unwrap();
        let catalog = WorkflowCatalog::load(&tmp).unwrap();
        let target = catalog.execution_target_for("no-such-workflow", "no-such-state");
        assert_eq!(
            target,
            ExecutionTarget::Local,
            "unknown workflow/state in catalog must default to Local"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
