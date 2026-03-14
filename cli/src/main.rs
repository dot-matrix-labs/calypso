use calypso_cli::app::{run_doctor, run_status};
use calypso_cli::claude::{
    ClaudeConfig, ClaudeOutcome, ClaudeSession, SessionContext, parse_clarification,
};
use calypso_cli::feature_start::{FeatureStartRequest, run_feature_start};
use calypso_cli::state::{RepositoryState, TransitionFacts, WorkflowState};
use calypso_cli::template::TemplateSet;
use calypso_cli::tui::{OperatorSurface, run_terminal_surface};
use calypso_cli::{BuildInfo, render_help, render_version};

fn build_info() -> BuildInfo<'static> {
    const VERSION: &str = concat!(
        env!("CARGO_PKG_VERSION"),
        "+",
        env!("CALYPSO_BUILD_GIT_HASH")
    );

    BuildInfo {
        version: VERSION,
        git_hash: env!("CALYPSO_BUILD_GIT_HASH"),
        build_time: env!("CALYPSO_BUILD_TIME"),
        git_tags: env!("CALYPSO_BUILD_GIT_TAGS"),
    }
}

fn main() {
    let info = build_info();
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.as_slice() {
        [flag] if flag == "-v" || flag == "--version" => println!("{}", render_version(info)),
        [command] if command == "doctor" => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            println!("{}", run_doctor(&cwd));
        }
        [command] if command == "status" => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            match run_status(&cwd) {
                Ok(output) => println!("{output}"),
                Err(error) => {
                    eprintln!("status error: {error}");
                    std::process::exit(1);
                }
            }
        }
        [command, flag, path, headless]
            if command == "status" && flag == "--state" && headless == "--headless" =>
        {
            render_status(path)
        }
        [command, flag, path] if command == "status" && flag == "--state" => run_status_tui(path),
        [command, subcommand] if command == "state" && subcommand == "show" => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            let state_path = cwd.join(".calypso").join("state.json");
            match RepositoryState::load_from_path(&state_path) {
                Ok(state) => println!(
                    "{}",
                    state.to_json_pretty().expect("state should serialize")
                ),
                Err(error) => {
                    eprintln!("state show error: {error}");
                    std::process::exit(1);
                }
            }
        }
        [command, feature_id, flag, worktree_base]
            if command == "feature-start" && flag == "--worktree-base" =>
        {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            let request = FeatureStartRequest {
                feature_id: feature_id.to_string(),
                worktree_base: std::path::PathBuf::from(worktree_base),
                title: None,
                body: None,
                allow_dirty: false,
                allow_non_main: false,
            };

            match run_feature_start(&cwd, &request) {
                Ok(result) => {
                    println!("Feature started");
                    println!("Branch: {}", result.branch);
                    println!("Worktree: {}", result.worktree_path.display());
                    println!(
                        "Pull request: #{} {}",
                        result.pull_request.number, result.pull_request.url
                    );
                    println!("State: {}", result.state_path.display());
                }
                Err(error) => {
                    eprintln!("feature-start error: {error}");
                    std::process::exit(1);
                }
            }
        }
        // calypso run <feature-id> --role <role>
        [command, _feature_id, role_flag, role] if command == "run" && role_flag == "--role" => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            let state_path = cwd.join(".calypso/repository-state.json");
            run_claude_session(&state_path.to_string_lossy(), role);
        }
        [command, subcommand] if command == "template" && subcommand == "validate" => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            run_template_validate(&cwd);
        }
        _ => println!("{}", render_help(info)),
    }
}

fn run_template_validate(cwd: &std::path::Path) {
    match TemplateSet::load_from_directory(cwd) {
        Ok(template_set) => {
            let errors = template_set.validate_coherence();
            if errors.is_empty() {
                println!("OK");
            } else {
                for error in &errors {
                    eprintln!("coherence error: {error}");
                }
                std::process::exit(1);
            }
        }
        Err(error) => {
            eprintln!("template error: {error}");
            std::process::exit(1);
        }
    }
}

fn render_status(path: &str) {
    let state = RepositoryState::load_from_path(std::path::Path::new(path))
        .expect("status state file should load");
    let surface = OperatorSurface::from_feature_state(&state.current_feature);
    println!("{}", surface.render());
}

fn run_status_tui(path: &str) {
    run_status_tui_with(path, run_terminal_surface).expect("status tui should complete");
}

fn run_status_tui_with<Runner>(path: &str, runner: Runner) -> Result<(), String>
where
    Runner: FnOnce(&mut calypso_cli::state::FeatureState) -> std::io::Result<()>,
{
    let mut state = RepositoryState::load_from_path(std::path::Path::new(path))
        .map_err(|error| error.to_string())?;
    runner(&mut state.current_feature).map_err(|error| error.to_string())?;
    state
        .save_to_path(std::path::Path::new(path))
        .map_err(|error| error.to_string())
}

fn run_claude_session(state_path: &str, role: &str) {
    let mut state = RepositoryState::load_from_path(std::path::Path::new(state_path))
        .expect("state file should load");

    let prompt = format!(
        "You are acting as the `{role}` agent for feature `{}`.\n\
         Current workflow state: {:?}\n\
         Complete your role tasks and emit a [CALYPSO:OK], [CALYPSO:NOK], [CALYPSO:CLARIFICATION], or [CALYPSO:ABORTED] outcome marker.",
        state.current_feature.feature_id, state.current_feature.workflow_state,
    );

    let config = ClaudeConfig::default();
    let session = ClaudeSession::new(config.clone());
    let context = SessionContext {
        working_directory: Some(state.current_feature.worktree_path.clone()),
    };

    let transcript_path = std::path::Path::new(state_path)
        .parent()
        .map(|p| p.join(format!("claude-transcript-{}.jsonl", session.session_id)));

    // First, capture raw output so we can detect CLARIFICATION markers that
    // are not terminal outcomes.
    let raw_stdout = capture_raw_claude_output(&config, &prompt, &context);

    if let Some(ref raw) = raw_stdout {
        // Check for clarification before attempting full outcome parse.
        if let Some(clarification) = parse_clarification(raw, &session.session_id) {
            println!("Outcome: CLARIFICATION");
            println!("Question: {}", clarification.question);
            eprintln!("Operator input required: {}", clarification.question);
            std::process::exit(2);
        }
    }

    let outcome = session
        .invoke(&prompt, &context, transcript_path.as_deref())
        .unwrap_or_else(|error| {
            eprintln!("claude invocation error: {error}");
            std::process::exit(1);
        });

    match &outcome {
        ClaudeOutcome::Ok {
            summary,
            artifact_refs,
            suggested_next_state,
        } => {
            println!("Outcome: OK");
            println!("Summary: {summary}");
            if !artifact_refs.is_empty() {
                println!("Artifacts: {}", artifact_refs.join(", "));
            }
            if let Some(next) = suggested_next_state {
                println!("Suggested next state: {next}");
            }

            // Advance workflow state to the first valid forward state.
            // When Claude reports OK, we treat all forward-progress facts as
            // satisfied so the appropriate transition can be selected regardless
            // of which state we are currently in.
            let facts = TransitionFacts {
                stage_complete: true,
                ready_for_review: true,
                feature_binding_complete: true,
                ..Default::default()
            };
            let valid = state.current_feature.workflow_state.valid_next_states();
            if let Some(next_state) = valid
                .iter()
                .find(|s| !matches!(s, WorkflowState::Blocked | WorkflowState::Aborted))
                .cloned()
            {
                if let Err(err) = state
                    .current_feature
                    .transition_to(next_state.clone(), &facts)
                {
                    eprintln!("state transition error: {err}");
                    // Non-fatal: outcome was OK, just couldn't advance state
                } else {
                    state
                        .save_to_path(std::path::Path::new(state_path))
                        .unwrap_or_else(|err| eprintln!("state save error: {err}"));
                    println!(
                        "State: {} -> {}",
                        state.current_feature.workflow_state.as_str(),
                        next_state.as_str()
                    );
                }
            }
        }
        ClaudeOutcome::Nok { summary, reason } => {
            println!("Outcome: NOK");
            println!("Summary: {summary}");
            println!("Reason: {reason}");
            // State file unchanged — do not save.
            eprintln!("Session NOK: {reason}");
            std::process::exit(1);
        }
        ClaudeOutcome::Aborted { reason } => {
            println!("Outcome: ABORTED");
            println!("Reason: {reason}");

            // Transition to Aborted state.
            let facts = TransitionFacts {
                aborted: true,
                ..Default::default()
            };
            if let Err(err) = state
                .current_feature
                .transition_to(WorkflowState::Aborted, &facts)
            {
                eprintln!("state transition error: {err}");
            } else {
                state
                    .save_to_path(std::path::Path::new(state_path))
                    .unwrap_or_else(|err| eprintln!("state save error: {err}"));
            }
            std::process::exit(3);
        }
    }
}

/// Invoke `claude` and capture the raw stdout for clarification detection.
///
/// This is a lightweight pre-check before the full `ClaudeSession::invoke` path.
/// Returns `None` if the binary cannot be spawned or produces non-UTF-8 output.
fn capture_raw_claude_output(
    config: &ClaudeConfig,
    prompt: &str,
    context: &SessionContext,
) -> Option<String> {
    let mut cmd = std::process::Command::new(&config.binary);
    for flag in &config.default_flags {
        cmd.arg(flag);
    }
    cmd.arg(prompt);
    if let Some(dir) = &context.working_directory {
        cmd.current_dir(dir);
    }
    let output = cmd.output().ok()?;
    String::from_utf8(output.stdout).ok()
}
