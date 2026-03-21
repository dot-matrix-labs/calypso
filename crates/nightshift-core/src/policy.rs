use std::fs;
use std::path::Path;
use std::process::Command;
use std::time::SystemTime;

use crate::state::BuiltinEvidence;
use crate::template::TemplateSet;

pub trait PolicyEnvironment {
    fn file_exists(&self, path: &Path) -> bool;
    fn file_modified(&self, path: &Path) -> Option<SystemTime>;
    fn is_main_compatible(&self, repo_root: &Path) -> bool;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct HostPolicyEnvironment;

impl PolicyEnvironment for HostPolicyEnvironment {
    fn file_exists(&self, path: &Path) -> bool {
        path.is_file()
    }

    fn file_modified(&self, path: &Path) -> Option<SystemTime> {
        fs::metadata(path).ok()?.modified().ok()
    }

    fn is_main_compatible(&self, repo_root: &Path) -> bool {
        branch_contains_main(repo_root)
    }
}

pub fn collect_policy_evidence(
    environment: &impl PolicyEnvironment,
    repo_root: &Path,
    template: &TemplateSet,
) -> BuiltinEvidence {
    template
        .state_machine
        .policy_gates
        .iter()
        .fold(BuiltinEvidence::new(), |evidence, gate| {
            evidence.with_result(
                gate.evaluator.as_str(),
                evaluate_policy_gate(environment, repo_root, gate),
            )
        })
}

fn evaluate_policy_gate(
    environment: &impl PolicyEnvironment,
    repo_root: &Path,
    gate: &crate::template::PolicyGateTemplate,
) -> bool {
    match gate.evaluator.as_str() {
        "builtin.policy.implementation_plan_present"
        | "builtin.policy.next_prompt_present"
        | "builtin.policy.required_workflows_present" => gate
            .paths
            .iter()
            .all(|path| environment.file_exists(&repo_root.join(path))),
        "builtin.policy.implementation_plan_fresh" => {
            let primary = repo_root.join(&gate.paths[0]);
            let Some(primary_modified) = environment.file_modified(&primary) else {
                return false;
            };

            gate.watched_paths
                .iter()
                .filter_map(|path| environment.file_modified(&repo_root.join(path)))
                .all(|watched_modified| primary_modified >= watched_modified)
        }
        "builtin.git.is_main_compatible" => environment.is_main_compatible(repo_root),
        _ => false,
    }
}

fn branch_contains_main(repo_root: &Path) -> bool {
    [
        ["merge-base", "--is-ancestor", "main", "HEAD"].as_slice(),
        ["merge-base", "--is-ancestor", "origin/main", "HEAD"].as_slice(),
    ]
    .into_iter()
    .any(|args| {
        Command::new("git")
            .args(args)
            .current_dir(repo_root)
            .output()
            .is_ok_and(|output| output.status.success())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::template::TemplateSet;
    use std::collections::{BTreeMap, BTreeSet};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, UNIX_EPOCH};

    #[derive(Default)]
    struct FakePolicyEnvironment {
        files: BTreeSet<PathBuf>,
        modified_times: BTreeMap<PathBuf, SystemTime>,
        main_compatible_roots: BTreeSet<PathBuf>,
    }

    impl FakePolicyEnvironment {
        fn with_file(mut self, path: &Path, modified: SystemTime) -> Self {
            self.files.insert(path.to_path_buf());
            self.modified_times.insert(path.to_path_buf(), modified);
            self
        }

        fn with_main_compatible_root(mut self, path: &Path) -> Self {
            self.main_compatible_roots.insert(path.to_path_buf());
            self
        }
    }

    impl PolicyEnvironment for FakePolicyEnvironment {
        fn file_exists(&self, path: &Path) -> bool {
            self.files.contains(path)
        }

        fn file_modified(&self, path: &Path) -> Option<SystemTime> {
            self.modified_times.get(path).copied()
        }

        fn is_main_compatible(&self, repo_root: &Path) -> bool {
            self.main_compatible_roots.contains(repo_root)
        }
    }

    fn sample_template() -> TemplateSet {
        TemplateSet::from_yaml_strings(
            r#"
initial_state: new
states:
  - new
gate_groups:
  - id: policy
    label: Policy
    gates:
      - id: implementation-plan-present
        label: Implementation plan present
        task: implementation-plan-present
      - id: implementation-plan-fresh
        label: Implementation plan fresh
        task: implementation-plan-fresh
      - id: next-prompt-present
        label: Next prompt present
        task: next-prompt-present
      - id: required-workflows-present
        label: Required workflows present
        task: required-workflows-present
      - id: merge-drift-reviewed
        label: Merge drift reviewed
        task: main-compatibility
policy_gates:
  - gate_id: implementation-plan-present
    evaluator: builtin.policy.implementation_plan_present
    kind: hook
    paths:
      - docs/plans/implementation-plan.md
  - gate_id: implementation-plan-fresh
    evaluator: builtin.policy.implementation_plan_fresh
    kind: hook
    paths:
      - docs/plans/implementation-plan.md
    watched_paths:
      - docs/prd.md
  - gate_id: next-prompt-present
    evaluator: builtin.policy.next_prompt_present
    kind: hook
    paths:
      - docs/plans/next-prompt.md
  - gate_id: required-workflows-present
    evaluator: builtin.policy.required_workflows_present
    kind: workflow
    paths:
      - .github/workflows/rust-quality.yml
      - .github/workflows/rust-unit.yml
  - gate_id: merge-drift-reviewed
    evaluator: builtin.git.is_main_compatible
    kind: hook
    skip_on_tag_push: true
"#,
            r#"
tasks:
  - name: implementation-plan-present
    kind: builtin
    builtin: builtin.policy.implementation_plan_present
  - name: implementation-plan-fresh
    kind: builtin
    builtin: builtin.policy.implementation_plan_fresh
  - name: next-prompt-present
    kind: builtin
    builtin: builtin.policy.next_prompt_present
  - name: required-workflows-present
    kind: builtin
    builtin: builtin.policy.required_workflows_present
  - name: main-compatibility
    kind: builtin
    builtin: builtin.git.is_main_compatible
"#,
            "prompts: {}\n",
        )
        .expect("template should validate")
    }

    #[test]
    fn policy_evidence_marks_planning_and_workflow_rules_from_filesystem_state() {
        let repo_root = Path::new("/tmp/calypso-policy");
        let environment = FakePolicyEnvironment::default()
            .with_file(
                &repo_root.join("docs/plans/implementation-plan.md"),
                UNIX_EPOCH + Duration::from_secs(20),
            )
            .with_file(
                &repo_root.join("docs/prd.md"),
                UNIX_EPOCH + Duration::from_secs(10),
            )
            .with_file(
                &repo_root.join("docs/plans/next-prompt.md"),
                UNIX_EPOCH + Duration::from_secs(15),
            )
            .with_file(
                &repo_root.join(".github/workflows/rust-quality.yml"),
                UNIX_EPOCH + Duration::from_secs(15),
            )
            .with_file(
                &repo_root.join(".github/workflows/rust-unit.yml"),
                UNIX_EPOCH + Duration::from_secs(15),
            )
            .with_main_compatible_root(repo_root);

        let evidence = collect_policy_evidence(&environment, repo_root, &sample_template());

        assert_eq!(
            evidence.result_for("builtin.policy.implementation_plan_present"),
            Some(true)
        );
        assert_eq!(
            evidence.result_for("builtin.policy.implementation_plan_fresh"),
            Some(true)
        );
        assert_eq!(
            evidence.result_for("builtin.policy.next_prompt_present"),
            Some(true)
        );
        assert_eq!(
            evidence.result_for("builtin.policy.required_workflows_present"),
            Some(true)
        );
        assert_eq!(
            evidence.result_for("builtin.git.is_main_compatible"),
            Some(true)
        );
    }

    #[test]
    fn implementation_plan_fresh_returns_false_when_primary_has_no_modification_time() {
        let repo_root = Path::new("/tmp/calypso-policy");
        // watched_paths file exists with a time, but the primary (implementation-plan.md)
        // has no recorded modification time — file_modified returns None for it
        let environment = FakePolicyEnvironment::default().with_file(
            &repo_root.join("docs/prd.md"),
            UNIX_EPOCH + Duration::from_secs(10),
        );

        let evidence = collect_policy_evidence(&environment, repo_root, &sample_template());

        assert_eq!(
            evidence.result_for("builtin.policy.implementation_plan_fresh"),
            Some(false)
        );
    }

    #[test]
    fn evaluate_policy_gate_returns_false_for_unknown_evaluator() {
        use crate::template::{PolicyGateKind, PolicyGateTemplate};

        let gate = PolicyGateTemplate {
            gate_id: "unknown-gate".to_string(),
            evaluator: "builtin.unknown.evaluator".to_string(),
            kind: PolicyGateKind::Hook,
            paths: vec![],
            watched_paths: vec![],
            skip_on_tag_push: false,
        };
        let environment = FakePolicyEnvironment::default();
        let result = evaluate_policy_gate(&environment, Path::new("/tmp"), &gate);
        assert!(!result);
    }

    #[test]
    fn policy_evidence_fails_when_plan_or_workflows_are_missing_or_stale() {
        let repo_root = Path::new("/tmp/calypso-policy");
        let environment = FakePolicyEnvironment::default()
            .with_file(
                &repo_root.join("docs/plans/implementation-plan.md"),
                UNIX_EPOCH + Duration::from_secs(10),
            )
            .with_file(
                &repo_root.join("docs/prd.md"),
                UNIX_EPOCH + Duration::from_secs(20),
            );

        let evidence = collect_policy_evidence(&environment, repo_root, &sample_template());

        assert_eq!(
            evidence.result_for("builtin.policy.implementation_plan_present"),
            Some(true)
        );
        assert_eq!(
            evidence.result_for("builtin.policy.implementation_plan_fresh"),
            Some(false)
        );
        assert_eq!(
            evidence.result_for("builtin.policy.next_prompt_present"),
            Some(false)
        );
        assert_eq!(
            evidence.result_for("builtin.policy.required_workflows_present"),
            Some(false)
        );
        assert_eq!(
            evidence.result_for("builtin.git.is_main_compatible"),
            Some(false)
        );
    }
}
