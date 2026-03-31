//! Non-interactive run inspection and control commands.
//!
//! These commands read and mutate the daemon-owned `WorkflowRun` state file
//! at `<repo_root>/.calypso/workflow-run.json`.  They are designed for
//! operator use in daemon-first, non-interactive environments.
//!
//! # Control commands
//!
//! | Command              | Description                                       |
//! |----------------------|---------------------------------------------------|
//! | `run list`           | List workflow runs (currently at most one active)  |
//! | `run inspect [id]`   | Show details for the active or specified run       |
//! | `run retry`          | Retry the current step                             |
//! | `run abort`          | Abort the active run                               |
//! | `run clarify <msg>`  | Provide clarification to a stuck agent step        |
//! | `run force-transition <state> --reason <reason>` | Force-transition  |
//!
//! All control commands record operator intent through the `WorkflowRun`
//! steering model rather than ad hoc trigger files.

use std::path::Path;

use calypso_runtime::workflow_run::{SteeringAction, SteeringOutcome, TerminalReason, WorkflowRun};

/// Load the active workflow run or exit with an error message.
fn load_run_or_exit(cwd: &Path) -> WorkflowRun {
    let path = WorkflowRun::default_path(cwd);
    match WorkflowRun::load(&path) {
        Ok(Some(run)) => run,
        Ok(None) => {
            eprintln!("No active workflow run found.");
            eprintln!("Start a daemon with `calypso daemon start` to create a workflow run.");
            std::process::exit(1);
        }
        Err(e) => {
            eprintln!("Failed to load workflow run: {e}");
            std::process::exit(1);
        }
    }
}

/// Save the workflow run or exit with an error message.
fn save_run_or_exit(cwd: &Path, run: &WorkflowRun) {
    let path = WorkflowRun::default_path(cwd);
    if let Err(e) = run.save(&path) {
        eprintln!("Failed to save workflow run: {e}");
        std::process::exit(1);
    }
}

/// `calypso run list` — list workflow runs.
///
/// Currently there is at most one active run per repository.
pub fn run_list(cwd: &Path) {
    let path = WorkflowRun::default_path(cwd);
    match WorkflowRun::load(&path) {
        Ok(Some(run)) => {
            let status = if run.is_stopped() {
                "stopped"
            } else {
                "active"
            };
            println!(
                "{} {} state={} iterations={} status={}",
                run.run_id, run.workflow_id, run.current_state, run.iteration, status,
            );
        }
        Ok(None) => {
            println!("No workflow runs.");
        }
        Err(e) => {
            eprintln!("Failed to load workflow run: {e}");
            std::process::exit(1);
        }
    }
}

/// `calypso run inspect [run-id]` — show details for the current or specified run.
pub fn run_inspect(cwd: &Path, _run_id: Option<&str>) {
    let run = load_run_or_exit(cwd);
    let inspection = run.inspect();

    println!("Run:             {}", inspection.run_id);
    println!("Workflow:        {}", inspection.workflow_id);
    println!("Current state:   {}", inspection.current_state);
    println!("Locality:        {}", inspection.locality);
    println!("Iteration:       {}", inspection.iteration);
    println!("Transitions:     {}", inspection.transition_count);
    println!("Pending checks:  {}", inspection.pending_check_count);
    println!("Failing checks:  {}", inspection.failing_check_count);
    println!("Active agents:   {}", inspection.active_agent_count);
    println!("Pending steering:{}", inspection.steering_pending_count);
    println!("Created:         {}", inspection.created_at);
    println!("Updated:         {}", inspection.updated_at);

    if let Some(ref reason) = inspection.terminal_reason {
        println!("Terminal reason:  {reason}");
    }

    // Print transition history.
    if !run.transition_history.is_empty() {
        println!("\nTransition history:");
        for t in &run.transition_history {
            println!(
                "  {} -> {} ({}) at {}",
                t.from_state, t.to_state, t.trigger, t.timestamp
            );
        }
    }

    // Print steering history.
    if !run.steering.is_empty() {
        println!("\nSteering history:");
        for s in &run.steering {
            let outcome = match &s.outcome {
                SteeringOutcome::Pending => "pending",
                SteeringOutcome::Applied => "applied",
                SteeringOutcome::Rejected { reason } => reason.as_str(),
            };
            println!("  {:?} -> {} at {}", s.action, outcome, s.requested_at);
        }
    }
}

/// `calypso run retry` — record a retry steering request on the active run.
pub fn run_retry(cwd: &Path) {
    let mut run = load_run_or_exit(cwd);

    if run.is_stopped() {
        eprintln!("Cannot retry: workflow run is already stopped.");
        std::process::exit(1);
    }

    run.add_steering(SteeringAction::Retry);
    save_run_or_exit(cwd, &run);
    println!(
        "Retry requested for run {} at state '{}'.",
        run.run_id, run.current_state
    );
}

/// `calypso run abort` — abort the active workflow run.
pub fn run_abort(cwd: &Path) {
    let mut run = load_run_or_exit(cwd);

    if run.is_stopped() {
        eprintln!("Workflow run is already stopped.");
        std::process::exit(1);
    }

    run.add_steering(SteeringAction::Abort);
    run.resolve_steering(SteeringOutcome::Applied);
    run.terminate(TerminalReason::Aborted {
        reason: "operator abort via CLI".to_string(),
    });
    save_run_or_exit(cwd, &run);
    println!("Workflow run {} aborted.", run.run_id);
}

/// `calypso run clarify <message>` — provide clarification to a stuck agent step.
pub fn run_clarify(cwd: &Path, message: &str) {
    let mut run = load_run_or_exit(cwd);

    if run.is_stopped() {
        eprintln!("Cannot clarify: workflow run is already stopped.");
        std::process::exit(1);
    }

    run.add_steering(SteeringAction::Clarify {
        message: message.to_string(),
    });
    save_run_or_exit(cwd, &run);
    println!(
        "Clarification recorded for run {} at state '{}'.",
        run.run_id, run.current_state
    );
}

/// `calypso run force-transition <state> --reason <reason>` — force-transition
/// the active run to a target state with recorded operator intent.
pub fn run_force_transition(cwd: &Path, target_state: &str, reason: &str) {
    let mut run = load_run_or_exit(cwd);

    if run.is_stopped() {
        eprintln!("Cannot force-transition: workflow run is already stopped.");
        std::process::exit(1);
    }

    run.add_steering(SteeringAction::ForceTransition {
        target_state: target_state.to_string(),
        reason: reason.to_string(),
    });
    run.resolve_steering(SteeringOutcome::Applied);
    run.record_transition(target_state, &format!("force-transition: {reason}"));
    save_run_or_exit(cwd, &run);
    println!(
        "Forced transition: run {} now at state '{target_state}'.",
        run.run_id
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use calypso_runtime::workflow_run::WorkflowRun;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_id() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos()
    }

    fn temp_project_with_run(state: &str) -> (std::path::PathBuf, WorkflowRun) {
        let dir = std::env::temp_dir().join(format!("calypso-run-ctl-{}", unique_id()));
        let calypso_dir = dir.join(".calypso");
        std::fs::create_dir_all(&calypso_dir).expect("create .calypso dir");

        let run = WorkflowRun::new("test-wf", state, 1);
        let path = WorkflowRun::default_path(&dir);
        run.save(&path).expect("save run");
        (dir, run)
    }

    #[test]
    fn run_list_shows_active_run() {
        let (dir, _) = temp_project_with_run("implement");
        // run_list prints to stdout; we just verify it does not panic.
        run_list(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_list_handles_no_run() {
        let dir = std::env::temp_dir().join(format!("calypso-run-ctl-empty-{}", unique_id()));
        let calypso_dir = dir.join(".calypso");
        std::fs::create_dir_all(&calypso_dir).expect("create .calypso dir");
        run_list(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_inspect_shows_run_details() {
        let (dir, _) = temp_project_with_run("scan");
        run_inspect(&dir, None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_retry_records_steering() {
        let (dir, _) = temp_project_with_run("implement");
        run_retry(&dir);
        let path = WorkflowRun::default_path(&dir);
        let reloaded = WorkflowRun::load(&path).unwrap().unwrap();
        assert_eq!(reloaded.steering.len(), 1);
        assert!(matches!(reloaded.steering[0].action, SteeringAction::Retry));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_abort_terminates_run() {
        let (dir, _) = temp_project_with_run("implement");
        run_abort(&dir);
        let path = WorkflowRun::default_path(&dir);
        let reloaded = WorkflowRun::load(&path).unwrap().unwrap();
        assert!(reloaded.is_stopped());
        assert!(matches!(
            reloaded.terminal_reason,
            Some(TerminalReason::Aborted { .. })
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_clarify_records_message() {
        let (dir, _) = temp_project_with_run("stuck");
        run_clarify(&dir, "Use the production database credentials.");
        let path = WorkflowRun::default_path(&dir);
        let reloaded = WorkflowRun::load(&path).unwrap().unwrap();
        assert_eq!(reloaded.steering.len(), 1);
        assert!(matches!(
            &reloaded.steering[0].action,
            SteeringAction::Clarify { message } if message == "Use the production database credentials."
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn run_force_transition_updates_state() {
        let (dir, _) = temp_project_with_run("implement");
        run_force_transition(&dir, "review", "CI is green, skip to review");
        let path = WorkflowRun::default_path(&dir);
        let reloaded = WorkflowRun::load(&path).unwrap().unwrap();
        assert_eq!(reloaded.current_state, "review");
        assert_eq!(reloaded.transition_history.len(), 1);
        assert_eq!(reloaded.steering.len(), 1);
        assert!(matches!(
            &reloaded.steering[0].action,
            SteeringAction::ForceTransition { target_state, .. } if target_state == "review"
        ));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
