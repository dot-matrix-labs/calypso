use calypso_cli::app::{run_doctor, run_status};
use calypso_cli::claude::{ClaudeConfig, ClaudeOutcome, ClaudeSession, SessionContext};
use calypso_cli::feature_start::{FeatureStartRequest, run_feature_start};
use calypso_cli::init::{InitRequest, run_init};
use calypso_cli::pr_checklist::update_pr_body;
use calypso_cli::state::RepositoryState;
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
        [flag] if flag == "-v" || flag == "-V" || flag == "--version" => {
            println!("{}", render_version(info))
        }
        [flag] if flag == "-h" || flag == "--help" => println!("{}", render_help(info)),
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
        // calypso init
        [command] if command == "init" => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            run_init_command(&cwd, None, false);
        }
        // calypso init --allow-reinit
        [command, flag] if command == "init" && flag == "--allow-reinit" => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            run_init_command(&cwd, None, true);
        }
        // calypso init --provider <name>
        [command, flag, provider] if command == "init" && flag == "--provider" => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            run_init_command(&cwd, Some(provider.as_str()), false);
        }
        // calypso init --provider <name> --allow-reinit
        [command, flag, provider, reinit_flag]
            if command == "init" && flag == "--provider" && reinit_flag == "--allow-reinit" =>
        {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            run_init_command(&cwd, Some(provider.as_str()), true);
        }
        [command, subcommand] if command == "sync-pr" && subcommand == "--state" => {
            // Require --state flag: sync-pr --state <path>
            println!("{}", render_help(info));
        }
        [command, flag, path] if command == "sync-pr" && flag == "--state" => {
            run_sync_pr(path);
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
    let state = RepositoryState::load_from_path(std::path::Path::new(state_path))
        .expect("state file should load");

    let prompt = format!(
        "You are acting as the `{role}` agent for feature `{}`.\n\
         Current workflow state: {:?}\n\
         Complete your role tasks and emit a [CALYPSO:OK], [CALYPSO:NOK], or [CALYPSO:ABORTED] outcome marker.",
        state.current_feature.feature_id, state.current_feature.workflow_state,
    );

    let config = ClaudeConfig::default();
    let session = ClaudeSession::new(config);
    let context = SessionContext {
        working_directory: Some(state.current_feature.worktree_path.clone()),
    };

    let transcript_path = std::path::Path::new(state_path)
        .parent()
        .map(|p| p.join(format!("claude-transcript-{}.jsonl", session.session_id)));

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
        }
        ClaudeOutcome::Nok { summary, reason } => {
            println!("Outcome: NOK");
            println!("Summary: {summary}");
            println!("Reason: {reason}");
        }
        ClaudeOutcome::Aborted { reason } => {
            println!("Outcome: ABORTED");
            println!("Reason: {reason}");
        }
    }
}

fn run_init_command(cwd: &std::path::Path, provider: Option<&str>, allow_reinit: bool) {
    let request = InitRequest {
        repo_path: cwd.to_path_buf(),
        provider: provider.map(str::to_string),
        allow_reinit,
    };
    match run_init(&request) {
        Ok(_) => println!("Repository initialised."),
        Err(error) => {
            eprintln!("init error: {error}");
            std::process::exit(1);
        }
    }
}

fn run_sync_pr(state_path: &str) {
    let path = std::path::Path::new(state_path);
    let state = match RepositoryState::load_from_path(path) {
        Ok(s) => s,
        Err(error) => {
            eprintln!("sync-pr error: {error}");
            std::process::exit(1);
        }
    };

    let worktree_path = std::path::Path::new(&state.current_feature.worktree_path);
    let template = match TemplateSet::load_from_directory(worktree_path) {
        Ok(t) => t,
        Err(error) => {
            eprintln!("sync-pr template error: {error}");
            std::process::exit(1);
        }
    };

    let existing_body = std::process::Command::new("gh")
        .args([
            "pr",
            "view",
            &state.current_feature.pull_request.number.to_string(),
            "--json",
            "body",
            "--jq",
            ".body",
        ])
        .current_dir(worktree_path)
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();

    let updated = update_pr_body(
        &existing_body,
        &state.current_feature.gate_groups,
        &template,
    );

    let result = std::process::Command::new("gh")
        .args([
            "pr",
            "edit",
            &state.current_feature.pull_request.number.to_string(),
            "--body",
            &updated,
        ])
        .current_dir(worktree_path)
        .output();

    match result {
        Ok(output) if output.status.success() => println!("PR body updated."),
        Ok(output) => {
            eprintln!(
                "sync-pr gh error: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            );
            std::process::exit(1);
        }
        Err(error) => {
            eprintln!("sync-pr error: {error}");
            std::process::exit(1);
        }
    }
}
