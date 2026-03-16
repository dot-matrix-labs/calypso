//! Workflow interpreter — drives execution through the unified YAML workflow graph.
//!
//! The interpreter loads all embedded blueprint workflows, resolves `kind: workflow`
//! references via a call stack, and tracks execution position as
//! `(workflow_name, state_name)` pairs.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::blueprint_workflows::{
    BlueprintWorkflow, BlueprintWorkflowLibrary, StateConfig, StateKind,
};

/// The root workflow from which execution begins.
pub const ROOT_WORKFLOW: &str = "calypso-orchestrator-startup";

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

    /// Create the initial execution state — entry point of the root workflow.
    pub fn initial_state(&self) -> Result<WorkflowExecutionState, String> {
        let root = self
            .registry
            .get(ROOT_WORKFLOW)
            .ok_or_else(|| format!("root workflow '{ROOT_WORKFLOW}' not found"))?;
        let initial = root
            .initial_state
            .as_deref()
            .ok_or_else(|| format!("root workflow '{ROOT_WORKFLOW}' has no initial_state"))?;
        Ok(WorkflowExecutionState {
            position: WorkflowPosition {
                workflow: ROOT_WORKFLOW.to_string(),
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
                ))
            }
        };

        let next = match &cfg.next {
            Some(next) => next,
            None => {
                return StepOutcome::Error(format!(
                    "state '{}' in '{}' has no next transitions",
                    exec.position.state, exec.position.workflow
                ))
            }
        };

        let target = match next.target_for(event) {
            Some(t) => t.to_string(),
            None => {
                return StepOutcome::Error(format!(
                    "state '{}' in '{}' has no transition for event '{event}'",
                    exec.position.state, exec.position.workflow
                ))
            }
        };

        // Look up the target state config
        let target_cfg = match self
            .registry
            .get_state(&exec.position.workflow, &target)
        {
            Some(cfg) => cfg,
            None => {
                return StepOutcome::Error(format!(
                    "transition target '{target}' not found in workflow '{}'",
                    exec.position.workflow
                ))
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
                ))
            }
        };

        let parent_next = match &parent_cfg.next {
            Some(n) => n,
            None => {
                return StepOutcome::Error(format!(
                    "parent state '{}' in '{}' has no next transitions",
                    frame.state, frame.workflow
                ))
            }
        };

        let parent_target = match parent_next.target_for(&terminal_name) {
            Some(t) => t.to_string(),
            None => {
                return StepOutcome::Error(format!(
                    "parent state '{}' in '{}' has no handler for terminal '{terminal_name}'",
                    frame.state, frame.workflow
                ))
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
                ))
            }
        };

        let sub_wf = match self.registry.get(&sub_wf_name) {
            Some(wf) => wf,
            None => {
                return StepOutcome::Error(format!(
                    "sub-workflow '{sub_wf_name}' not found in registry"
                ))
            }
        };

        let sub_initial = match &sub_wf.initial_state {
            Some(s) => s.clone(),
            None => {
                return StepOutcome::Error(format!(
                    "sub-workflow '{sub_wf_name}' has no initial_state"
                ))
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

    #[test]
    fn registry_loads_all_embedded_workflows() {
        let registry = WorkflowRegistry::from_embedded().unwrap();
        assert!(registry.get(ROOT_WORKFLOW).is_some());
        assert!(registry.get("calypso-planning").is_some());
        assert!(registry.get("calypso-default-feature-workflow").is_some());
        assert!(registry.get("calypso-implementation-loop").is_some());
        assert!(registry.get("calypso-save-state").is_some());
    }

    #[test]
    fn initial_state_is_scan_work_queue() {
        let interp = WorkflowInterpreter::new().unwrap();
        let exec = interp.initial_state().unwrap();
        assert_eq!(exec.position.workflow, ROOT_WORKFLOW);
        assert_eq!(exec.position.state, "scan-work-queue");
        assert!(exec.call_stack.is_empty());
    }

    #[test]
    fn advance_to_non_workflow_state() {
        let interp = WorkflowInterpreter::new().unwrap();
        let mut exec = interp.initial_state().unwrap();

        // scan-work-queue → idle (on no-pending-tasks)
        let outcome = interp.advance(&mut exec, "no-pending-tasks");
        assert!(matches!(outcome, StepOutcome::Advanced(_)));
        assert_eq!(exec.position.state, "idle");
    }

    #[test]
    fn advance_enters_sub_workflow() {
        let interp = WorkflowInterpreter::new().unwrap();
        let mut exec = interp.initial_state().unwrap();

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
        assert_eq!(exec.call_stack[0].workflow, ROOT_WORKFLOW);
        assert_eq!(exec.call_stack[0].state, "dispatch-planning");
    }

    #[test]
    fn advance_enters_development_sub_workflow() {
        let interp = WorkflowInterpreter::new().unwrap();
        let mut exec = interp.initial_state().unwrap();

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
        let mut exec = interp.initial_state().unwrap();

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
        let mut exec = interp.initial_state().unwrap();
        let outcome = interp.advance(&mut exec, "nonexistent-event");
        assert!(matches!(outcome, StepOutcome::Error(_)));
    }

    #[test]
    fn sub_workflow_terminal_pops_call_stack() {
        let interp = WorkflowInterpreter::new().unwrap();
        let mut exec = interp.initial_state().unwrap();

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
                assert_eq!(parent.workflow, ROOT_WORKFLOW);
                assert_eq!(parent.state, "scan-work-queue");
            }
            other => panic!("expected ReturnedToParent, got {other:?}"),
        }
        assert!(exec.call_stack.is_empty());
    }

    #[test]
    fn current_kind_returns_state_kind() {
        let interp = WorkflowInterpreter::new().unwrap();
        let exec = interp.initial_state().unwrap();
        assert_eq!(interp.current_kind(&exec), Some(StateKind::Agent));
    }

    #[test]
    fn execution_state_serializes_roundtrip() {
        let state = WorkflowExecutionState {
            position: WorkflowPosition {
                workflow: ROOT_WORKFLOW.to_string(),
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
        let mut exec = interp.initial_state().unwrap();

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
