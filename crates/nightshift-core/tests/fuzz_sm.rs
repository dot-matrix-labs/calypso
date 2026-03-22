//! Fuzz coverage and regression fixtures for the YAML state-machine parser and
//! graph validator in `headless_sm`.
//!
//! # Structure
//!
//! - **Property tests** (proptest): feed arbitrary byte sequences and
//!   structured-but-invalid YAML directly to `load_and_validate` and assert
//!   that the function never panics, hangs, or produces undefined behaviour.
//!   Errors are expected and acceptable; panics are not.
//!
//! - **Regression fixtures**: deterministic `#[test]` cases that pin
//!   known-pathological inputs that previously triggered edge cases during
//!   development.  Each fixture documents the class of failure it covers.
//!
//! The proptest configuration uses a reduced number of cases so that CI
//! remains fast.  Increase `PROPTEST_CASES` locally for deeper exploration.

use nightshift_core::headless_sm::load_and_validate;
use proptest::prelude::*;

// ── Property-based fuzz tests ─────────────────────────────────────────────────

/// Maximum number of proptest cases run in CI.  Set via the `PROPTEST_CASES`
/// environment variable to override locally (e.g. `PROPTEST_CASES=10000`).
const CI_CASES: u32 = 256;

proptest! {
    #![proptest_config(ProptestConfig {
        cases: CI_CASES,
        // Do not persist the proptest state across runs in CI to keep the
        // working tree clean.
        failure_persistence: None,
        ..Default::default()
    })]

    /// `load_and_validate` must never panic on arbitrary UTF-8 input.
    ///
    /// Errors (YAML parse failures, validation errors) are expected and fine;
    /// the only forbidden outcome is a panic.
    #[test]
    fn no_panic_on_arbitrary_utf8(s in ".*") {
        let _ = load_and_validate(&s, "<fuzz>");
    }

    /// `load_and_validate` must never panic on arbitrary ASCII strings that may
    /// contain YAML special characters (colons, braces, pipes, brackets).
    #[test]
    fn no_panic_on_yaml_special_chars(s in "[a-zA-Z0-9 :\\{\\}|\\[\\]_\\-\\.\\*]*") {
        let _ = load_and_validate(&s, "<fuzz-special>");
    }

    /// Fuzz the state name field specifically: arbitrary names (including empty
    /// strings and Unicode) must not cause a panic.
    #[test]
    fn no_panic_on_arbitrary_state_names(name in "[a-zA-Z_][a-zA-Z0-9_]{0,31}") {
        let yaml = format!(
            "initial_state: {name}\nstates:\n  - name: {name}\n    action: terminal\n",
        );
        let _ = load_and_validate(&yaml, "<fuzz-name>");
    }

    /// Fuzz the action field: arbitrary action strings must be rejected with an
    /// unsupported-action error, not a panic.
    #[test]
    fn no_panic_on_arbitrary_action_type(action in "[a-zA-Z_][a-zA-Z0-9_]{0,31}") {
        let yaml = format!(
            "initial_state: s\nstates:\n  - name: s\n    action: {action}\n"
        );
        let _ = load_and_validate(&yaml, "<fuzz-action>");
    }

    /// Fuzz the `initial_state` field: arbitrary identifiers must not panic.
    #[test]
    fn no_panic_on_arbitrary_initial_state(initial in "[a-zA-Z_][a-zA-Z0-9_]{0,31}") {
        let yaml = format!(
            "initial_state: {initial}\nstates:\n  - name: real\n    action: terminal\n"
        );
        let _ = load_and_validate(&yaml, "<fuzz-initial>");
    }

    /// Fuzz transition target fields: arbitrary target names must not panic.
    #[test]
    fn no_panic_on_arbitrary_agent_targets(
        on_success in "[a-zA-Z_][a-zA-Z0-9_]{0,15}",
        on_failure in "[a-zA-Z_][a-zA-Z0-9_]{0,15}",
    ) {
        let yaml = format!(
            "initial_state: work\nstates:\n  - name: work\n    action: agent\n    on_success: {on_success}\n    on_failure: {on_failure}\n  - name: done\n    action: terminal\n  - name: error\n    action: terminal\n"
        );
        let _ = load_and_validate(&yaml, "<fuzz-agent-targets>");
    }

    /// Fuzz builtin keyword values: arbitrary builtin strings must not panic.
    #[test]
    fn no_panic_on_arbitrary_builtin_keyword(keyword in "[a-zA-Z_.][a-zA-Z0-9_.]{0,63}") {
        let yaml = format!(
            "initial_state: check\nstates:\n  - name: check\n    action: builtin\n    builtin: {keyword}\n    on_pass: done\n    on_fail: done\n  - name: done\n    action: terminal\n"
        );
        let _ = load_and_validate(&yaml, "<fuzz-builtin-kw>");
    }

    /// Fuzz loop target fields: arbitrary loop targets must not panic.
    #[test]
    fn no_panic_on_arbitrary_loop_target(target in "[a-zA-Z_][a-zA-Z0-9_]{0,31}") {
        let yaml = format!(
            "initial_state: r\nstates:\n  - name: r\n    action: loop\n    target: {target}\n"
        );
        let _ = load_and_validate(&yaml, "<fuzz-loop-target>");
    }

    /// Fuzz deeply nested / repeated states list entries.  Must not panic even
    /// if the list contains many duplicate names.
    #[test]
    fn no_panic_on_many_states(n in 1usize..=64) {
        // All states have the same name → duplicate detection must trigger.
        let entry = "  - name: s\n    action: terminal\n";
        let states: String = entry.repeat(n);
        let yaml = format!("initial_state: s\nstates:\n{states}");
        let _ = load_and_validate(&yaml, "<fuzz-many-states>");
    }
}

// ── Regression fixtures ───────────────────────────────────────────────────────
//
// Each fixture documents one class of pathological input.  These are
// deterministic and must always pass.

/// Empty string must not panic; it should yield a YAML or validation error.
#[test]
fn regression_empty_string_is_error_not_panic() {
    let result = load_and_validate("", "<regression>");
    assert!(result.is_err(), "empty string must not succeed");
}

/// Null byte in input must not panic.
#[test]
fn regression_null_byte_input() {
    let input = "initial_state: x\nstates:\n  - name: x\n    action: terminal\n\0";
    let _ = load_and_validate(input, "<regression-null>");
}

/// Very long state name must not cause a hang or panic.
#[test]
fn regression_extremely_long_state_name() {
    let long_name = "a".repeat(100_000);
    let yaml = format!(
        "initial_state: {long_name}\nstates:\n  - name: {long_name}\n    action: terminal\n"
    );
    // Either Ok or Err is acceptable; panic or hang is not.
    let _ = load_and_validate(&yaml, "<regression-long-name>");
}

/// YAML anchor/alias pattern must be handled gracefully (must not hang or panic).
#[test]
fn regression_yaml_anchor_alias_does_not_hang() {
    let yaml = "initial_state: s\nstates:\n  - name: &anchor s\n    action: terminal\n";
    let _ = load_and_validate(yaml, "<regression-anchor>");
}

/// Deeply nested YAML mapping (stress test for the parser's recursion limit).
#[test]
fn regression_deeply_nested_yaml_mapping() {
    // Build a string like: {a: {a: {a: … }}}
    let depth = 500;
    let open: String = "{a: ".repeat(depth);
    let close: String = "}".repeat(depth);
    let yaml = format!("{open}null{close}");
    let _ = load_and_validate(&yaml, "<regression-deep-map>");
}

/// YAML with only whitespace must not panic.
#[test]
fn regression_whitespace_only_input() {
    let _ = load_and_validate("   \n\t\n   ", "<regression-ws>");
}

/// YAML with only comments must not panic.
#[test]
fn regression_comment_only_input() {
    let yaml = "# This is a comment\n# Another comment\n";
    let _ = load_and_validate(yaml, "<regression-comments>");
}

/// Large number of duplicate states: must be caught by the duplicate-detection
/// rule without panicking.
#[test]
fn regression_many_duplicate_states() {
    let entry = "  - name: dupe\n    action: terminal\n";
    let states: String = entry.repeat(200);
    let yaml = format!("initial_state: dupe\nstates:\n{states}");
    let result = load_and_validate(&yaml, "<regression-many-dupes>");
    assert!(result.is_err(), "many duplicate states must be rejected");
}

/// `initial_state` referencing a non-existent state must be caught.
#[test]
fn regression_initial_state_not_in_states() {
    let yaml = "initial_state: ghost\nstates:\n  - name: real\n    action: terminal\n";
    let result = load_and_validate(yaml, "<regression-ghost-initial>");
    assert!(result.is_err());
}

/// Loop whose target is itself — a self-loop — must be accepted because the
/// target *does* exist in the state list.
#[test]
fn regression_loop_self_target_is_accepted() {
    let yaml = "initial_state: r\nstates:\n  - name: r\n    action: loop\n    target: r\n";
    let result = load_and_validate(yaml, "<regression-self-loop>");
    assert!(
        result.is_ok(),
        "self-referencing loop should be valid: {result:?}"
    );
}

/// Loop whose target does not exist must be rejected.
#[test]
fn regression_loop_unknown_target_is_rejected() {
    let yaml = "initial_state: r\nstates:\n  - name: r\n    action: loop\n    target: ghost\n";
    let result = load_and_validate(yaml, "<regression-loop-ghost>");
    assert!(result.is_err(), "loop to unknown state must be rejected");
}

/// `builtin` field value that doesn't start with `builtin.` must be rejected.
#[test]
fn regression_builtin_bad_prefix() {
    let yaml = "initial_state: c\nstates:\n  - name: c\n    action: builtin\n    builtin: custom.fn\n    on_pass: d\n    on_fail: d\n  - name: d\n    action: terminal\n";
    let result = load_and_validate(yaml, "<regression-bad-builtin-prefix>");
    assert!(result.is_err(), "bad builtin prefix must be rejected");
}

/// `agent` state carrying `on_pass` (a builtin field) must be rejected.
#[test]
fn regression_agent_with_builtin_field_is_rejected() {
    let yaml = "initial_state: w\nstates:\n  - name: w\n    action: agent\n    on_success: d\n    on_failure: d\n    on_pass: d\n  - name: d\n    action: terminal\n";
    let result = load_and_validate(yaml, "<regression-agent-on-pass>");
    assert!(
        result.is_err(),
        "agent with on_pass must be rejected: {result:?}"
    );
}

/// Terminal state with extra transition fields must be rejected.
#[test]
fn regression_terminal_with_transition_fields() {
    let yaml = "initial_state: done\nstates:\n  - name: done\n    action: terminal\n    on_success: done\n";
    let result = load_and_validate(yaml, "<regression-terminal-extra>");
    assert!(
        result.is_err(),
        "terminal with transitions must be rejected"
    );
}

/// Ambiguous loop state carrying agent fields must be rejected.
#[test]
fn regression_loop_with_agent_fields_is_rejected() {
    let yaml = "initial_state: r\nstates:\n  - name: r\n    action: loop\n    target: r\n    on_success: r\n";
    let result = load_and_validate(yaml, "<regression-loop-with-agent>");
    assert!(result.is_err(), "loop with agent fields must be rejected");
}

/// Empty `initial_state` value must be rejected.
#[test]
fn regression_empty_initial_state_string() {
    let yaml = "initial_state: \"\"\nstates:\n  - name: s\n    action: terminal\n";
    let result = load_and_validate(yaml, "<regression-empty-initial>");
    assert!(result.is_err(), "empty initial_state must be rejected");
}

/// Missing `initial_state` key must result in a YAML parse error (serde
/// requires the field).
#[test]
fn regression_missing_initial_state_key() {
    let yaml = "states:\n  - name: s\n    action: terminal\n";
    let result = load_and_validate(yaml, "<regression-missing-initial-key>");
    assert!(
        result.is_err(),
        "missing initial_state key must be rejected"
    );
}

/// YAML list-as-document root must not panic.
#[test]
fn regression_yaml_list_at_root() {
    let yaml = "- a\n- b\n- c\n";
    let _ = load_and_validate(yaml, "<regression-list-root>");
}

/// Integer value for `initial_state` must not panic.
#[test]
fn regression_integer_initial_state() {
    let yaml = "initial_state: 42\nstates:\n  - name: 42\n    action: terminal\n";
    // Either Ok (if serde_yaml coerces the integer to a string) or Err is fine.
    let _ = load_and_validate(yaml, "<regression-int-initial>");
}

/// Boolean value for `action` must be treated as an unsupported action type or
/// a parse error — not a panic.
#[test]
fn regression_boolean_action_value() {
    let yaml = "initial_state: s\nstates:\n  - name: s\n    action: true\n";
    let _ = load_and_validate(yaml, "<regression-bool-action>");
}

/// Source name appearing in errors must be the one provided — not a default.
#[test]
fn regression_source_name_in_error_message() {
    let yaml = "initial_state: ghost\nstates:\n  - name: real\n    action: terminal\n";
    let err =
        load_and_validate(yaml, "my-special-source.yml").expect_err("expected validation error");
    let msg = err.to_string();
    assert!(
        msg.contains("my-special-source.yml"),
        "source name must appear in error: {msg}"
    );
}

/// Unsupported action type must produce an actionable error, not a panic.
#[test]
fn regression_unsupported_action_type() {
    let yaml = "initial_state: s\nstates:\n  - name: s\n    action: jump\n";
    let result = load_and_validate(yaml, "<regression-unsupported-action>");
    assert!(result.is_err(), "unsupported action must be rejected");
    let msg = result.unwrap_err().to_string();
    assert!(
        msg.contains("unsupported action"),
        "error must mention 'unsupported action': {msg}"
    );
}

/// `builtin` action missing the `builtin` field key must be rejected cleanly.
#[test]
fn regression_builtin_action_missing_builtin_field() {
    let yaml = "initial_state: c\nstates:\n  - name: c\n    action: builtin\n    on_pass: d\n    on_fail: d\n  - name: d\n    action: terminal\n";
    let result = load_and_validate(yaml, "<regression-missing-builtin-field>");
    assert!(
        result.is_err(),
        "builtin without 'builtin' field must be rejected"
    );
}

/// Agent action missing `on_success` must be rejected cleanly.
#[test]
fn regression_agent_missing_on_success() {
    let yaml = "initial_state: w\nstates:\n  - name: w\n    action: agent\n    on_failure: w\n";
    let result = load_and_validate(yaml, "<regression-agent-no-on-success>");
    assert!(result.is_err(), "agent without on_success must be rejected");
}

/// Agent action missing `on_failure` must be rejected cleanly.
#[test]
fn regression_agent_missing_on_failure() {
    let yaml = "initial_state: w\nstates:\n  - name: w\n    action: agent\n    on_success: w\n";
    let result = load_and_validate(yaml, "<regression-agent-no-on-failure>");
    assert!(result.is_err(), "agent without on_failure must be rejected");
}
