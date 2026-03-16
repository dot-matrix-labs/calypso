//! Workflow interpreter — drives execution through the unified YAML workflow graph.
//!
//! The interpreter loads all embedded blueprint workflows, resolves `kind: workflow`
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

use serde::{Deserialize, Serialize};

use crate::blueprint_workflows::{
    BlueprintWorkflow, BlueprintWorkflowLibrary, StateConfig, StateKind,
};

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
    workflows: BTreeMap<String, BlueprintWorkflow>,
}

impl WorkflowRegistry {
    /// Load all embedded blueprint workflows into the registry.
    pub fn from_embedded() -> Result<Self, String> {
        let mut workflows = BTreeMap::new();
        for (stem, yaml) in BlueprintWorkflowLibrary::list() {
            let wf = BlueprintWorkflowLibrary::parse(yaml)
                .map_err(|e| format!("failed to parse workflow '{stem}': {e}"))?;
            workflows.insert(stem.to_string(), wf);
        }
        Ok(Self { workflows })
    }

    /// Look up a workflow by name.
    pub fn get(&self, name: &str) -> Option<&BlueprintWorkflow> {
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
                entries.push(EntryPoint::EventTriggered {
                    workflow: name.clone(),
                    event: trigger.event.clone().unwrap_or_default(),
                    pattern: trigger.pattern.clone(),
                });
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
    /// Create a new interpreter with all embedded workflows loaded.
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            registry: WorkflowRegistry::from_embedded()?,
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

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── Registry & startup ───────────────────────────────────────────────────

    #[test]
    fn registry_loads_all_embedded_workflows() {
        let registry = WorkflowRegistry::from_embedded().unwrap();
        assert!(registry.get("calypso-orchestrator-startup").is_some());
        assert!(registry.get("calypso-planning").is_some());
        assert!(registry.get("calypso-default-feature-workflow").is_some());
        assert!(registry.get("calypso-implementation-loop").is_some());
        assert!(registry.get("calypso-save-state").is_some());
    }

    #[test]
    fn start_orchestrator_at_initial_state() {
        let interp = WorkflowInterpreter::new().unwrap();
        let exec = interp.start("calypso-orchestrator-startup").unwrap();
        assert_eq!(exec.position.workflow, "calypso-orchestrator-startup");
        assert_eq!(exec.position.state, "scan-work-queue");
        assert!(exec.call_stack.is_empty());
    }

    #[test]
    fn start_any_workflow_by_name() {
        let interp = WorkflowInterpreter::new().unwrap();
        let exec = interp.start("calypso-planning").unwrap();
        assert_eq!(exec.position.workflow, "calypso-planning");
        assert!(!exec.position.state.is_empty());
        assert!(exec.call_stack.is_empty());
    }

    #[test]
    fn start_unknown_workflow_returns_error() {
        let interp = WorkflowInterpreter::new().unwrap();
        assert!(interp.start("does-not-exist").is_err());
    }

    // ── Entry point discovery ────────────────────────────────────────────────

    #[test]
    fn sub_workflow_names_excludes_top_level_workflows() {
        let registry = WorkflowRegistry::from_embedded().unwrap();
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
        let interp = WorkflowInterpreter::new().unwrap();
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
        // Sub-workflows must not appear as entry points.
        assert!(!names.contains(&"calypso-planning"));
        assert!(!names.contains(&"calypso-implementation-loop"));
        assert!(!names.contains(&"calypso-save-state"));
    }

    #[test]
    fn release_request_is_event_triggered() {
        let interp = WorkflowInterpreter::new().unwrap();
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
        let interp = WorkflowInterpreter::new().unwrap();
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
        let interp = WorkflowInterpreter::new().unwrap();
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
        let interp = WorkflowInterpreter::new().unwrap();
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
        let interp = WorkflowInterpreter::new().unwrap();
        let mut exec = interp.start("calypso-orchestrator-startup").unwrap();

        // scan-work-queue → idle (on no-pending-tasks)
        let outcome = interp.advance(&mut exec, "no-pending-tasks");
        assert!(matches!(outcome, StepOutcome::Advanced(_)));
        assert_eq!(exec.position.state, "idle");
    }

    #[test]
    fn advance_enters_sub_workflow() {
        let interp = WorkflowInterpreter::new().unwrap();
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
        let interp = WorkflowInterpreter::new().unwrap();
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
        let interp = WorkflowInterpreter::new().unwrap();
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
        let interp = WorkflowInterpreter::new().unwrap();
        let mut exec = interp.start("calypso-orchestrator-startup").unwrap();
        let outcome = interp.advance(&mut exec, "nonexistent-event");
        assert!(matches!(outcome, StepOutcome::Error(_)));
    }

    #[test]
    fn sub_workflow_terminal_pops_call_stack() {
        let interp = WorkflowInterpreter::new().unwrap();
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
        let interp = WorkflowInterpreter::new().unwrap();
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
        let interp = WorkflowInterpreter::new().unwrap();
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
}
