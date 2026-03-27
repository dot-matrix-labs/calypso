use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};

use nightshift_core::claude::{
    ClaudeConfig, ClaudeOutcome, ClaudeSession, SessionContext, parse_clarification, parse_outcome,
};

// ── Test helpers ──────────────────────────────────────────────────────────────

static PATH_MUTEX: OnceLock<Mutex<()>> = OnceLock::new();

fn path_mutex() -> &'static Mutex<()> {
    PATH_MUTEX.get_or_init(|| Mutex::new(()))
}

fn unique_temp_dir(prefix: &str) -> PathBuf {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let path = std::env::temp_dir().join(format!("{prefix}-{ts}"));
    fs::create_dir_all(&path).expect("temp dir should be created");
    path
}

fn write_fake_script(dir: &Path, name: &str, contents: &str) -> PathBuf {
    let script = dir.join(name);
    fs::write(&script, contents).expect("fake script should be written");

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&script)
            .expect("script metadata should exist")
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&script, perms).expect("script should be made executable");
    }

    script
}

fn with_fake_claude<T>(script_body: &str, test_fn: impl FnOnce(ClaudeConfig) -> T) -> T {
    let _guard = path_mutex()
        .lock()
        .expect("PATH mutex should not be poisoned");

    let dir = unique_temp_dir("calypso-claude-test");
    write_fake_script(&dir, "claude", script_body);

    let old_path = std::env::var_os("PATH");
    let mut parts = vec![dir.clone()];
    if let Some(existing) = old_path.as_ref() {
        parts.extend(std::env::split_paths(existing));
    }
    let new_path = std::env::join_paths(parts).expect("PATH should join");
    unsafe { std::env::set_var("PATH", &new_path) };

    let config = ClaudeConfig {
        binary: dir.join("claude").to_string_lossy().into_owned(),
        default_flags: vec![],
        auth_env_var: "ANTHROPIC_API_KEY".to_string(),
    };

    let result = test_fn(config);

    match old_path {
        Some(p) => unsafe { std::env::set_var("PATH", p) },
        None => unsafe { std::env::remove_var("PATH") },
    }

    fs::remove_dir_all(dir).expect("temp dir should be cleaned up");
    result
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[test]
fn check_auth_returns_false_for_nonexistent_binary() {
    let config = ClaudeConfig {
        binary: "/nonexistent/path/to/claude".to_string(),
        default_flags: vec![],
        auth_env_var: "ANTHROPIC_API_KEY".to_string(),
    };

    assert!(!ClaudeSession::check_auth(&config));
}

#[test]
fn check_auth_returns_true_when_binary_responds_successfully() {
    with_fake_claude("#!/bin/sh\nexit 0\n", |config| {
        assert!(ClaudeSession::check_auth(&config));
    });
}

#[test]
fn parse_outcome_ok_produces_correct_variant() {
    let output = r#"[CALYPSO:OK]{"summary":"done"}"#;
    let outcome = parse_outcome(output).expect("should parse OK outcome");
    assert_eq!(
        outcome,
        ClaudeOutcome::Ok {
            summary: "done".to_string(),
            artifact_refs: vec![],
            suggested_next_state: None,
        }
    );
}

#[test]
fn parse_outcome_nok_produces_correct_variant() {
    let output = r#"[CALYPSO:NOK]{"summary":"failed","reason":"tests red"}"#;
    let outcome = parse_outcome(output).expect("should parse NOK outcome");
    assert_eq!(
        outcome,
        ClaudeOutcome::Nok {
            summary: "failed".to_string(),
            reason: "tests red".to_string(),
        }
    );
}

#[test]
fn parse_outcome_aborted_produces_correct_variant() {
    let output = r#"[CALYPSO:ABORTED]{"reason":"operator cancelled"}"#;
    let outcome = parse_outcome(output).expect("should parse ABORTED outcome");
    assert_eq!(
        outcome,
        ClaudeOutcome::Aborted {
            reason: "operator cancelled".to_string(),
        }
    );
}

#[test]
fn parse_clarification_extracts_question_and_session_id() {
    let output = "[CALYPSO:CLARIFICATION]What is the ticket number?";
    let req = parse_clarification(output, "claude-session-42").expect("should find clarification");
    assert_eq!(req.question, "What is the ticket number?");
    assert_eq!(req.session_id, "claude-session-42");
}

#[test]
fn parse_outcome_with_no_marker_returns_error() {
    let output = "Some normal output without any calypso marker";
    let err = parse_outcome(output).expect_err("should fail on missing marker");
    assert!(
        err.to_string().contains("no [CALYPSO"),
        "error message should mention missing marker, got: {err}"
    );
}

#[test]
fn transcript_is_written_after_invocation() {
    let script = "#!/bin/sh\nprintf '[CALYPSO:OK]{\"summary\":\"transcript test\"}\\n'\nexit 0\n";

    with_fake_claude(script, |config| {
        let session = ClaudeSession::new(config);
        let context = SessionContext::default();
        let transcript_dir = unique_temp_dir("calypso-claude-transcript");
        let transcript_path = transcript_dir.join("transcript.jsonl");

        let outcome = session
            .invoke("do the thing", &context, Some(&transcript_path))
            .expect("invocation should succeed");

        assert_eq!(
            outcome,
            ClaudeOutcome::Ok {
                summary: "transcript test".to_string(),
                artifact_refs: vec![],
                suggested_next_state: None,
            }
        );

        assert!(transcript_path.exists(), "transcript file should exist");
        let contents = fs::read_to_string(&transcript_path).expect("transcript should be readable");
        assert!(
            contents.contains("do the thing"),
            "transcript should contain the prompt"
        );
        assert!(
            contents.contains("transcript test"),
            "transcript should contain the output"
        );
        assert!(
            !contents.contains("ANTHROPIC_API_KEY"),
            "transcript must not contain the API key env var name as a secret"
        );

        fs::remove_dir_all(transcript_dir).expect("temp dir should be cleaned up");
    });
}

#[test]
fn invoke_without_transcript_path_does_not_create_file() {
    let script = "#!/bin/sh\nprintf '[CALYPSO:OK]{\"summary\":\"no transcript\"}\\n'\nexit 0\n";

    with_fake_claude(script, |config| {
        let session = ClaudeSession::new(config);
        let context = SessionContext::default();

        let outcome = session
            .invoke("hello", &context, None)
            .expect("invocation should succeed");

        assert!(matches!(outcome, ClaudeOutcome::Ok { .. }));
    });
}

#[test]
fn parse_outcome_ok_with_artifact_refs_and_suggested_next_state() {
    let output = r#"[CALYPSO:OK]{"summary":"done","artifact_refs":["file.rs"],"suggested_next_state":"review"}"#;
    let outcome = parse_outcome(output).expect("should parse");
    assert_eq!(
        outcome,
        ClaudeOutcome::Ok {
            summary: "done".to_string(),
            artifact_refs: vec!["file.rs".to_string()],
            suggested_next_state: Some("review".to_string()),
        }
    );
}

#[test]
fn invoke_with_working_directory_succeeds() {
    let script = "#!/bin/sh\nprintf '[CALYPSO:OK]{\"summary\":\"cwd test\"}\\n'\nexit 0\n";

    with_fake_claude(script, |config| {
        let session = ClaudeSession::new(config);
        let context = SessionContext {
            working_directory: Some(std::env::temp_dir().to_string_lossy().into_owned()),
        };

        let outcome = session
            .invoke("task", &context, None)
            .expect("invocation with working_directory should succeed");

        assert!(matches!(outcome, ClaudeOutcome::Ok { .. }));
    });
}
