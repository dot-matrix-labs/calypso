use calypso_cli::error::{CalypsoError, Recoverability, codes, redact, register_secret};

// ---------------------------------------------------------------------------
// Error category constructors — correct codes
// ---------------------------------------------------------------------------

#[test]
fn provider_auth_carries_expected_code() {
    let err = CalypsoError::provider_auth("bad credentials");
    assert_eq!(err.code, codes::PROVIDER_AUTH);
}

#[test]
fn subprocess_spawn_carries_expected_code() {
    let err = CalypsoError::subprocess_spawn("could not exec");
    assert_eq!(err.code, codes::SUBPROCESS_SPAWN);
}

#[test]
fn malformed_provider_output_carries_expected_code() {
    let err = CalypsoError::malformed_provider_output("unexpected json");
    assert_eq!(err.code, codes::MALFORMED_PROVIDER_OUTPUT);
}

#[test]
fn transport_carries_expected_code() {
    let err = CalypsoError::transport("connection reset");
    assert_eq!(err.code, codes::TRANSPORT);
}

#[test]
fn git_carries_expected_code() {
    let err = CalypsoError::git("ref not found");
    assert_eq!(err.code, codes::GIT);
}

#[test]
fn github_api_carries_expected_code() {
    let err = CalypsoError::github_api("rate limited");
    assert_eq!(err.code, codes::GITHUB_API);
}

#[test]
fn invalid_state_transition_carries_expected_code() {
    let err = CalypsoError::invalid_state_transition("cannot go from new to blocked");
    assert_eq!(err.code, codes::INVALID_STATE_TRANSITION);
}

#[test]
fn missing_clarification_carries_expected_code() {
    let err = CalypsoError::missing_clarification("no answer provided");
    assert_eq!(err.code, codes::MISSING_CLARIFICATION);
}

#[test]
fn state_corruption_carries_expected_code() {
    let err = CalypsoError::state_corruption("checksum mismatch");
    assert_eq!(err.code, codes::STATE_CORRUPTION);
}

#[test]
fn studio_lifecycle_carries_expected_code() {
    let err = CalypsoError::studio_lifecycle("studio failed to start");
    assert_eq!(err.code, codes::STUDIO_LIFECYCLE);
}

// ---------------------------------------------------------------------------
// Recoverability classifications
// ---------------------------------------------------------------------------

#[test]
fn provider_auth_is_user_action_required() {
    assert_eq!(
        CalypsoError::provider_auth("x").recoverability,
        Recoverability::UserActionRequired
    );
}

#[test]
fn subprocess_spawn_is_unrecoverable() {
    assert_eq!(
        CalypsoError::subprocess_spawn("x").recoverability,
        Recoverability::Unrecoverable
    );
}

#[test]
fn malformed_provider_output_is_recoverable() {
    assert_eq!(
        CalypsoError::malformed_provider_output("x").recoverability,
        Recoverability::Recoverable
    );
}

#[test]
fn transport_is_recoverable() {
    assert_eq!(
        CalypsoError::transport("x").recoverability,
        Recoverability::Recoverable
    );
}

#[test]
fn git_is_unrecoverable() {
    assert_eq!(
        CalypsoError::git("x").recoverability,
        Recoverability::Unrecoverable
    );
}

#[test]
fn github_api_is_recoverable() {
    assert_eq!(
        CalypsoError::github_api("x").recoverability,
        Recoverability::Recoverable
    );
}

#[test]
fn invalid_state_transition_is_user_action_required() {
    assert_eq!(
        CalypsoError::invalid_state_transition("x").recoverability,
        Recoverability::UserActionRequired
    );
}

#[test]
fn missing_clarification_is_user_action_required() {
    assert_eq!(
        CalypsoError::missing_clarification("x").recoverability,
        Recoverability::UserActionRequired
    );
}

#[test]
fn state_corruption_is_unrecoverable() {
    assert_eq!(
        CalypsoError::state_corruption("x").recoverability,
        Recoverability::Unrecoverable
    );
}

#[test]
fn studio_lifecycle_is_unrecoverable() {
    assert_eq!(
        CalypsoError::studio_lifecycle("x").recoverability,
        Recoverability::Unrecoverable
    );
}

// ---------------------------------------------------------------------------
// Structured JSON serialization
// ---------------------------------------------------------------------------

#[test]
fn json_output_contains_required_fields() {
    let err = CalypsoError::git("branch missing").with_context("branch", "feat/xyz");
    let json = err.to_json();
    let v: serde_json::Value = serde_json::from_str(&json).expect("valid json");

    assert_eq!(v["code"], codes::GIT);
    assert_eq!(v["message"], "branch missing");
    assert!(v.get("recoverability").is_some());
    assert_eq!(v["context"]["branch"], "feat/xyz");
}

#[test]
fn json_recoverability_is_kebab_case() {
    let json = CalypsoError::provider_auth("x").to_json();
    assert!(json.contains("user-action-required"), "json: {json}");

    let json = CalypsoError::transport("x").to_json();
    assert!(json.contains("recoverable"), "json: {json}");

    let json = CalypsoError::git("x").to_json();
    assert!(json.contains("unrecoverable"), "json: {json}");
}

#[test]
fn json_omits_context_when_empty() {
    let json = CalypsoError::transport("x").to_json();
    let v: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        v.get("context").is_none(),
        "context should be omitted when empty"
    );
}

// ---------------------------------------------------------------------------
// redact() — bearer tokens
// ---------------------------------------------------------------------------

#[test]
fn redact_scrubs_bearer_token() {
    let input = "Authorization: Bearer mysecrettoken123";
    let output = redact(input);
    assert!(!output.contains("mysecrettoken123"), "output: {output}");
    assert!(output.contains("Bearer [REDACTED]"), "output: {output}");
}

#[test]
fn redact_scrubs_bearer_token_case_insensitive() {
    let input = "authorization: bearer MYSECRETTOKEN";
    let output = redact(input);
    assert!(!output.contains("MYSECRETTOKEN"), "output: {output}");
}

// ---------------------------------------------------------------------------
// redact() — GitHub PAT patterns
// ---------------------------------------------------------------------------

#[test]
fn redact_scrubs_ghp_token() {
    let input = "token=ghp_abcdefghijklmnopqrstuvwxyz123456";
    let output = redact(input);
    assert!(!output.contains("ghp_"), "output: {output}");
    assert!(output.contains("[REDACTED]"), "output: {output}");
}

#[test]
fn redact_scrubs_github_pat_token() {
    let input = "auth=github_pat_ABCDEFGHIJ1234567890_extra";
    let output = redact(input);
    assert!(!output.contains("github_pat_"), "output: {output}");
    assert!(output.contains("[REDACTED]"), "output: {output}");
}

// ---------------------------------------------------------------------------
// redact() — generic 40+ char hex secrets
// ---------------------------------------------------------------------------

#[test]
fn redact_scrubs_40_char_hex_string() {
    let hex = "a3f1b2c4d5e6f7089a1b2c3d4e5f607182930a4b";
    assert_eq!(hex.len(), 40);
    let input = format!("token: {hex}");
    let output = redact(&input);
    assert!(!output.contains(hex), "output: {output}");
    assert!(output.contains("[REDACTED]"), "output: {output}");
}

#[test]
fn redact_does_not_scrub_short_hex() {
    // 36 hex chars — below the 40-character threshold, should be left alone.
    let short_hex = "a3f1b2c4d5e6f7089a1b2c3d4e5f60718293";
    assert_eq!(short_hex.len(), 36);
    let input = format!("ref: {short_hex}");
    let output = redact(&input);
    assert!(
        output.contains(short_hex),
        "short hex should not be redacted; output: {output}"
    );
}

// ---------------------------------------------------------------------------
// redact() — normal strings are unchanged
// ---------------------------------------------------------------------------

#[test]
fn redact_leaves_normal_strings_unchanged() {
    let inputs = [
        "hello world",
        "error: file not found",
        "version: 1.2.3",
        "branch: feat/my-feature",
    ];
    for input in inputs {
        assert_eq!(redact(input), input, "input: {input}");
    }
}

// ---------------------------------------------------------------------------
// redact() — registry-based secrets
// ---------------------------------------------------------------------------

#[test]
fn redact_scrubs_registered_secret() {
    register_secret("super_secret_api_key_value_xyz");
    let input = "calling api with super_secret_api_key_value_xyz ok";
    let output = redact(input);
    assert!(
        !output.contains("super_secret_api_key_value_xyz"),
        "output: {output}"
    );
    assert!(output.contains("[REDACTED]"), "output: {output}");
}

#[test]
fn register_secret_ignores_empty_string() {
    // Calling with an empty value must not panic and must not register anything.
    register_secret("");
    // A subsequent redact of a normal string should be unaffected.
    assert_eq!(redact("hello"), "hello");
}

// ---------------------------------------------------------------------------
// Recoverability — Display impl
// ---------------------------------------------------------------------------

#[test]
fn recoverability_display_recoverable() {
    assert_eq!(Recoverability::Recoverable.to_string(), "recoverable");
}

#[test]
fn recoverability_display_user_action_required() {
    assert_eq!(
        Recoverability::UserActionRequired.to_string(),
        "user-action-required"
    );
}

#[test]
fn recoverability_display_unrecoverable() {
    assert_eq!(Recoverability::Unrecoverable.to_string(), "unrecoverable");
}

// ---------------------------------------------------------------------------
// CalypsoError — Display impl
// ---------------------------------------------------------------------------

#[test]
fn calypso_error_display_includes_code_and_message() {
    let err = CalypsoError::git("branch not found");
    let display = err.to_string();
    assert!(display.contains("git"), "display: {display}");
    assert!(display.contains("branch not found"), "display: {display}");
}

// ---------------------------------------------------------------------------
// emit_stderr — smoke test (just ensure it doesn't panic)
// ---------------------------------------------------------------------------

#[test]
fn emit_stderr_does_not_panic() {
    CalypsoError::transport("connection refused").emit_stderr();
}
