# Test Review Agent Prompt

## Context

You are auditing the test suite for the `calypso` Rust CLI codebase at `/home/lucas/calypso/cli/`.

Your job is **not** to fix anything — only to report. Produce a clear, actionable findings report that a developer can use to prioritise which tests to rewrite or delete.

The test suite was written under a 90%–99% coverage gate. Some tests may have been written primarily to hit coverage numbers rather than to catch real regressions. You are looking for those tests.

---

## What to look for

### 1. Tests that only assert that code does not panic

```rust
// Red flag — this proves almost nothing
fn test_foo() {
    let _ = Foo::new();
}
```

A test with no `assert!`, `assert_eq!`, `assert_matches!`, or `?`-propagation on a meaningful result is not testing behaviour — it is testing that the constructor does not immediately crash.

### 2. Tests that assert trivially true things

```rust
assert_eq!(result.is_ok(), true);   // instead of: result.unwrap() or assert!(result.is_ok())
assert!(vec.len() >= 0);            // always true
assert_eq!(s, s);                   // tautology
```

### 3. Tests that reproduce the implementation

A test that constructs a struct, calls one method, and then checks that the struct now has the value that was just passed in:

```rust
let mut state = State::new();
state.set_name("foo");
assert_eq!(state.name, "foo");
```

This is a getter/setter test. It proves the struct stores values, not that any logic works.

### 4. Tests added specifically to cover `Display`, `Debug`, `Default`, or `from_str` with no edge cases

Formatting traits almost never contain bugs. A test that calls `format!("{}", x)` and checks it is non-empty — or calls `SomeEnum::default()` and checks it equals the first variant — adds coverage numbers without value.

Look especially at the commit that lowered or raised the coverage gate (PR #62 and the restore PR) for tests added at the last minute to hit a threshold.

### 5. Tests that mock away all the interesting behaviour

A test that replaces every dependency with a no-op mock and then asserts that the orchestration code calls those mocks in order is testing the test setup, not the logic. Look for tests that:

- Construct a fake/stub of the entire environment
- Assert only that certain functions were called (call-counting assertions), not what they produced
- Would pass even if the production logic were replaced with `return Ok(())`

### 6. `#[ignore]` tests and `todo!()` test bodies

Tests marked `#[ignore]` that have been ignored for longer than the duration of a single sprint are probably dead. Tests whose body is `todo!()` or `unimplemented!()` are placeholders that inflate the test count.

### 7. Duplicate tests under different names

Two tests that set up identical fixtures and assert identical things, just named differently — one of them should be deleted.

### 8. Tests that only exercise error formatting

```rust
let err = CalypsoError::NotFound("x".to_string());
assert!(format!("{}", err).contains("x"));
```

Error Display implementations rarely break. These tests add lines to the coverage report cheaply.

---

## Files to audit

Read every file in:
- `/home/lucas/calypso/cli/tests/`
- Test modules (`#[cfg(test)]` blocks) inside `/home/lucas/calypso/cli/src/*.rs`

Pay particular attention to:
- `state.rs` tests (largest file, written under coverage pressure)
- `telemetry.rs` tests
- `state_v2.rs` and `state_release.rs` (added late in Phase 1)
- Any test added in commits that mention "coverage" or "threshold" in their message

---

## Report format

Produce a structured findings report. For each finding:

```
### <file>:<test_name>

**Category:** <one of the categories above>
**Why it's weak:** <one or two sentences>
**Recommendation:** Delete | Rewrite with real assertions | Keep (explain why)
```

At the end, include a summary table:

| File | Weak tests found | Total tests (approx) | Action |
|------|-----------------|----------------------|--------|
| ...  | ...             | ...                  | ...    |

And a **Priority list**: the 5–10 tests most worth rewriting first, ranked by the risk that a real bug would slip past them.

---

## Rules

- Do NOT make any code changes.
- Do NOT create any files (other than your report output).
- Be sceptical but fair: a test that looks trivial may be the only thing catching a subtle serialization edge case — note that context.
- If you are unsure whether a test is meaningful, say so rather than guessing.
