use calypso_templates::{StepType, load_embedded_template_set};
use nightshift_core::driver::{DriverMode, DriverStepResult, StateMachineDriver};
use nightshift_core::execution::ExecutionConfig;

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
fn step_type_is_agent_for_all_default_states() {
    let template = load_embedded_template_set().expect("template should load");
    for state in &template.state_machine.states {
        assert_eq!(
            template.step_type_for_state(state.name()),
            StepType::Agent,
            "default state '{}' should be Agent type",
            state.name()
        );
    }
}

#[test]
fn function_for_state_returns_none_for_simple_state() {
    let template = load_embedded_template_set().expect("template should load");
    assert!(template.function_for_state("new").is_none());
    assert!(template.function_for_state("implementation").is_none());
}

#[test]
fn function_for_state_returns_none_for_unknown_state() {
    let template = load_embedded_template_set().expect("template should load");
    assert!(template.function_for_state("unknown-state").is_none());
}

#[test]
fn driver_mode_auto_and_step_are_distinct() {
    assert_ne!(DriverMode::Auto, DriverMode::Step);
    assert_eq!(DriverMode::Auto, DriverMode::Auto);
    assert_eq!(DriverMode::Step, DriverMode::Step);
}

#[test]
fn driver_step_result_advanced_holds_workflow_state() {
    use nightshift_core::state::WorkflowState;
    let result = DriverStepResult::Advanced(WorkflowState::PrdReview);
    assert!(matches!(
        result,
        DriverStepResult::Advanced(WorkflowState::PrdReview)
    ));
}

#[test]
fn driver_step_result_failed_holds_reason() {
    let result = DriverStepResult::Failed {
        reason: "test failure".to_string(),
    };
    match result {
        DriverStepResult::Failed { reason } => assert_eq!(reason, "test failure"),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn driver_step_result_error_holds_message() {
    let result = DriverStepResult::Error("oops".to_string());
    match result {
        DriverStepResult::Error(msg) => assert_eq!(msg, "oops"),
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn driver_step_result_clarification_holds_question() {
    let result = DriverStepResult::ClarificationRequired("what should I do?".to_string());
    match result {
        DriverStepResult::ClarificationRequired(q) => {
            assert_eq!(q, "what should I do?");
        }
        other => panic!("unexpected result: {other:?}"),
    }
}

#[test]
fn driver_step_errors_when_state_file_missing() {
    let template = load_embedded_template_set().expect("template should load");
    let driver = StateMachineDriver {
        mode: DriverMode::Auto,
        state_path: std::path::PathBuf::from("/nonexistent/path/state.json"),
        template,
        config: ExecutionConfig::default(),
        executor: None,
    };

    let result = driver.step();
    assert!(
        matches!(result, DriverStepResult::Error(_)),
        "missing state file should produce Error result"
    );
}

#[test]
fn run_auto_stops_on_first_error() {
    let template = load_embedded_template_set().expect("template should load");
    let driver = StateMachineDriver {
        mode: DriverMode::Auto,
        state_path: std::path::PathBuf::from("/nonexistent/path/state.json"),
        template,
        config: ExecutionConfig::default(),
        executor: None,
    };

    let results = driver.run_auto();
    assert_eq!(results.len(), 1, "auto run should stop after first error");
    assert!(matches!(results[0], DriverStepResult::Error(_)));
}
