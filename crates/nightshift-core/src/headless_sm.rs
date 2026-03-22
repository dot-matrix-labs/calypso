//! Minimum supported headless YAML state-machine schema.
//!
//! This module defines the schema, loader, and validator for user-authored YAML
//! state-machine files used in Nightshift headless execution.  Only the minimum
//! contract needed to execute a simple looping state machine is covered here.
//!
//! # Schema
//!
//! A headless state-machine file contains a single top-level mapping with the
//! following keys:
//!
//! ```yaml
//! name: optional-workflow-name   # optional
//! initial_state: scan            # required — must name a state in `states`
//!
//! states:
//!   - name: scan
//!     action: agent              # agent | builtin | loop | terminal
//!     on_success: check
//!     on_failure: error
//!
//!   - name: check
//!     action: builtin
//!     builtin: builtin.git.is_main_compatible
//!     on_pass: done
//!     on_fail: retry
//!
//!   - name: retry
//!     action: loop
//!     target: scan
//!
//!   - name: done
//!     action: terminal
//!
//!   - name: error
//!     action: terminal
//! ```
//!
//! # Supported action types
//!
//! | `action`   | Required fields                | Optional fields |
//! |------------|-------------------------------|-----------------|
//! | `agent`    | `on_success`, `on_failure`    | —               |
//! | `builtin`  | `builtin`, `on_pass`, `on_fail` | —             |
//! | `loop`     | `target`                      | —               |
//! | `terminal` | —                             | —               |
//!
//! Any other value for `action` is rejected during validation.
//!
//! # Validation
//!
//! [`load_and_validate`] performs a single validation pass over the parsed
//! graph and returns a [`ValidationError`] if any structural rule is violated.
//! The following pathological inputs are explicitly covered:
//!
//! - Missing `states` list (empty state graph)
//! - Duplicate state names
//! - Missing or non-existent `initial_state`
//! - Missing target states in `on_success`, `on_failure`, `on_pass`, `on_fail`,
//!   or `target` transitions
//! - Empty step lists where steps are required
//! - Invalid loop targets (target state does not exist)
//! - Unsupported `action` types
//! - Ambiguous transition definitions (conflicting or overlapping transition keys)
//!
//! All error messages are specific and actionable.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use serde::Deserialize;

// ── Schema types ─────────────────────────────────────────────────────────────

/// A fully parsed and validated headless state-machine graph loaded from one
/// or more user-authored YAML files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessStateMachine {
    /// Optional workflow display name.
    pub name: Option<String>,
    /// The name of the initial state.
    pub initial_state: String,
    /// All states in the graph, keyed by name.
    pub states: BTreeMap<String, HeadlessState>,
}

/// A single state in the headless state-machine graph.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeadlessState {
    /// The state name (also used as the map key in [`HeadlessStateMachine`]).
    pub name: String,
    /// The step type that drives this state.
    pub action: HeadlessAction,
}

/// The action (step type) for a headless state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HeadlessAction {
    /// Run a supervised agent session.
    ///
    /// Fields: `on_success` → next state on success, `on_failure` → next state on failure.
    Agent {
        on_success: String,
        on_failure: String,
    },
    /// Evaluate a built-in function.
    ///
    /// Fields: `builtin` keyword, `on_pass` → next state when the builtin passes,
    /// `on_fail` → next state when the builtin fails.
    Builtin {
        builtin: String,
        on_pass: String,
        on_fail: String,
    },
    /// Loop back to another state unconditionally.
    ///
    /// Field: `target` → the state to jump to.
    Loop { target: String },
    /// Terminal state — execution stops here.
    Terminal,
}

// ── Raw deserialization types ────────────────────────────────────────────────

/// Raw, unvalidated state-machine document deserialized directly from YAML.
#[derive(Debug, Deserialize)]
struct RawStateMachine {
    #[serde(default)]
    name: Option<String>,
    initial_state: String,
    #[serde(default)]
    states: Vec<RawState>,
}

/// Raw, unvalidated state entry deserialized from YAML.
#[derive(Debug, Deserialize)]
struct RawState {
    name: String,
    action: String,
    // builtin action
    #[serde(default)]
    builtin: Option<String>,
    // agent transitions
    #[serde(default)]
    on_success: Option<String>,
    #[serde(default)]
    on_failure: Option<String>,
    // builtin transitions
    #[serde(default)]
    on_pass: Option<String>,
    #[serde(default)]
    on_fail: Option<String>,
    // loop
    #[serde(default)]
    target: Option<String>,
}

// ── Validation error ─────────────────────────────────────────────────────────

/// A validation error produced when a headless YAML state-machine fails its
/// single-pass structural check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationError {
    /// The source file name (or `"<inline>"` for in-memory content).
    pub source: String,
    /// Human-readable, actionable description of the problem.
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.source, self.message)
    }
}

impl std::error::Error for ValidationError {}

// ── Load error ───────────────────────────────────────────────────────────────

/// An error produced while loading (parsing or validating) a headless YAML
/// state-machine file.
#[derive(Debug)]
pub enum LoadError {
    /// An I/O error occurred while reading the file.
    Io {
        source: String,
        error: std::io::Error,
    },
    /// The YAML content could not be parsed.
    Yaml {
        source: String,
        error: serde_yaml::Error,
    },
    /// The parsed graph failed structural validation.
    Validation(ValidationError),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io { source, error } => {
                write!(f, "[{source}] I/O error: {error}")
            }
            LoadError::Yaml { source, error } => {
                write!(f, "[{source}] YAML parse error: {error}")
            }
            LoadError::Validation(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for LoadError {}

// ── Public API ───────────────────────────────────────────────────────────────

/// Parse and validate a headless state-machine from an in-memory YAML string.
///
/// `source_name` is used in error messages (e.g. the file name).
///
/// Returns [`LoadError::Yaml`] if parsing fails, or [`LoadError::Validation`]
/// if the graph is structurally invalid.
pub fn load_and_validate(yaml: &str, source_name: &str) -> Result<HeadlessStateMachine, LoadError> {
    let raw: RawStateMachine = serde_yaml::from_str(yaml).map_err(|e| LoadError::Yaml {
        source: source_name.to_string(),
        error: e,
    })?;

    validate_raw(raw, source_name).map_err(LoadError::Validation)
}

/// Read a file from disk and parse and validate it as a headless state-machine.
///
/// Returns [`LoadError::Io`] if the file cannot be read, [`LoadError::Yaml`]
/// if parsing fails, or [`LoadError::Validation`] if the graph is structurally
/// invalid.
pub fn load_and_validate_file(path: &Path) -> Result<HeadlessStateMachine, LoadError> {
    let source_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string());

    let yaml = std::fs::read_to_string(path).map_err(|e| LoadError::Io {
        source: source_name.clone(),
        error: e,
    })?;

    load_and_validate(&yaml, &source_name)
}

/// Load and validate multiple headless state-machine YAML files.
///
/// Each file is loaded independently.  The first failure stops the load and
/// returns the corresponding error.  On success all machines are returned in
/// the order the paths were supplied.
pub fn load_and_validate_files(paths: &[&Path]) -> Result<Vec<HeadlessStateMachine>, LoadError> {
    paths.iter().map(|p| load_and_validate_file(p)).collect()
}

// ── Internal validation ──────────────────────────────────────────────────────

/// Validate a raw deserialized state-machine document and convert it into a
/// [`HeadlessStateMachine`].
fn validate_raw(
    raw: RawStateMachine,
    source: &str,
) -> Result<HeadlessStateMachine, ValidationError> {
    let err = |message: String| ValidationError {
        source: source.to_string(),
        message,
    };

    // 1. States list must not be empty.
    if raw.states.is_empty() {
        return Err(err(
            "state machine has no states; at least one state is required".to_string(),
        ));
    }

    // 2. Detect duplicate state names.
    {
        let mut seen: BTreeSet<String> = BTreeSet::new();
        for state in &raw.states {
            if !seen.insert(state.name.clone()) {
                return Err(err(format!(
                    "duplicate state name '{}'; each state must have a unique name",
                    state.name
                )));
            }
        }
    }

    // 3. initial_state must exist in the states list.
    //
    // Collect state names first, then consume `raw.states` in the loop below.
    let state_names: BTreeSet<String> = raw.states.iter().map(|s| s.name.clone()).collect();
    let state_name_refs: BTreeSet<&str> = state_names.iter().map(|s| s.as_str()).collect();
    if raw.initial_state.is_empty() {
        return Err(err(
            "initial_state is empty; specify the name of the starting state".to_string(),
        ));
    }
    if !state_name_refs.contains(raw.initial_state.as_str()) {
        return Err(err(format!(
            "initial_state '{}' does not name any defined state",
            raw.initial_state
        )));
    }

    // 4. Validate each state and convert to the typed representation.
    let mut states: BTreeMap<String, HeadlessState> = BTreeMap::new();
    for raw_state in raw.states {
        let action = validate_state_action(&raw_state, &state_name_refs, source)?;
        states.insert(
            raw_state.name.clone(),
            HeadlessState {
                name: raw_state.name,
                action,
            },
        );
    }

    Ok(HeadlessStateMachine {
        name: raw.name,
        initial_state: raw.initial_state,
        states,
    })
}

/// Supported action type strings.
const SUPPORTED_ACTIONS: &[&str] = &["agent", "builtin", "loop", "terminal"];

/// Validate a single raw state's action and transition fields.
///
/// Requires `state_names` to be the complete set of state names so target
/// references can be resolved.
fn validate_state_action(
    raw: &RawState,
    state_names: &BTreeSet<&str>,
    source: &str,
) -> Result<HeadlessAction, ValidationError> {
    let err = |message: String| ValidationError {
        source: source.to_string(),
        message,
    };

    let require_target = |field: &str, value: &Option<String>| -> Result<String, ValidationError> {
        let name = value.as_deref().ok_or_else(|| {
            err(format!(
                "state '{}': action '{}' requires field '{field}'",
                raw.name, raw.action
            ))
        })?;
        if name.is_empty() {
            return Err(err(format!(
                "state '{}': field '{field}' must not be empty",
                raw.name
            )));
        }
        if !state_names.contains(name) {
            return Err(err(format!(
                "state '{}': '{field}' references unknown state '{name}'",
                raw.name
            )));
        }
        Ok(name.to_string())
    };

    match raw.action.as_str() {
        "agent" => {
            // Detect ambiguous transition definitions: agent states must not
            // also carry builtin-specific or loop-specific fields.
            if raw.builtin.is_some() || raw.on_pass.is_some() || raw.on_fail.is_some() {
                return Err(err(format!(
                    "state '{}': action 'agent' must not define 'builtin', 'on_pass', or 'on_fail'",
                    raw.name
                )));
            }
            if raw.target.is_some() {
                return Err(err(format!(
                    "state '{}': action 'agent' must not define 'target'",
                    raw.name
                )));
            }
            let on_success = require_target("on_success", &raw.on_success)?;
            let on_failure = require_target("on_failure", &raw.on_failure)?;
            Ok(HeadlessAction::Agent {
                on_success,
                on_failure,
            })
        }

        "builtin" => {
            // Ambiguity check: builtin states must not carry agent or loop fields.
            if raw.on_success.is_some() || raw.on_failure.is_some() {
                return Err(err(format!(
                    "state '{}': action 'builtin' must not define 'on_success' or 'on_failure'",
                    raw.name
                )));
            }
            if raw.target.is_some() {
                return Err(err(format!(
                    "state '{}': action 'builtin' must not define 'target'",
                    raw.name
                )));
            }
            let builtin_kw = raw.builtin.as_deref().ok_or_else(|| {
                err(format!(
                    "state '{}': action 'builtin' requires field 'builtin'",
                    raw.name
                ))
            })?;
            if builtin_kw.is_empty() {
                return Err(err(format!(
                    "state '{}': field 'builtin' must not be empty",
                    raw.name
                )));
            }
            if !builtin_kw.starts_with("builtin.") {
                return Err(err(format!(
                    "state '{}': 'builtin' value '{}' must start with 'builtin.' (e.g. builtin.git.is_main_compatible)",
                    raw.name, builtin_kw
                )));
            }
            let on_pass = require_target("on_pass", &raw.on_pass)?;
            let on_fail = require_target("on_fail", &raw.on_fail)?;
            Ok(HeadlessAction::Builtin {
                builtin: builtin_kw.to_string(),
                on_pass,
                on_fail,
            })
        }

        "loop" => {
            // Ambiguity check: loop states must not carry agent or builtin fields.
            if raw.on_success.is_some()
                || raw.on_failure.is_some()
                || raw.on_pass.is_some()
                || raw.on_fail.is_some()
                || raw.builtin.is_some()
            {
                return Err(err(format!(
                    "state '{}': action 'loop' must only define 'target'",
                    raw.name
                )));
            }
            let target = require_target("target", &raw.target)?;
            Ok(HeadlessAction::Loop { target })
        }

        "terminal" => {
            // Ambiguity check: terminal states must not carry any transition fields.
            if raw.on_success.is_some()
                || raw.on_failure.is_some()
                || raw.on_pass.is_some()
                || raw.on_fail.is_some()
                || raw.target.is_some()
                || raw.builtin.is_some()
            {
                return Err(err(format!(
                    "state '{}': action 'terminal' must not define any transition fields",
                    raw.name
                )));
            }
            Ok(HeadlessAction::Terminal)
        }

        other => Err(err(format!(
            "state '{}': unsupported action '{}'; supported actions are: {}",
            raw.name,
            other,
            SUPPORTED_ACTIONS.join(", ")
        ))),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── helpers ──────────────────────────────────────────────────────────────

    fn ok(yaml: &str) -> HeadlessStateMachine {
        load_and_validate(yaml, "<test>").expect("expected Ok but got Err")
    }

    fn err_msg(yaml: &str) -> String {
        match load_and_validate(yaml, "<test>") {
            Ok(_) => panic!("expected Err but got Ok"),
            Err(e) => e.to_string(),
        }
    }

    // ── happy path ───────────────────────────────────────────────────────────

    #[test]
    fn minimal_terminal_machine_loads() {
        let yaml = r#"
initial_state: idle
states:
  - name: idle
    action: terminal
"#;
        let sm = ok(yaml);
        assert_eq!(sm.initial_state, "idle");
        assert_eq!(sm.states.len(), 1);
        assert_eq!(sm.states["idle"].action, HeadlessAction::Terminal);
    }

    #[test]
    fn agent_state_loads_correctly() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
    on_failure: error
  - name: done
    action: terminal
  - name: error
    action: terminal
"#;
        let sm = ok(yaml);
        assert_eq!(
            sm.states["work"].action,
            HeadlessAction::Agent {
                on_success: "done".to_string(),
                on_failure: "error".to_string(),
            }
        );
    }

    #[test]
    fn builtin_state_loads_correctly() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_pass: done
    on_fail: error
  - name: done
    action: terminal
  - name: error
    action: terminal
"#;
        let sm = ok(yaml);
        assert_eq!(
            sm.states["check"].action,
            HeadlessAction::Builtin {
                builtin: "builtin.git.is_main_compatible".to_string(),
                on_pass: "done".to_string(),
                on_fail: "error".to_string(),
            }
        );
    }

    #[test]
    fn loop_state_loads_correctly() {
        let yaml = r#"
initial_state: scan
states:
  - name: scan
    action: agent
    on_success: done
    on_failure: retry
  - name: retry
    action: loop
    target: scan
  - name: done
    action: terminal
"#;
        let sm = ok(yaml);
        assert_eq!(
            sm.states["retry"].action,
            HeadlessAction::Loop {
                target: "scan".to_string(),
            }
        );
    }

    #[test]
    fn optional_name_field_is_preserved() {
        let yaml = r#"
name: my-recovery-workflow
initial_state: idle
states:
  - name: idle
    action: terminal
"#;
        let sm = ok(yaml);
        assert_eq!(sm.name, Some("my-recovery-workflow".to_string()));
    }

    #[test]
    fn no_name_field_gives_none() {
        let yaml = r#"
initial_state: idle
states:
  - name: idle
    action: terminal
"#;
        let sm = ok(yaml);
        assert_eq!(sm.name, None);
    }

    /// Verifies the full user-recovery-workflow.yml fixture loads successfully.
    #[test]
    fn user_recovery_workflow_fixture_loads() {
        let yaml = include_str!("../tests/fixtures/user-recovery-workflow.yml");
        let sm = ok(yaml);
        assert_eq!(sm.initial_state, "scan");
        assert_eq!(sm.states.len(), 5);
        assert!(sm.states.contains_key("scan"));
        assert!(sm.states.contains_key("check"));
        assert!(sm.states.contains_key("retry"));
        assert!(sm.states.contains_key("done"));
        assert!(sm.states.contains_key("error"));
    }

    // ── missing states ───────────────────────────────────────────────────────

    #[test]
    fn empty_states_list_is_rejected() {
        let yaml = r#"
initial_state: idle
states: []
"#;
        let msg = err_msg(yaml);
        assert!(msg.contains("no states"), "expected 'no states' in: {msg}");
    }

    #[test]
    fn missing_states_key_is_rejected() {
        // `states` defaults to an empty vec via serde default
        let yaml = "initial_state: idle\n";
        let msg = err_msg(yaml);
        assert!(msg.contains("no states"), "expected 'no states' in: {msg}");
    }

    // ── duplicate state names ─────────────────────────────────────────────────

    #[test]
    fn duplicate_state_names_are_rejected() {
        let yaml = r#"
initial_state: idle
states:
  - name: idle
    action: terminal
  - name: idle
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("duplicate state name 'idle'"),
            "expected duplicate error in: {msg}"
        );
    }

    // ── missing start state ───────────────────────────────────────────────────

    #[test]
    fn initial_state_not_in_states_is_rejected() {
        let yaml = r#"
initial_state: nonexistent
states:
  - name: idle
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("initial_state 'nonexistent'"),
            "expected initial_state error in: {msg}"
        );
    }

    // ── missing target states ─────────────────────────────────────────────────

    #[test]
    fn agent_on_success_targeting_unknown_state_is_rejected() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: ghost
    on_failure: error
  - name: error
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("unknown state 'ghost'"),
            "expected unknown state error in: {msg}"
        );
    }

    #[test]
    fn agent_on_failure_targeting_unknown_state_is_rejected() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
    on_failure: ghost
  - name: done
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("unknown state 'ghost'"),
            "expected unknown state error in: {msg}"
        );
    }

    #[test]
    fn builtin_on_pass_targeting_unknown_state_is_rejected() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_pass: ghost
    on_fail: error
  - name: error
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("unknown state 'ghost'"),
            "expected unknown state error in: {msg}"
        );
    }

    #[test]
    fn builtin_on_fail_targeting_unknown_state_is_rejected() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_pass: done
    on_fail: ghost
  - name: done
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("unknown state 'ghost'"),
            "expected unknown state error in: {msg}"
        );
    }

    // ── empty step lists (missing required fields) ────────────────────────────

    #[test]
    fn agent_missing_on_success_is_rejected() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_failure: error
  - name: error
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("on_success"),
            "expected on_success error in: {msg}"
        );
    }

    #[test]
    fn agent_missing_on_failure_is_rejected() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
  - name: done
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("on_failure"),
            "expected on_failure error in: {msg}"
        );
    }

    #[test]
    fn builtin_missing_builtin_field_is_rejected() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    on_pass: done
    on_fail: error
  - name: done
    action: terminal
  - name: error
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("'builtin'"),
            "expected builtin field error in: {msg}"
        );
    }

    #[test]
    fn builtin_missing_on_pass_is_rejected() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_fail: error
  - name: error
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(msg.contains("on_pass"), "expected on_pass error in: {msg}");
    }

    #[test]
    fn builtin_missing_on_fail_is_rejected() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_pass: done
  - name: done
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(msg.contains("on_fail"), "expected on_fail error in: {msg}");
    }

    #[test]
    fn loop_missing_target_is_rejected() {
        let yaml = r#"
initial_state: scan
states:
  - name: scan
    action: loop
"#;
        let msg = err_msg(yaml);
        assert!(msg.contains("target"), "expected target error in: {msg}");
    }

    // ── invalid loop targets ──────────────────────────────────────────────────

    #[test]
    fn loop_targeting_unknown_state_is_rejected() {
        let yaml = r#"
initial_state: scan
states:
  - name: scan
    action: loop
    target: ghost
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("unknown state 'ghost'"),
            "expected unknown target error in: {msg}"
        );
    }

    // ── unsupported action types ──────────────────────────────────────────────

    #[test]
    fn unsupported_action_type_is_rejected() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: magic
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("unsupported action 'magic'"),
            "expected unsupported action error in: {msg}"
        );
        assert!(
            msg.contains("agent"),
            "expected supported actions list in: {msg}"
        );
    }

    // ── ambiguous transition definitions ──────────────────────────────────────

    #[test]
    fn agent_state_with_builtin_fields_is_rejected() {
        let yaml = r#"
initial_state: work
states:
  - name: work
    action: agent
    on_success: done
    on_failure: error
    on_pass: done
  - name: done
    action: terminal
  - name: error
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("on_pass"),
            "expected on_pass ambiguity error in: {msg}"
        );
    }

    #[test]
    fn builtin_state_with_agent_fields_is_rejected() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_pass: done
    on_fail: error
    on_success: done
  - name: done
    action: terminal
  - name: error
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("on_success"),
            "expected on_success ambiguity error in: {msg}"
        );
    }

    #[test]
    fn terminal_state_with_transition_fields_is_rejected() {
        let yaml = r#"
initial_state: done
states:
  - name: done
    action: terminal
    on_success: done
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("terminal"),
            "expected terminal ambiguity error in: {msg}"
        );
    }

    #[test]
    fn loop_state_with_extra_fields_is_rejected() {
        let yaml = r#"
initial_state: retry
states:
  - name: retry
    action: loop
    target: retry
    on_success: retry
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("'loop' must only define 'target'"),
            "expected loop ambiguity error in: {msg}"
        );
    }

    // ── builtin keyword validation ────────────────────────────────────────────

    #[test]
    fn builtin_keyword_not_starting_with_builtin_dot_is_rejected() {
        let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: my.custom.evaluator
    on_pass: done
    on_fail: error
  - name: done
    action: terminal
  - name: error
    action: terminal
"#;
        let msg = err_msg(yaml);
        assert!(
            msg.contains("must start with 'builtin.'"),
            "expected builtin prefix error in: {msg}"
        );
    }

    // ── file loading ──────────────────────────────────────────────────────────

    #[test]
    fn load_and_validate_file_loads_fixture() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/user-recovery-workflow.yml");
        let sm = load_and_validate_file(&path).expect("fixture should load");
        assert_eq!(sm.initial_state, "scan");
    }

    #[test]
    fn load_and_validate_file_returns_io_error_for_missing_file() {
        let path = std::path::Path::new("/tmp/calypso-headless-sm-no-such-file.yml");
        let err = load_and_validate_file(path).expect_err("expected Err for missing file");
        assert!(
            matches!(err, LoadError::Io { .. }),
            "expected Io error variant, got: {err}"
        );
    }

    #[test]
    fn load_and_validate_files_loads_multiple_files() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/user-recovery-workflow.yml");
        let paths: Vec<&Path> = vec![path.as_path(), path.as_path()];
        let machines =
            load_and_validate_files(&paths).expect("should load multiple instances of fixture");
        assert_eq!(machines.len(), 2);
    }

    #[test]
    fn load_and_validate_files_stops_at_first_failure() {
        let good = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures/user-recovery-workflow.yml");
        let bad = std::path::Path::new("/tmp/calypso-no-such-headless-sm.yml");
        let paths: Vec<&Path> = vec![good.as_path(), bad];
        let err = load_and_validate_files(&paths).expect_err("expected Err for missing file");
        assert!(
            matches!(err, LoadError::Io { .. }),
            "expected Io error, got: {err}"
        );
    }

    // ── error message formatting ──────────────────────────────────────────────

    #[test]
    fn validation_error_display_includes_source_and_message() {
        let e = ValidationError {
            source: "my-file.yml".to_string(),
            message: "something went wrong".to_string(),
        };
        let display = e.to_string();
        assert!(display.contains("[my-file.yml]"));
        assert!(display.contains("something went wrong"));
    }

    #[test]
    fn load_error_yaml_display_includes_source() {
        let result = load_and_validate("not: valid: yaml: {", "bad-file.yml");
        let msg = result.expect_err("expected Err").to_string();
        assert!(msg.contains("bad-file.yml"), "expected source in: {msg}");
    }

    #[test]
    fn load_and_validate_returns_yaml_error_for_invalid_yaml() {
        let result = load_and_validate(": bad :", "<test>");
        let err = result.expect_err("expected Err");
        assert!(
            matches!(err, LoadError::Yaml { .. }),
            "expected Yaml error variant"
        );
    }

    #[test]
    fn load_and_validate_returns_validation_error_for_bad_graph() {
        let result = load_and_validate("initial_state: idle\nstates: []\n", "<test>");
        let err = result.expect_err("expected Err");
        assert!(
            matches!(err, LoadError::Validation(_)),
            "expected Validation error variant"
        );
    }

    // ── derived trait smoke tests ─────────────────────────────────────────────

    #[test]
    fn headless_action_debug_and_eq() {
        let a = HeadlessAction::Terminal;
        let b = HeadlessAction::Terminal;
        assert_eq!(a, b);
        assert_eq!(format!("{a:?}"), "Terminal");
    }

    #[test]
    fn headless_state_debug_and_clone() {
        let s = HeadlessState {
            name: "idle".to_string(),
            action: HeadlessAction::Terminal,
        };
        let s2 = s.clone();
        assert_eq!(s, s2);
        assert!(format!("{s:?}").contains("idle"));
    }

    #[test]
    fn headless_state_machine_debug_and_clone() {
        let yaml = r#"
name: test
initial_state: idle
states:
  - name: idle
    action: terminal
"#;
        let sm = ok(yaml);
        let sm2 = sm.clone();
        assert_eq!(sm, sm2);
        assert!(format!("{sm:?}").contains("idle"));
    }
}
