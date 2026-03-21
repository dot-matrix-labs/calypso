//! State machine driver — auto and step modes.
//!
//! Auto mode: drives the state machine forward, executing function steps
//! directly and launching supervised agent sessions for agent steps.
//! Pauses only on clarification requests or failures.
//!
//! Step mode: executes one step per human keypress (Enter to advance,
//! q to quit).

use std::path::Path;
use std::sync::Arc;

use crate::execution::{ExecutionConfig, ExecutionError, ExecutionOutcome, run_supervised_session};
use crate::state::{RepositoryState, WorkflowState};
use crate::template::{StateDefinition, StepType, TemplateSet};

// ── SessionExecutor trait ─────────────────────────────────────────────────────

/// Abstraction over the supervised-session execution layer.
///
/// The real implementation calls Claude via `run_supervised_session`.
/// Tests inject a `PhonyExecutor` that returns pre-canned outcomes without
/// spawning any external process.
pub trait SessionExecutor: Send + Sync {
    fn run(
        &self,
        state_path: &Path,
        role: &str,
        config: &ExecutionConfig,
    ) -> Result<ExecutionOutcome, ExecutionError>;
}

/// Production executor — delegates to `run_supervised_session`.
pub struct RealExecutor;

impl SessionExecutor for RealExecutor {
    fn run(
        &self,
        state_path: &Path,
        role: &str,
        config: &ExecutionConfig,
    ) -> Result<ExecutionOutcome, ExecutionError> {
        run_supervised_session(state_path, role, config)
    }
}

/// Whether the driver runs all steps automatically or waits for a keypress between steps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriverMode {
    Auto,
    Step,
}

/// The result of executing a single state machine step.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriverStepResult {
    /// Step executed successfully; advanced to this state.
    Advanced(WorkflowState),
    /// Step executed but no transition was available (terminal state).
    Terminal,
    /// Function step succeeded but state did not change (unexpected).
    Unchanged,
    /// Agent step requires clarification from the operator.
    ClarificationRequired(String),
    /// Step failed (agent NOK or function error).
    Failed { reason: String },
    /// Provider/runtime error.
    Error(String),
}

/// Drives the state machine forward, executing function and agent steps in sequence.
pub struct StateMachineDriver {
    pub mode: DriverMode,
    pub state_path: std::path::PathBuf,
    pub template: TemplateSet,
    pub config: ExecutionConfig,
    /// Pluggable session executor. `None` uses `RealExecutor` (default).
    pub executor: Option<Arc<dyn SessionExecutor>>,
}

impl StateMachineDriver {
    /// Execute the current state's step once.
    pub fn step(&self) -> DriverStepResult {
        let state = match RepositoryState::load_from_path(&self.state_path) {
            Ok(s) => s,
            Err(e) => return DriverStepResult::Error(e.to_string()),
        };

        if state.current_feature.workflow_state.is_terminal() {
            return DriverStepResult::Terminal;
        }

        let current = state.current_feature.workflow_state.as_str().to_string();
        let step_type = self.template.step_type_for_state(&current);

        match step_type {
            StepType::Function => self.execute_function_step(state, &current),
            StepType::Agent => self.execute_agent_step(&current),
        }
    }

    fn execute_function_step(
        &self,
        mut state: RepositoryState,
        state_name: &str,
    ) -> DriverStepResult {
        let fn_name = self
            .template
            .function_for_state(state_name)
            .unwrap_or_else(|| state_name.replace('-', "_"));

        match dispatch_function_step(&fn_name, &self.state_path) {
            Ok(Some(next)) => {
                state.current_feature.workflow_state = next.clone();
                match state.save_to_path(&self.state_path) {
                    Ok(()) => DriverStepResult::Advanced(next),
                    Err(e) => DriverStepResult::Error(e.to_string()),
                }
            }
            Ok(None) => match self.next_template_transition(state_name) {
                Some(next) => {
                    state.current_feature.workflow_state = next.clone();
                    match state.save_to_path(&self.state_path) {
                        Ok(()) => DriverStepResult::Advanced(next),
                        Err(e) => DriverStepResult::Error(e.to_string()),
                    }
                }
                None if state.current_feature.workflow_state.is_terminal() => {
                    DriverStepResult::Terminal
                }
                None => DriverStepResult::Unchanged,
            },
            Err(e) => DriverStepResult::Failed { reason: e },
        }
    }

    fn execute_agent_step(&self, state_name: &str) -> DriverStepResult {
        let role = self
            .template
            .state_machine
            .states
            .iter()
            .find(|s| s.name() == state_name)
            .and_then(|s| match s {
                StateDefinition::Detailed(c) => c.role.clone(),
                StateDefinition::Simple(_) => None,
            })
            .unwrap_or_else(|| state_name.to_string());

        // Look up the task for this role and inject its prompt as context.
        let task_context = self
            .template
            .agents
            .tasks
            .iter()
            .find(|t| t.role.as_deref() == Some(role.as_str()))
            .and_then(|t| self.template.prompts.prompts.get(&t.name))
            .cloned();

        // Resolve template-defined next states for the current state.
        let allowed_next_states: Option<Vec<WorkflowState>> = {
            let nexts: Vec<WorkflowState> = self
                .template
                .state_machine
                .transitions
                .iter()
                .filter(|t| t.from == state_name)
                .filter_map(|t| WorkflowState::from_template_state_name(&t.to).ok())
                .collect();
            if nexts.is_empty() { None } else { Some(nexts) }
        };

        let config = {
            let mut c = self.config.clone();
            if task_context.is_some() {
                c.task_context = task_context;
            }
            if allowed_next_states.is_some() {
                c.allowed_next_states = allowed_next_states;
            }
            c
        };

        let result = if let Some(exec) = &self.executor {
            exec.run(&self.state_path, &role, &config)
        } else {
            run_supervised_session(&self.state_path, &role, &config)
        };

        match result {
            Ok(outcome) => match outcome {
                ExecutionOutcome::Ok { advanced_to, .. } => advanced_to
                    .map(DriverStepResult::Advanced)
                    .unwrap_or(DriverStepResult::Unchanged),
                ExecutionOutcome::Nok { reason, .. } => DriverStepResult::Failed { reason },
                ExecutionOutcome::Aborted { reason } => DriverStepResult::Failed { reason },
                ExecutionOutcome::ClarificationRequired(req) => {
                    DriverStepResult::ClarificationRequired(req.question)
                }
                ExecutionOutcome::ProviderFailure { detail } => DriverStepResult::Error(detail),
            },
            Err(e) => DriverStepResult::Error(e.to_string()),
        }
    }

    /// Run in auto mode: loop until terminal, error, or clarification required.
    ///
    /// Returns a summary of all step results.
    pub fn run_auto(&self) -> Vec<DriverStepResult> {
        let mut results = Vec::new();
        loop {
            let result = self.step();
            let done = matches!(
                result,
                DriverStepResult::Terminal
                    | DriverStepResult::Failed { .. }
                    | DriverStepResult::Error(_)
                    | DriverStepResult::ClarificationRequired(_)
            );
            results.push(result);
            if done {
                break;
            }
        }
        results
    }
}

/// Dispatch a named function step. Returns `Ok(Some(next_state))` on advancement,
/// `Ok(None)` when the step succeeds without a deterministic transition, or
/// `Err(reason)` on failure.
fn dispatch_function_step(
    fn_name: &str,
    state_path: &Path,
) -> Result<Option<WorkflowState>, String> {
    match fn_name {
        "git_init" => {
            let repo_path = state_path
                .parent()
                .and_then(|p| p.parent())
                .ok_or("cannot determine repo path from state path")?;
            std::process::Command::new("git")
                .args(["init", &repo_path.to_string_lossy()])
                .output()
                .map_err(|e| e.to_string())?;
            Ok(None)
        }
        "verify_setup" => {
            use crate::doctor::{DoctorStatus, HostDoctorEnvironment, collect_doctor_report};
            let repo_path = state_path
                .parent()
                .and_then(|p| p.parent())
                .ok_or("cannot determine repo path")?;
            let report = collect_doctor_report(&HostDoctorEnvironment, repo_path);
            let all_pass = report
                .checks
                .iter()
                .all(|c| c.status == DoctorStatus::Passing);
            if all_pass {
                Ok(None)
            } else {
                let failures: Vec<String> = report
                    .checks
                    .iter()
                    .filter(|c| c.status != DoctorStatus::Passing)
                    .map(|c| c.id.label().to_string())
                    .collect();
                Err(format!("doctor checks failed: {}", failures.join(", ")))
            }
        }
        _ => {
            // Unknown function — treat as no-op for now
            Ok(None)
        }
    }

    fn next_template_transition(&self, state_name: &str) -> Option<WorkflowState> {
        self.template
            .state_machine
            .transitions
            .iter()
            .find(|transition| transition.from == state_name)
            .and_then(|transition| WorkflowState::from_template_state_name(&transition.to).ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        FeatureState, FeatureType, PullRequestRef, RepositoryState, SchedulingMeta,
    };
    use crate::template::{TemplateSet, load_embedded_template_set};
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_dir(label: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("calypso-driver-{label}-{nanos}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");
        dir
    }

    fn minimal_state(workflow_state: WorkflowState) -> RepositoryState {
        RepositoryState {
            version: 1,
            repo_id: "test-repo".to_string(),
            schema_version: 2,
            current_feature: FeatureState {
                feature_id: "test-feature".to_string(),
                branch: "feat/test".to_string(),
                worktree_path: "/tmp".to_string(),
                pull_request: PullRequestRef {
                    number: 1,
                    url: "https://github.com/example/repo/pull/1".to_string(),
                },
                github_snapshot: None,
                github_error: None,
                workflow_state,
                gate_groups: vec![],
                active_sessions: vec![],
                feature_type: FeatureType::Feat,
                roles: vec![],
                scheduling: SchedulingMeta::default(),
                artifact_refs: vec![],
                transcript_refs: vec![],
                clarification_history: vec![],
            },
            identity: Default::default(),
            providers: vec![],
            releases: vec![],
            deployments: vec![],
        }
    }

    fn write_state(dir: &Path, state: &RepositoryState) -> PathBuf {
        let state_dir = dir.join(".calypso");
        std::fs::create_dir_all(&state_dir).expect("create .calypso dir");
        let path = state_dir.join("repository-state.json");
        state.save_to_path(&path).expect("save state");
        path
    }

    fn function_template() -> TemplateSet {
        TemplateSet::from_yaml_strings(
            r#"
initial_state: new
states:
  - name: new
    type: function
    function: noop
  - name: prd-review
    type: function
    function: noop
  - done
transitions:
  - from: new
    to: prd-review
  - from: new
    to: done
  - from: prd-review
    to: new
gate_groups:
  - id: runtime
    label: Runtime
    gates:
      - id: runtime-gate
        label: Runtime gate
        task: runtime-human
"#,
            r#"
tasks:
  - name: runtime-human
    kind: human
"#,
            "prompts: {}\n",
        )
        .expect("function template should parse")
    }

    #[test]
    fn step_type_defaults_to_agent_for_simple_state_name() {
        let template = load_embedded_template_set().expect("template should load");
        let step_type = template.step_type_for_state("new");
        assert_eq!(step_type, StepType::Agent);
    }

    #[test]
    fn step_type_unknown_state_defaults_to_agent() {
        let template = load_embedded_template_set().expect("template should load");
        let step_type = template.step_type_for_state("nonexistent-state");
        assert_eq!(step_type, StepType::Agent);
    }

    #[test]
    fn function_for_state_returns_none_for_simple_state() {
        let template = load_embedded_template_set().expect("template should load");
        assert!(template.function_for_state("new").is_none());
    }

    #[test]
    fn driver_mode_variants_are_distinct() {
        assert_ne!(DriverMode::Auto, DriverMode::Step);
    }

    #[test]
    fn terminal_state_returns_terminal_without_running_step() {
        let dir = temp_dir("terminal");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::Done));
        let driver = StateMachineDriver {
            mode: DriverMode::Auto,
            state_path,
            template: function_template(),
            config: ExecutionConfig::default(),
            executor: None,
        };

        assert_eq!(driver.step(), DriverStepResult::Terminal);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn function_step_chooses_first_yaml_transition_and_persists_it() {
        let dir = temp_dir("function-advance");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::New));
        let driver = StateMachineDriver {
            mode: DriverMode::Auto,
            state_path: state_path.clone(),
            template: function_template(),
            config: ExecutionConfig::default(),
            executor: None,
        };

        assert_eq!(
            driver.step(),
            DriverStepResult::Advanced(WorkflowState::PrdReview)
        );

        let persisted = RepositoryState::load_from_path(&state_path).expect("load state");
        assert_eq!(
            persisted.current_feature.workflow_state,
            WorkflowState::PrdReview
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn function_step_can_follow_explicit_loop_back_transition() {
        let dir = temp_dir("function-loop");
        let state_path = write_state(&dir, &minimal_state(WorkflowState::PrdReview));
        let driver = StateMachineDriver {
            mode: DriverMode::Auto,
            state_path: state_path.clone(),
            template: function_template(),
            config: ExecutionConfig::default(),
            executor: None,
        };

        assert_eq!(
            driver.step(),
            DriverStepResult::Advanced(WorkflowState::New)
        );

        let persisted = RepositoryState::load_from_path(&state_path).expect("load state");
        assert_eq!(persisted.current_feature.workflow_state, WorkflowState::New);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
