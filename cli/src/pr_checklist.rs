use crate::state::{GateGroup, GateStatus};
use crate::template::TemplateSet;

const GATES_START: &str = "<!-- CALYPSO:GATES:START -->";
const GATES_END: &str = "<!-- CALYPSO:GATES:END -->";

/// Build the full PR description body with a seeded gate checklist.
/// All gates are rendered as unchecked (`- [ ]`) regardless of current status.
pub fn seed_pr_body(
    feature_id: &str,
    feature_type: &str,
    gate_groups: &[GateGroup],
    template: &TemplateSet,
) -> String {
    let gates_section = render_gates_section(gate_groups, template);

    format!(
        "## Summary\n<!-- Describe what this {feature_type} `{feature_id}` does -->\n\n## Gates\n{GATES_START}\n{gates_section}{GATES_END}\n\n## Risks\n<!-- List risks -->\n\n## Deployment notes\n<!-- Deployment steps -->\n\n## Rollback notes\n<!-- Rollback steps -->"
    )
}

/// Update an existing PR body, replacing only the gate checklist section between
/// `CALYPSO:GATES:START` and `CALYPSO:GATES:END` markers. Content outside those
/// markers is preserved unchanged. If the markers are absent, the body is returned
/// unchanged.
pub fn update_pr_body(
    existing_body: &str,
    gate_groups: &[GateGroup],
    template: &TemplateSet,
) -> String {
    let Some(start_pos) = existing_body.find(GATES_START) else {
        return existing_body.to_string();
    };
    let Some(end_pos) = existing_body.find(GATES_END) else {
        return existing_body.to_string();
    };

    let before = &existing_body[..start_pos + GATES_START.len()];
    let after = &existing_body[end_pos..];
    let gates_section = render_gates_section(gate_groups, template);

    format!("{before}\n{gates_section}{after}")
}

fn render_gates_section(gate_groups: &[GateGroup], template: &TemplateSet) -> String {
    let mut out = String::new();

    for group in gate_groups {
        out.push_str(&format!("### {}\n", group.label));
        for gate in &group.gates {
            let label = template
                .state_machine
                .gate_groups
                .iter()
                .flat_map(|g| g.gates.iter())
                .find(|t| t.id == gate.id)
                .and_then(|t| t.pr_checklist_label.as_deref())
                .unwrap_or(&gate.label);

            let marker = match gate.status {
                GateStatus::Passing => "x",
                GateStatus::Manual => "~",
                _ => " ",
            };
            out.push_str(&format!("- [{marker}] {label}\n"));
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{Gate, GateGroup, GateStatus};
    use crate::template::TemplateSet;

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

    fn minimal_template_with_checklist_label() -> TemplateSet {
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

    fn pending_gate_groups() -> Vec<GateGroup> {
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
        let groups = pending_gate_groups();
        let body = seed_pr_body("my-feature", "feat", &groups, &template);

        assert!(body.contains("- [ ] Tests pass"));
        assert!(body.contains("- [ ] Code review"));
        assert!(!body.contains("- [x]"));
        assert!(!body.contains("- [~]"));
    }

    #[test]
    fn seed_pr_body_includes_required_sections() {
        let template = minimal_template();
        let groups = pending_gate_groups();
        let body = seed_pr_body("my-feature", "feat", &groups, &template);

        assert!(body.contains("## Summary"));
        assert!(body.contains("## Gates"));
        assert!(body.contains(GATES_START));
        assert!(body.contains(GATES_END));
        assert!(body.contains("## Risks"));
        assert!(body.contains("## Deployment notes"));
        assert!(body.contains("## Rollback notes"));
    }

    #[test]
    fn update_pr_body_replaces_only_gates_section() {
        let template = minimal_template();
        let original = format!(
            "## Summary\nMy custom summary.\n\n## Gates\n{GATES_START}\n### Old\n- [ ] Old gate\n{GATES_END}\n\n## Risks\nSome risk."
        );

        let mut groups = pending_gate_groups();
        groups[0].gates[0].status = GateStatus::Passing;

        let updated = update_pr_body(&original, &groups, &template);

        assert!(updated.contains("My custom summary."));
        assert!(updated.contains("Some risk."));
        assert!(updated.contains("- [x] Tests pass"));
        assert!(!updated.contains("- [ ] Tests pass"));
        assert!(!updated.contains("Old gate"));
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
        let template = minimal_template_with_checklist_label();
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
        assert!(body.contains("- [ ] All tests green"));
        assert!(!body.contains("- [ ] Tests pass"));
    }

    #[test]
    fn update_pr_body_is_noop_when_gate_states_unchanged() {
        let template = minimal_template();
        let groups = pending_gate_groups();
        let body = seed_pr_body("f", "feat", &groups, &template);
        let updated = update_pr_body(&body, &groups, &template);
        // Both should have the same gate markers
        assert_eq!(
            body.contains("- [ ] Tests pass"),
            updated.contains("- [ ] Tests pass")
        );
        assert_eq!(
            body.contains("- [ ] Code review"),
            updated.contains("- [ ] Code review")
        );
    }

    #[test]
    fn update_pr_body_without_markers_returns_unchanged() {
        let template = minimal_template();
        let groups = pending_gate_groups();
        let body = "## Summary\nNo gates section here.\n\n## Risks\nNone.";
        let result = update_pr_body(body, &groups, &template);
        assert_eq!(result, body);
    }

    #[test]
    fn failing_gate_renders_as_unchecked() {
        let template = minimal_template();
        let groups = vec![GateGroup {
            id: "quality".to_string(),
            label: "Quality".to_string(),
            gates: vec![Gate {
                id: "tests".to_string(),
                label: "Tests pass".to_string(),
                task: "run-tests".to_string(),
                status: GateStatus::Failing,
            }],
        }];

        let body = seed_pr_body("f", "feat", &groups, &template);
        assert!(body.contains("- [ ] Tests pass"));
    }
}
