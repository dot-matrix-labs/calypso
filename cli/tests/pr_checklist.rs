use calypso_cli::pr_checklist::{seed_pr_body, update_pr_body};
use calypso_cli::state::{Gate, GateGroup, GateStatus};
use calypso_cli::template::TemplateSet;

fn minimal_template() -> TemplateSet {
    let sm = r#"
initial_state: implementation
states: [implementation]
gate_groups:
  - id: quality
    label: Quality
    gates:
      - id: tests
        label: Tests pass
        task: run-tests
      - id: review
        label: Code review
        task: human-review
"#;
    let agents = r#"
tasks:
  - name: run-tests
    kind: builtin
    role: null
    builtin: builtin.github.pr_checks_green
  - name: human-review
    kind: human
    role: null
    builtin: null
"#;
    let prompts = "prompts: {}";
    TemplateSet::from_yaml_strings(sm, agents, prompts).expect("template should parse")
}

fn template_with_checklist_label() -> TemplateSet {
    let sm = r#"
initial_state: implementation
states: [implementation]
gate_groups:
  - id: quality
    label: Quality
    gates:
      - id: tests
        label: Tests pass
        task: run-tests
        pr_checklist_label: "All tests green"
"#;
    let agents = r#"
tasks:
  - name: run-tests
    kind: builtin
    role: null
    builtin: builtin.github.pr_checks_green
"#;
    let prompts = "prompts: {}";
    TemplateSet::from_yaml_strings(sm, agents, prompts).expect("template should parse")
}

fn pending_groups() -> Vec<GateGroup> {
    vec![GateGroup {
        id: "quality".to_string(),
        label: "Quality".to_string(),
        gates: vec![
            Gate {
                id: "tests".to_string(),
                label: "Tests pass".to_string(),
                task: "run-tests".to_string(),
                status: GateStatus::Pending,
            },
            Gate {
                id: "review".to_string(),
                label: "Code review".to_string(),
                task: "human-review".to_string(),
                status: GateStatus::Pending,
            },
        ],
    }]
}

#[test]
fn seed_pr_body_produces_all_unchecked_gates() {
    let template = minimal_template();
    let groups = pending_groups();
    let body = seed_pr_body("my-feature", "feat", &groups, &template);

    assert!(
        body.contains("- [ ] Tests pass"),
        "tests gate should be unchecked"
    );
    assert!(
        body.contains("- [ ] Code review"),
        "review gate should be unchecked"
    );
    assert!(!body.contains("- [x]"), "no gate should be checked");
    assert!(!body.contains("- [~]"), "no gate should be manual");
}

#[test]
fn update_pr_body_replaces_only_gates_section_preserving_other_content() {
    let template = minimal_template();
    let original = "## Summary\nMy custom summary.\n\n## Gates\n<!-- CALYPSO:GATES:START -->\n### Old\n- [ ] Old gate\n<!-- CALYPSO:GATES:END -->\n\n## Risks\nSome risk.";

    let mut groups = pending_groups();
    groups[0].gates[0].status = GateStatus::Passing;

    let updated = update_pr_body(original, &groups, &template);

    assert!(
        updated.contains("My custom summary."),
        "summary text should be preserved"
    );
    assert!(
        updated.contains("Some risk."),
        "risks text should be preserved"
    );
    assert!(
        updated.contains("- [x] Tests pass"),
        "passing gate should be checked"
    );
    assert!(
        !updated.contains("Old gate"),
        "old gate content should be replaced"
    );
}

#[test]
fn passing_gate_renders_as_checked() {
    let template = minimal_template();
    let groups = vec![GateGroup {
        id: "quality".to_string(),
        label: "Quality".to_string(),
        gates: vec![Gate {
            id: "tests".to_string(),
            label: "Tests pass".to_string(),
            task: "run-tests".to_string(),
            status: GateStatus::Passing,
        }],
    }];

    let body = seed_pr_body("f", "feat", &groups, &template);
    assert!(body.contains("- [x] Tests pass"));
}

#[test]
fn manual_gate_renders_with_tilde() {
    let template = minimal_template();
    let groups = vec![GateGroup {
        id: "quality".to_string(),
        label: "Quality".to_string(),
        gates: vec![Gate {
            id: "review".to_string(),
            label: "Code review".to_string(),
            task: "human-review".to_string(),
            status: GateStatus::Manual,
        }],
    }];

    let body = seed_pr_body("f", "feat", &groups, &template);
    assert!(body.contains("- [~] Code review"));
}

#[test]
fn gate_with_pr_checklist_label_uses_that_label() {
    let template = template_with_checklist_label();
    let groups = vec![GateGroup {
        id: "quality".to_string(),
        label: "Quality".to_string(),
        gates: vec![Gate {
            id: "tests".to_string(),
            label: "Tests pass".to_string(),
            task: "run-tests".to_string(),
            status: GateStatus::Pending,
        }],
    }];

    let body = seed_pr_body("f", "feat", &groups, &template);
    assert!(
        body.contains("- [ ] All tests green"),
        "pr_checklist_label should override gate label"
    );
    assert!(
        !body.contains("- [ ] Tests pass"),
        "original gate label should not appear"
    );
}

#[test]
fn update_pr_body_is_noop_when_gate_states_unchanged() {
    let template = minimal_template();
    let groups = pending_groups();
    let body = seed_pr_body("f", "feat", &groups, &template);
    let updated = update_pr_body(&body, &groups, &template);

    assert!(updated.contains("- [ ] Tests pass"));
    assert!(updated.contains("- [ ] Code review"));
}

#[test]
fn update_pr_body_without_markers_returns_body_unchanged() {
    let template = minimal_template();
    let groups = pending_groups();
    let body = "## Summary\nNo gates section here.\n\n## Risks\nNone.";
    let result = update_pr_body(body, &groups, &template);
    assert_eq!(result, body);
}
