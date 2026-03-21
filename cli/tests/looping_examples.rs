use calypso_cli::template::TemplateSet;
use std::fs;
use std::path::{Path, PathBuf};

struct ExampleCase {
    name: &'static str,
    initial_state: &'static str,
    restart_target: &'static str,
}

fn example_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("examples/looping-state-machines")
}

fn load_example(case: &ExampleCase) -> TemplateSet {
    let dir = example_root().join(case.name);
    let state_machine =
        fs::read_to_string(dir.join("state-machine.yml")).expect("state-machine example should load");
    let agents = fs::read_to_string(dir.join("agents.yml")).expect("agents example should load");
    let prompts = fs::read_to_string(dir.join("prompts.yml")).expect("prompts example should load");

    TemplateSet::from_yaml_strings(&state_machine, &agents, &prompts)
        .expect("example template should validate")
}

fn has_transition(template: &TemplateSet, from: &str, to: &str) -> bool {
    template
        .state_machine
        .transitions
        .iter()
        .any(|transition| transition.from == from && transition.to == to)
}

fn example_cases() -> [ExampleCase; 3] {
    [
        ExampleCase {
            name: "project-task-iteration",
            initial_state: "task-intake",
            restart_target: "task-intake",
        },
        ExampleCase {
            name: "review-queue",
            initial_state: "review-queue",
            restart_target: "review-queue",
        },
        ExampleCase {
            name: "implementation-loop",
            initial_state: "inspect-backlog",
            restart_target: "inspect-backlog",
        },
    ]
}

#[test]
fn looping_examples_validate_under_the_current_template_schema() {
    for case in example_cases() {
        let template = load_example(&case);

        assert_eq!(template.state_machine.initial_state, case.initial_state);
        assert!(
            !template.state_machine.gate_groups.is_empty(),
            "example {} should define at least one gate group",
            case.name
        );
    }
}

#[test]
fn looping_examples_restart_back_to_the_top_of_their_loop() {
    for case in example_cases() {
        let template = load_example(&case);

        assert!(
            has_transition(&template, "restart", case.restart_target),
            "example {} should loop from restart back to {}",
            case.name,
            case.restart_target
        );
    }
}

#[test]
fn documented_project_task_iteration_example_is_present() {
    let readme = example_root().join("README.md");
    let contents = fs::read_to_string(&readme).expect("example README should load");

    assert!(
        contents.contains("project-task-iteration/"),
        "{} should document the project-task-iteration example",
        display_path(&readme)
    );
    assert!(
        contents.contains("from: restart") && contents.contains("to: task-intake"),
        "{} should show the explicit loop edge",
        display_path(&readme)
    );
}

fn display_path(path: &Path) -> String {
    path.display().to_string()
}
