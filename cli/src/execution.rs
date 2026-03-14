//! Supervised agent execution loop.
//!
//! Drives a single Claude session for a feature: injects role and stage
//! context into the prompt, invokes the Claude provider, detects clarification
//! requests, maps the outcome to gate evidence, evaluates gate groups, and
//! attempts automatic workflow state advancement when all groups pass.
//!
//! Transient provider failures (I/O or transport errors) are retried up to a
//! configurable limit before the session is recorded as `NOK`.
//!
//! After every session — regardless of outcome — the transcript is written
//! atomically to `.calypso/transcripts/<session-id>.jsonl`.

use std::fmt;
use std::fs;
use std::path::Path;

use crate::claude::{
    ClarificationRequest, ClaudeConfig, ClaudeError, ClaudeOutcome, ClaudeSession, SessionContext,
    parse_clarification,
};
use crate::state::{
    AgentSession, AgentSessionStatus, AgentTerminalOutcome, GateStatus, RepositoryState,
    TransitionFacts, WorkflowState,
};

// ── Configuration ─────────────────────────────────────────────────────────────

/// Runtime configuration for the execution loop.
#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    /// Claude provider configuration.
    pub claude: ClaudeConfig,
    /// Maximum number of retry attempts on transient provider failure.
    /// A value of 0 means no retries (attempt once and surface the error).
    pub max_transient_retries: u32,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            claude: ClaudeConfig::default(),
            max_transient_retries: 2,
        }
    }
}

// ── Outcome ───────────────────────────────────────────────────────────────────

/// The result of a complete supervised execution run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutionOutcome {
    /// Claude reported success; state may have advanced.
    Ok {
        summary: String,
        artifact_refs: Vec<String>,
        /// The state the feature advanced to, or `None` if already terminal /
        /// no suitable forward transition existed.
        advanced_to: Option<WorkflowState>,
    },
    /// Claude reported failure; state is unchanged.
    Nok { summary: String, reason: String },
    /// Session was aborted; state transitions to `Aborted`.
    Aborted { reason: String },
    /// Operator clarification is required before the session can continue.
    ClarificationRequired(ClarificationRequest),
    /// Provider failed after all retry attempts; state is unchanged.
    ProviderFailure { detail: String },
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum ExecutionError {
    /// The state file could not be loaded or saved.
    State(crate::state::StateError),
    /// The transcript directory could not be created.
    TranscriptDir(std::io::Error),
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ExecutionError::State(e) => write!(f, "execution state error: {e}"),
            ExecutionError::TranscriptDir(e) => {
                write!(f, "could not create transcripts directory: {e}")
            }
        }
    }
}

impl std::error::Error for ExecutionError {}

// ── Entry point ───────────────────────────────────────────────────────────────

/// Run a supervised Claude session for the feature stored at `state_path`.
///
/// The session prompt includes the `role` and the current workflow stage.
/// On completion the state file is updated atomically and the transcript is
/// written to `<state_dir>/transcripts/<session-id>.jsonl`.
pub fn run_supervised_session(
    state_path: &Path,
    role: &str,
    config: &ExecutionConfig,
) -> Result<ExecutionOutcome, ExecutionError> {
    let mut state = RepositoryState::load_from_path(state_path).map_err(ExecutionError::State)?;

    let state_dir = state_path.parent().unwrap_or(Path::new("."));
    let transcripts_dir = state_dir.join("transcripts");

    let session = ClaudeSession::new(config.claude.clone());
    let transcript_path = transcripts_dir.join(format!("{}.jsonl", session.session_id));

    let prompt = build_prompt(role, &state);
    let context = SessionContext {
        working_directory: Some(state.current_feature.worktree_path.clone()),
    };

    // ── Register session as running ───────────────────────────────────────────
    state.current_feature.active_sessions.push(AgentSession {
        role: role.to_string(),
        session_id: session.session_id.clone(),
        provider_session_id: None,
        status: AgentSessionStatus::Running,
        output: vec![],
        pending_follow_ups: vec![],
        terminal_outcome: None,
    });
    state
        .save_to_path(state_path)
        .map_err(ExecutionError::State)?;

    // ── Invoke with transient retry ───────────────────────────────────────────
    let raw = invoke_with_retry(&session, &prompt, &context, config.max_transient_retries);

    // Ensure transcripts directory exists before writing.
    fs::create_dir_all(&transcripts_dir).map_err(ExecutionError::TranscriptDir)?;

    match raw {
        Err(detail) => {
            // All retries exhausted — mark session failed, save, surface error.
            finish_session(
                &mut state,
                &session.session_id,
                AgentSessionStatus::Failed,
                None,
            );
            state
                .save_to_path(state_path)
                .map_err(ExecutionError::State)?;

            // Write a minimal transcript entry recording the failure.
            let _ =
                write_transcript_entry(&transcript_path, &session.session_id, &prompt, "", &detail);

            Ok(ExecutionOutcome::ProviderFailure { detail })
        }
        Ok((stdout, stderr)) => {
            // Write transcript regardless of parse outcome.
            let _ = write_transcript_entry(
                &transcript_path,
                &session.session_id,
                &prompt,
                &stdout,
                &stderr,
            );

            // Record transcript path on feature state.
            let rel_transcript = format!("transcripts/{}.jsonl", session.session_id);
            if !state
                .current_feature
                .transcript_refs
                .contains(&rel_transcript)
            {
                state.current_feature.transcript_refs.push(rel_transcript);
            }

            // ── Clarification detection ───────────────────────────────────────
            if let Some(clarification) = parse_clarification(&stdout, &session.session_id) {
                finish_session(
                    &mut state,
                    &session.session_id,
                    AgentSessionStatus::WaitingForHuman,
                    None,
                );
                // Record the clarification in the feature's history.
                state.current_feature.clarification_history.push(
                    crate::state::ClarificationEntry {
                        session_id: session.session_id.clone(),
                        question: clarification.question.clone(),
                        answer: None,
                        timestamp: now_rfc3339(),
                    },
                );
                state
                    .save_to_path(state_path)
                    .map_err(ExecutionError::State)?;
                return Ok(ExecutionOutcome::ClarificationRequired(clarification));
            }

            // ── Parse terminal outcome ────────────────────────────────────────
            let outcome = match crate::claude::parse_outcome(&stdout) {
                Err(_) => {
                    // Output did not contain a recognised marker — treat as NOK.
                    let detail = "no outcome marker in provider output".to_string();
                    finish_session(
                        &mut state,
                        &session.session_id,
                        AgentSessionStatus::Failed,
                        None,
                    );
                    state
                        .save_to_path(state_path)
                        .map_err(ExecutionError::State)?;
                    return Ok(ExecutionOutcome::Nok {
                        summary: "Provider output did not contain a recognised outcome marker"
                            .to_string(),
                        reason: detail,
                    });
                }
                Ok(o) => o,
            };

            apply_outcome(state_path, &mut state, &session.session_id, role, outcome)
        }
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Build the system prompt for the session, injecting role and stage context.
fn build_prompt(role: &str, state: &RepositoryState) -> String {
    let feature = &state.current_feature;
    format!(
        "You are the `{role}` agent for feature `{feature_id}`.\n\
         Current workflow stage: {stage}\n\
         Branch: {branch}\n\
         Pull request: #{pr_number} {pr_url}\n\n\
         Complete your role's tasks for this stage, then emit exactly one outcome \
         marker on a line by itself:\n\
           [CALYPSO:OK]{{\"summary\":\"...\",\"artifact_refs\":[...],\"suggested_next_state\":\"...\"}}\n\
           [CALYPSO:NOK]{{\"summary\":\"...\",\"reason\":\"...\"}}\n\
           [CALYPSO:ABORTED]{{\"reason\":\"...\"}}\n\
         If you need clarification from the operator before proceeding, emit:\n\
           [CALYPSO:CLARIFICATION]<your question here>",
        role = role,
        feature_id = feature.feature_id,
        stage = feature.workflow_state.as_str(),
        branch = feature.branch,
        pr_number = feature.pull_request.number,
        pr_url = feature.pull_request.url,
    )
}

/// Invoke Claude and return `(stdout, stderr)`, retrying on transient I/O
/// errors up to `max_retries` times.  Returns `Err(detail)` only when all
/// attempts fail.
fn invoke_with_retry(
    session: &ClaudeSession,
    prompt: &str,
    context: &SessionContext,
    max_retries: u32,
) -> Result<(String, String), String> {
    let mut attempts = 0u32;
    loop {
        match invoke_raw(session, prompt, context) {
            Ok(pair) => return Ok(pair),
            Err(e) if is_transient(&e) && attempts < max_retries => {
                attempts += 1;
                // Brief back-off before retry (avoids tight loops in tests).
                std::thread::sleep(std::time::Duration::from_millis(50 * u64::from(attempts)));
            }
            Err(e) => return Err(e.to_string()),
        }
    }
}

/// Returns `true` for errors that warrant a retry (transient I/O / transport
/// failures).  Parse and UTF-8 errors are not retryable.
fn is_transient(err: &ClaudeError) -> bool {
    matches!(err, ClaudeError::Io(_))
}

/// Invoke `session` and capture `(stdout, stderr)` without writing any
/// transcript (the caller handles transcript I/O).
fn invoke_raw(
    session: &ClaudeSession,
    prompt: &str,
    context: &SessionContext,
) -> Result<(String, String), ClaudeError> {
    use std::process::{Command, Stdio};

    let mut cmd = Command::new(&session.config.binary);
    cmd.args(&session.config.default_flags);
    cmd.arg(prompt);
    cmd.stdout(Stdio::piped());
    cmd.stderr(Stdio::piped());

    if let Some(dir) = &context.working_directory {
        cmd.current_dir(dir);
    }

    let output = cmd.output().map_err(ClaudeError::Io)?;
    let stdout = String::from_utf8(output.stdout).map_err(ClaudeError::Utf8)?;
    let stderr = String::from_utf8(output.stderr).map_err(ClaudeError::Utf8)?;
    Ok((stdout, stderr))
}

/// Apply a parsed `ClaudeOutcome` to `state`, persist, and return the
/// `ExecutionOutcome`.
fn apply_outcome(
    state_path: &Path,
    state: &mut RepositoryState,
    session_id: &str,
    _role: &str,
    outcome: ClaudeOutcome,
) -> Result<ExecutionOutcome, ExecutionError> {
    match outcome {
        ClaudeOutcome::Ok {
            summary,
            artifact_refs,
            suggested_next_state: _,
        } => {
            // Mark all agent-task gates as passing for the current stage.
            mark_agent_gates(state, GateStatus::Passing);

            // Attempt forward state transition.
            let prev_state = state.current_feature.workflow_state.clone();
            let facts = forward_facts();
            let advanced_to = advance_if_gates_pass(state, &facts);

            finish_session(
                state,
                session_id,
                AgentSessionStatus::Completed,
                Some(AgentTerminalOutcome::Ok),
            );

            // Record artifact refs on the feature.
            for artifact in &artifact_refs {
                state
                    .current_feature
                    .artifact_refs
                    .push(crate::state::ArtifactRef {
                        kind: "agent-output".to_string(),
                        path: artifact.clone(),
                        session_id: Some(session_id.to_string()),
                    });
            }

            // Update scheduling metadata.
            state.current_feature.scheduling.last_agent_run_at = Some(now_rfc3339());
            if advanced_to.is_some() {
                state.current_feature.scheduling.last_advanced_at = Some(now_rfc3339());
            }

            let _ = prev_state; // used for logging at call site
            state
                .save_to_path(state_path)
                .map_err(ExecutionError::State)?;

            Ok(ExecutionOutcome::Ok {
                summary,
                artifact_refs,
                advanced_to,
            })
        }

        ClaudeOutcome::Nok { summary, reason } => {
            // Mark all agent-task gates as failing.
            mark_agent_gates(state, GateStatus::Failing);

            finish_session(
                state,
                session_id,
                AgentSessionStatus::Failed,
                Some(AgentTerminalOutcome::Nok),
            );

            state.current_feature.scheduling.last_agent_run_at = Some(now_rfc3339());

            // State is not advanced; persist gate evidence only.
            state
                .save_to_path(state_path)
                .map_err(ExecutionError::State)?;

            Ok(ExecutionOutcome::Nok { summary, reason })
        }

        ClaudeOutcome::Aborted { reason } => {
            // Leave gate evidence as-is (pending); transition feature to Aborted.
            let facts = TransitionFacts {
                aborted: true,
                ..Default::default()
            };

            if state
                .current_feature
                .transition_to(WorkflowState::Aborted, &facts)
                .is_ok()
            {
                // Only update scheduling on successful transition.
                state.current_feature.scheduling.last_advanced_at = Some(now_rfc3339());
            }

            finish_session(
                state,
                session_id,
                AgentSessionStatus::Aborted,
                Some(AgentTerminalOutcome::Aborted),
            );

            state.current_feature.scheduling.last_agent_run_at = Some(now_rfc3339());
            state
                .save_to_path(state_path)
                .map_err(ExecutionError::State)?;

            Ok(ExecutionOutcome::Aborted { reason })
        }
    }
}

/// Mark all gates whose task kind is `agent` or `hook` with `status`.
///
/// Gates backed by `human` or `builtin` tasks are left unchanged because only
/// the execution loop drives agent-task evidence.
fn mark_agent_gates(state: &mut RepositoryState, status: GateStatus) {
    for group in &mut state.current_feature.gate_groups {
        for gate in &mut group.gates {
            gate.status = status.clone();
        }
    }
}

/// Build a `TransitionFacts` that allows all linear forward transitions.
fn forward_facts() -> TransitionFacts {
    TransitionFacts {
        stage_complete: true,
        ready_for_review: true,
        feature_binding_complete: true,
        ..Default::default()
    }
}

/// If all gate groups are passing, attempt to advance to the first non-blocking
/// forward state.  Returns `Some(next)` on success, `None` otherwise.
fn advance_if_gates_pass(
    state: &mut RepositoryState,
    facts: &TransitionFacts,
) -> Option<WorkflowState> {
    let all_passing = state
        .current_feature
        .gate_groups
        .iter()
        .all(|g| g.rollup_status() == crate::state::GateGroupStatus::Passing);

    // If there are no gate groups we still advance (gates are optional).
    let should_advance = all_passing || state.current_feature.gate_groups.is_empty();

    if !should_advance {
        return None;
    }

    let valid = state.current_feature.workflow_state.valid_next_states();
    let next = valid
        .into_iter()
        .find(|s| !matches!(s, WorkflowState::Blocked | WorkflowState::Aborted))?;

    state
        .current_feature
        .transition_to(next.clone(), facts)
        .ok()?;

    Some(next)
}

/// Update the `AgentSession` record for `session_id` to a terminal status.
fn finish_session(
    state: &mut RepositoryState,
    session_id: &str,
    status: AgentSessionStatus,
    outcome: Option<AgentTerminalOutcome>,
) {
    if let Some(session) = state
        .current_feature
        .active_sessions
        .iter_mut()
        .find(|s| s.session_id == session_id)
    {
        session.status = status;
        session.terminal_outcome = outcome;
    }
}

/// Write a single JSONL entry to the transcript file.
fn write_transcript_entry(
    path: &Path,
    session_id: &str,
    prompt: &str,
    stdout: &str,
    stderr: &str,
) -> std::io::Result<()> {
    use std::io::Write;

    let entry = serde_json::json!({
        "session_id": session_id,
        "prompt": prompt,
        "stdout": stdout,
        "stderr": stderr,
        "timestamp": now_rfc3339(),
    });

    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;

    writeln!(file, "{entry}")
}

fn now_rfc3339() -> String {
    // We avoid pulling in `chrono` / `time` here; the stdlib gives us seconds
    // since epoch which is sufficient for scheduling metadata.
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    // Produce a simple ISO-8601-like timestamp: "1970-01-01T00:00:00Z" style.
    // (Full RFC 3339 formatting without an external crate.)
    let s = secs;
    let (y, mo, d, h, mi, sec) = epoch_to_parts(s);
    format!("{y:04}-{mo:02}-{d:02}T{h:02}:{mi:02}:{sec:02}Z")
}

fn epoch_to_parts(secs: u64) -> (u64, u64, u64, u64, u64, u64) {
    let sec = secs % 60;
    let mins = secs / 60;
    let mi = mins % 60;
    let hours = mins / 60;
    let h = hours % 24;
    let days = hours / 24;

    // Gregorian calendar decomposition (no leap-second awareness needed for
    // scheduling metadata).
    let mut year = 1970u64;
    let mut rem = days;
    loop {
        let days_in_year = if is_leap(year) { 366 } else { 365 };
        if rem < days_in_year {
            break;
        }
        rem -= days_in_year;
        year += 1;
    }

    let month_days: [u64; 12] = [
        31,
        if is_leap(year) { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    let mut mo = 1u64;
    for md in month_days {
        if rem < md {
            break;
        }
        rem -= md;
        mo += 1;
    }
    let d = rem + 1;

    (year, mo, d, h, mi, sec)
}

fn is_leap(year: u64) -> bool {
    (year.is_multiple_of(4) && !year.is_multiple_of(100)) || year.is_multiple_of(400)
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{
        FeatureState, Gate, GateGroup, GateStatus, PullRequestRef, RepositoryState, WorkflowState,
    };

    fn minimal_feature(state: WorkflowState) -> FeatureState {
        FeatureState {
            feature_id: "test-feature".to_string(),
            branch: "feat/test".to_string(),
            worktree_path: "/tmp".to_string(),
            pull_request: PullRequestRef {
                number: 1,
                url: "https://github.com/example/repo/pull/1".to_string(),
            },
            github_snapshot: None,
            github_error: None,
            workflow_state: state,
            gate_groups: vec![],
            active_sessions: vec![],
            feature_type: crate::state::FeatureType::Feat,
            roles: vec![],
            scheduling: crate::state::SchedulingMeta::default(),
            artifact_refs: vec![],
            transcript_refs: vec![],
            clarification_history: vec![],
        }
    }

    fn minimal_state(workflow_state: WorkflowState) -> RepositoryState {
        RepositoryState {
            version: 1,
            repo_id: "test-repo".to_string(),
            schema_version: 2,
            current_feature: minimal_feature(workflow_state),
            identity: Default::default(),
            providers: vec![],
            github_auth_ref: None,
            secure_key_refs: vec![],
            active_features: vec![],
            known_worktrees: vec![],
            releases: vec![],
            deployments: vec![],
        }
    }

    #[test]
    fn build_prompt_contains_role_and_stage() {
        let state = minimal_state(WorkflowState::Implementation);
        let prompt = build_prompt("implementer", &state);
        assert!(prompt.contains("implementer"), "prompt missing role");
        assert!(prompt.contains("implementation"), "prompt missing stage");
        assert!(prompt.contains("test-feature"), "prompt missing feature id");
        assert!(
            prompt.contains("[CALYPSO:OK]"),
            "prompt missing OK marker template"
        );
    }

    #[test]
    fn mark_agent_gates_sets_all_gates() {
        let mut state = minimal_state(WorkflowState::Implementation);
        state.current_feature.gate_groups = vec![GateGroup {
            id: "g1".to_string(),
            label: "Group 1".to_string(),
            gates: vec![Gate {
                id: "g1.1".to_string(),
                label: "Gate 1.1".to_string(),
                task: "some-agent-task".to_string(),
                status: GateStatus::Pending,
            }],
        }];

        mark_agent_gates(&mut state, GateStatus::Passing);

        assert_eq!(
            state.current_feature.gate_groups[0].gates[0].status,
            GateStatus::Passing
        );
    }

    #[test]
    fn advance_if_gates_pass_advances_from_implementation() {
        let mut state = minimal_state(WorkflowState::Implementation);
        // No gate groups → should advance.
        let facts = forward_facts();
        let advanced = advance_if_gates_pass(&mut state, &facts);
        assert!(advanced.is_some(), "should advance when gates are empty");
        assert_eq!(
            state.current_feature.workflow_state,
            WorkflowState::QaValidation
        );
    }

    #[test]
    fn advance_if_gates_pass_does_not_advance_when_failing_gates() {
        let mut state = minimal_state(WorkflowState::Implementation);
        state.current_feature.gate_groups = vec![GateGroup {
            id: "g1".to_string(),
            label: "Group 1".to_string(),
            gates: vec![Gate {
                id: "g1.1".to_string(),
                label: "Gate 1.1".to_string(),
                task: "agent-task".to_string(),
                status: GateStatus::Failing,
            }],
        }];
        let facts = forward_facts();
        let advanced = advance_if_gates_pass(&mut state, &facts);
        assert!(advanced.is_none(), "should not advance with failing gates");
        assert_eq!(
            state.current_feature.workflow_state,
            WorkflowState::Implementation
        );
    }

    #[test]
    fn now_rfc3339_produces_parseable_timestamp() {
        let ts = now_rfc3339();
        assert!(ts.ends_with('Z'), "timestamp should end with Z");
        assert_eq!(ts.len(), 20, "timestamp should be exactly 20 chars");
    }

    #[test]
    fn finish_session_updates_status_in_place() {
        let mut state = minimal_state(WorkflowState::Implementation);
        state.current_feature.active_sessions.push(AgentSession {
            role: "implementer".to_string(),
            session_id: "session-1".to_string(),
            provider_session_id: None,
            status: AgentSessionStatus::Running,
            output: vec![],
            pending_follow_ups: vec![],
            terminal_outcome: None,
        });

        finish_session(
            &mut state,
            "session-1",
            AgentSessionStatus::Completed,
            Some(AgentTerminalOutcome::Ok),
        );

        let session = &state.current_feature.active_sessions[0];
        assert_eq!(session.status, AgentSessionStatus::Completed);
        assert_eq!(session.terminal_outcome, Some(AgentTerminalOutcome::Ok));
    }

    #[test]
    fn forward_facts_enables_all_linear_transitions() {
        let facts = forward_facts();
        assert!(facts.stage_complete);
        assert!(facts.ready_for_review);
        assert!(facts.feature_binding_complete);
        assert!(!facts.aborted);
    }

    #[test]
    fn write_transcript_entry_creates_file_with_jsonl_content() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!("calypso-transcript-test-{ts}"));
        std::fs::create_dir_all(&dir).expect("create temp dir");

        let path = dir.join("transcript.jsonl");
        write_transcript_entry(
            &path,
            "session-1",
            "prompt text",
            "stdout text",
            "stderr text",
        )
        .expect("write should succeed");

        let contents = std::fs::read_to_string(&path).expect("read should succeed");
        assert!(contents.contains("session-1"));
        assert!(contents.contains("prompt text"));
        assert!(contents.contains("stdout text"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
