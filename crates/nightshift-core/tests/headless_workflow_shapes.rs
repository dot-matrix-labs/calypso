//! Integration tests for issue #189: mocked happy-path coverage for core headless
//! workflow shapes.
//!
//! Each test:
//!   - Constructs a small YAML state machine for one of the four canonical shapes.
//!   - Drives it with a `ScriptedExecutor` (no live services).
//!   - Asserts the correct `ExitReason`.
//!   - Asserts the **ordered** sequence of structured log events (state_entered,
//!     step_executed, transition_selected) extracted from the captured JSON output.
//!
//! The four shapes tested:
//!   1. Straight-line: A -> B -> Terminal (no loops)
//!   2. Branching: A -> (success -> B | failure -> C) -> Terminal
//!   3. Looping: A -> retry -> A -> ... -> Terminal
//!   4. Full event-type ordering assertion (agent and builtin)
//!
//! The Interrupted shape (ExitReason::Interrupted) is covered by the module-level
//! unit tests in `headless_sm_driver.rs`, which have access to the crate-internal
//! `ShutdownSignal::from_receiver` constructor.

use nightshift_core::headless_sm::load_and_validate;
use nightshift_core::headless_sm_driver::{
    AgentOutcome, BuiltinOutcome, ExitReason, HeadlessSmDriver, StepExecutor,
};
use nightshift_core::telemetry::{LogFormat, LogLevel, Logger};
use std::sync::{Arc, Mutex};

// ── Shared test helpers ───────────────────────────────────────────────────────

/// A `Write` implementation that accumulates bytes in a shared buffer.
#[derive(Clone)]
struct CaptureWriter {
    buf: Arc<Mutex<Vec<u8>>>,
}

impl CaptureWriter {
    fn new() -> Self {
        Self {
            buf: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn contents(&self) -> String {
        String::from_utf8_lossy(&self.buf.lock().unwrap()).to_string()
    }
}

impl std::io::Write for CaptureWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.buf.lock().unwrap().extend_from_slice(buf);
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn make_logger(w: CaptureWriter) -> Logger {
    Logger::_with_level_and_writer(LogLevel::Debug, Box::new(w)).with_format(LogFormat::Json)
}

/// A `StepExecutor` that returns pre-scripted outcomes in order.
///
/// Panics with a descriptive message if either queue is exhausted early.
struct ScriptedExecutor {
    agents: Mutex<Vec<AgentOutcome>>,
    builtins: Mutex<Vec<BuiltinOutcome>>,
}

impl ScriptedExecutor {
    fn new(agents: Vec<AgentOutcome>, builtins: Vec<BuiltinOutcome>) -> Self {
        Self {
            agents: Mutex::new(agents),
            builtins: Mutex::new(builtins),
        }
    }
}

impl StepExecutor for ScriptedExecutor {
    fn run_agent(&self, state: &str) -> AgentOutcome {
        let mut q = self.agents.lock().unwrap();
        if q.is_empty() {
            panic!("ScriptedExecutor: agent queue empty (state={state})");
        }
        q.remove(0)
    }

    fn run_builtin(&self, state: &str, key: &str) -> BuiltinOutcome {
        let mut q = self.builtins.lock().unwrap();
        if q.is_empty() {
            panic!("ScriptedExecutor: builtin queue empty (state={state}, key={key})");
        }
        q.remove(0)
    }
}

/// Parse JSON-lines output and extract every value of the given top-level key
/// in document order.
fn extract_field_sequence(output: &str, key: &str) -> Vec<String> {
    output
        .lines()
        .filter(|l| !l.trim().is_empty())
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            v.get(key)?.as_str().map(|s| s.to_string())
        })
        .collect()
}

/// Extract state names from all `state_entered` log lines in document order.
///
/// The `state` field lives inside the `fields` sub-object of each JSON log entry.
fn state_entry_sequence(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|l| l.contains("\"event\":\"state_entered\""))
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            v.get("fields")?.get("state")?.as_str().map(|s| s.to_string())
        })
        .collect()
}

/// Extract transition names from all `transition_selected` log lines in document order.
///
/// The `transition` field lives inside the `fields` sub-object of each JSON log entry.
fn transition_sequence(output: &str) -> Vec<String> {
    output
        .lines()
        .filter(|l| l.contains("\"event\":\"transition_selected\""))
        .filter_map(|line| {
            let v: serde_json::Value = serde_json::from_str(line).ok()?;
            v.get("fields")?.get("transition")?.as_str().map(|s| s.to_string())
        })
        .collect()
}

// ── Shape 1: Straight-line  A -> B -> Terminal ────────────────────────────────

/// Straight-line workflow: work --(agent success)--> done (terminal).
///
/// Ordered log assertions:
///   states entered: [work, done]
///   transitions selected: [on_success]
#[test]
fn straight_line_a_to_b_to_terminal() {
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
    let sm = load_and_validate(yaml, "<test>").unwrap();
    let executor = ScriptedExecutor::new(vec![AgentOutcome::Success], vec![]);
    let w = CaptureWriter::new();
    let logger = make_logger(w.clone());

    let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

    assert_eq!(
        result,
        ExitReason::Terminal {
            state: "done".to_string()
        },
        "expected Terminal(done)"
    );

    let output = w.contents();

    assert_eq!(
        transition_sequence(&output),
        vec!["on_success"],
        "expected exactly one on_success transition"
    );

    assert_eq!(
        state_entry_sequence(&output),
        vec!["work", "done"],
        "expected states entered in order [work, done]"
    );
}

/// Straight-line workflow via failure path: work --(agent failure)--> error (terminal).
///
/// Ordered log assertions:
///   states entered: [work, error]
///   transitions selected: [on_failure]
#[test]
fn straight_line_a_to_c_via_failure() {
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
    let sm = load_and_validate(yaml, "<test>").unwrap();
    let executor = ScriptedExecutor::new(
        vec![AgentOutcome::Failure {
            reason: "task error".to_string(),
        }],
        vec![],
    );
    let w = CaptureWriter::new();
    let logger = make_logger(w.clone());

    let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

    assert_eq!(
        result,
        ExitReason::Terminal {
            state: "error".to_string()
        },
        "expected Terminal(error)"
    );

    let output = w.contents();

    assert_eq!(
        transition_sequence(&output),
        vec!["on_failure"],
        "expected exactly one on_failure transition"
    );

    assert_eq!(
        state_entry_sequence(&output),
        vec!["work", "error"],
        "expected states entered in order [work, error]"
    );
}

// ── Shape 2: Branching  A -> (success -> B | failure -> C) -> Terminal ─────────

/// Branching success branch: gate --(success)--> branch_b (terminal).
#[test]
fn branching_success_branch_reaches_b() {
    let yaml = r#"
initial_state: gate
states:
  - name: gate
    action: agent
    on_success: branch_b
    on_failure: branch_c
  - name: branch_b
    action: terminal
  - name: branch_c
    action: terminal
"#;
    let sm = load_and_validate(yaml, "<test>").unwrap();
    let executor = ScriptedExecutor::new(vec![AgentOutcome::Success], vec![]);
    let w = CaptureWriter::new();
    let logger = make_logger(w.clone());

    let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

    assert_eq!(
        result,
        ExitReason::Terminal {
            state: "branch_b".to_string()
        },
        "success branch must reach branch_b"
    );

    let output = w.contents();

    assert_eq!(
        transition_sequence(&output),
        vec!["on_success"],
        "expected on_success transition only"
    );

    assert_eq!(
        state_entry_sequence(&output),
        vec!["gate", "branch_b"],
        "expected states [gate, branch_b] in order"
    );
}

/// Branching failure branch: gate --(failure)--> branch_c (terminal).
#[test]
fn branching_failure_branch_reaches_c() {
    let yaml = r#"
initial_state: gate
states:
  - name: gate
    action: agent
    on_success: branch_b
    on_failure: branch_c
  - name: branch_b
    action: terminal
  - name: branch_c
    action: terminal
"#;
    let sm = load_and_validate(yaml, "<test>").unwrap();
    let executor = ScriptedExecutor::new(
        vec![AgentOutcome::Failure {
            reason: "gate failed".to_string(),
        }],
        vec![],
    );
    let w = CaptureWriter::new();
    let logger = make_logger(w.clone());

    let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

    assert_eq!(
        result,
        ExitReason::Terminal {
            state: "branch_c".to_string()
        },
        "failure branch must reach branch_c"
    );

    let output = w.contents();

    assert_eq!(
        transition_sequence(&output),
        vec!["on_failure"],
        "expected on_failure transition only"
    );

    assert_eq!(
        state_entry_sequence(&output),
        vec!["gate", "branch_c"],
        "expected states [gate, branch_c] in order"
    );
}

// ── Shape 3: Looping  A -> retry -> A -> ... -> Terminal ─────────────────────

/// Looping workflow: scan fails twice, succeeds on the third attempt.
///
/// State sequence: scan, retry, scan, retry, scan, done
/// Transition sequence: on_failure, loop, on_failure, loop, on_success
#[test]
fn looping_scan_fails_twice_then_succeeds() {
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
    let sm = load_and_validate(yaml, "<test>").unwrap();
    let executor = ScriptedExecutor::new(
        vec![
            AgentOutcome::Failure {
                reason: "attempt 1 failed".to_string(),
            },
            AgentOutcome::Failure {
                reason: "attempt 2 failed".to_string(),
            },
            AgentOutcome::Success,
        ],
        vec![],
    );
    let w = CaptureWriter::new();
    let logger = make_logger(w.clone());

    let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

    assert_eq!(
        result,
        ExitReason::Terminal {
            state: "done".to_string()
        },
        "expected Terminal(done) after two retries"
    );

    let output = w.contents();

    assert_eq!(
        state_entry_sequence(&output),
        vec!["scan", "retry", "scan", "retry", "scan", "done"],
        "unexpected state entry order"
    );

    assert_eq!(
        transition_sequence(&output),
        vec!["on_failure", "loop", "on_failure", "loop", "on_success"],
        "unexpected transition order"
    );

    // Three agent calls => at least three step_executed entries
    let step_executed_count = output
        .lines()
        .filter(|l| l.contains("\"event\":\"step_executed\""))
        .count();

    assert!(
        step_executed_count >= 3,
        "expected at least 3 step_executed events, got {step_executed_count}"
    );
}

/// Builtin-gated looping workflow.
///
/// Layout: scan (agent) -> gate (builtin pass/fail) -> done/retry -> scan.
/// First gate fails, second passes.
///
/// State sequence: scan, gate, retry, scan, gate, done
/// Transition sequence: on_success, on_fail, loop, on_success, on_pass
#[test]
fn looping_builtin_gate_retries_until_pass() {
    let yaml = r#"
initial_state: scan
states:
  - name: scan
    action: agent
    on_success: gate
    on_failure: done
  - name: gate
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_pass: done
    on_fail: retry
  - name: retry
    action: loop
    target: scan
  - name: done
    action: terminal
"#;
    let sm = load_and_validate(yaml, "<test>").unwrap();
    let executor = ScriptedExecutor::new(
        vec![AgentOutcome::Success, AgentOutcome::Success],
        vec![
            BuiltinOutcome::Fail {
                reason: "not ready".to_string(),
            },
            BuiltinOutcome::Pass,
        ],
    );
    let w = CaptureWriter::new();
    let logger = make_logger(w.clone());

    let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

    assert_eq!(
        result,
        ExitReason::Terminal {
            state: "done".to_string()
        },
        "expected Terminal(done) after gate retry"
    );

    let output = w.contents();

    assert_eq!(
        state_entry_sequence(&output),
        vec!["scan", "gate", "retry", "scan", "gate", "done"],
        "unexpected state entry order"
    );

    assert_eq!(
        transition_sequence(&output),
        vec!["on_success", "on_fail", "loop", "on_success", "on_pass"],
        "unexpected transition order"
    );
}

// ── Shape 4: Full ordered event-type sequence assertions ──────────────────────

/// For a two-state agent machine, the complete ordered sequence of structured
/// event types emitted must be exactly:
///   startup -> state_entered -> step_executed -> transition_selected ->
///   state_entered -> shutdown
///
/// This verifies the event pipeline ordering that downstream log consumers
/// and audit tools must rely on.
#[test]
fn straight_line_full_event_type_sequence_is_ordered() {
    let yaml = r#"
initial_state: task
states:
  - name: task
    action: agent
    on_success: end
    on_failure: end
  - name: end
    action: terminal
"#;
    let sm = load_and_validate(yaml, "<test>").unwrap();
    let executor = ScriptedExecutor::new(vec![AgentOutcome::Success], vec![]);
    let w = CaptureWriter::new();
    let logger = make_logger(w.clone());

    let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

    assert_eq!(
        result,
        ExitReason::Terminal {
            state: "end".to_string()
        }
    );

    let output = w.contents();
    let events = extract_field_sequence(&output, "event");

    assert_eq!(
        events,
        vec![
            "startup",
            "state_entered",
            "step_executed",
            "transition_selected",
            "state_entered",
            "shutdown",
        ],
        "full event sequence mismatch: {events:?}"
    );
}

/// For a two-state builtin machine, the complete ordered event sequence must be:
///   startup -> state_entered -> step_executed -> transition_selected ->
///   state_entered -> shutdown
#[test]
fn builtin_gate_full_event_type_sequence_is_ordered() {
    let yaml = r#"
initial_state: check
states:
  - name: check
    action: builtin
    builtin: builtin.git.is_main_compatible
    on_pass: done
    on_fail: done
  - name: done
    action: terminal
"#;
    let sm = load_and_validate(yaml, "<test>").unwrap();
    let executor = ScriptedExecutor::new(vec![], vec![BuiltinOutcome::Pass]);
    let w = CaptureWriter::new();
    let logger = make_logger(w.clone());

    let result = HeadlessSmDriver::new(&sm).run(&executor, &logger, None);

    assert_eq!(
        result,
        ExitReason::Terminal {
            state: "done".to_string()
        }
    );

    let output = w.contents();
    let events = extract_field_sequence(&output, "event");

    assert_eq!(
        events,
        vec![
            "startup",
            "state_entered",
            "step_executed",
            "transition_selected",
            "state_entered",
            "shutdown",
        ],
        "builtin full event sequence mismatch: {events:?}"
    );
}
