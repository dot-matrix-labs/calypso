use calypso_cli::app::{run_doctor, run_status};
use calypso_cli::doctor::{DoctorFix, DoctorStatus, apply_fix, collect_doctor_report};
use calypso_cli::execution::{ExecutionConfig, ExecutionOutcome, run_supervised_session};
use calypso_cli::feature_start::{FeatureStartRequest, run_feature_start};
use calypso_cli::init::{HostInitEnvironment, run_init_interactive};
use calypso_cli::state::RepositoryState;
use calypso_cli::template::TemplateSet;
use calypso_cli::tui::{OperatorSurface, run_doctor_surface, run_terminal_surface, run_watch};
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
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // Strip -p / --path <dir> from args before dispatching.
    // This makes --path a global flag that works with every subcommand.
    let (path_override, args) = extract_path_flag(&raw_args);
    let cwd = path_override.unwrap_or_else(|| {
        std::env::current_dir().expect("current directory should resolve")
    });

    match args.as_slice() {
        [flag] if flag == "-h" || flag == "--help" => println!("{}", render_help(info)),
        [flag] if flag == "-v" || flag == "--version" => println!("{}", render_version(info)),
        [command] if command == "doctor" => {
            println!("{}", run_doctor(&cwd));
        }
        [command, flag, check_id] if command == "doctor" && flag == "--fix" => {
            run_doctor_fix(check_id, &cwd);
        }
        [command] if command == "status" => {
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
        [command] if command == "init" => {
            run_calypso_init(&cwd, false);
        }
        [command, flag] if command == "init" && flag == "--reinit" => {
            run_calypso_init(&cwd, true);
        }
        [command, flag] if command == "init" && flag == "--state" => {
            run_init_state_show(&cwd);
        }
        [command, subcommand] if command == "state" && subcommand == "show" => {
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
            let state_path = cwd.join(".calypso/repository-state.json");
            run_claude_session(&state_path.to_string_lossy(), role);
        }
        // calypso watch — live TUI from project directory state file
        [command] if command == "watch" => {
            let state_path = cwd.join(".calypso").join("state.json");
            run_watch(&state_path.to_string_lossy());
        }
        // calypso watch --state <path>
        [command, flag, path] if command == "watch" && flag == "--state" => {
            run_watch(path);
        }
        [command, subcommand] if command == "template" && subcommand == "validate" => {
            run_template_validate(&cwd);
        }
        // calypso <path> — positional project directory (kept for backward compatibility)
        [path] if looks_like_path(path) => {
            let project_dir = std::path::Path::new(path);
            let state_path = project_dir.join(".calypso").join("state.json");
            if state_path.exists() {
                run_state_machine_auto(&state_path);
            } else {
                run_doctor_surface(project_dir).unwrap_or_else(|e| eprintln!("tui error: {e}"));
            }
        }
        // calypso --step — step mode: one step per Enter keypress
        [flag] if flag == "--step" => {
            let state_path = cwd.join(".calypso").join("state.json");
            if state_path.exists() {
                run_state_machine_step(&state_path);
            } else {
                run_doctor_surface(&cwd).unwrap_or_else(|e| eprintln!("tui error: {e}"));
            }
        }
        // calypso — no args: drive state machine if initialized, else show doctor TUI
        [] => {
            let state_path = cwd.join(".calypso").join("state.json");
            if state_path.exists() {
                run_state_machine_auto(&state_path);
            } else {
                run_doctor_surface(&cwd).unwrap_or_else(|e| eprintln!("tui error: {e}"));
            }
        }
        _ => println!("{}", render_help(info)),
    }
}

/// Strip `-p`/`--path <dir>` from `args` and return the path (if present) plus the remaining args.
fn extract_path_flag(args: &[String]) -> (Option<std::path::PathBuf>, Vec<String>) {
    let mut remaining = Vec::new();
    let mut path: Option<std::path::PathBuf> = None;
    let mut i = 0;
    while i < args.len() {
        if (args[i] == "-p" || args[i] == "--path") && i + 1 < args.len() {
            path = Some(std::path::PathBuf::from(&args[i + 1]));
            i += 2;
        } else {
            remaining.push(args[i].clone());
            i += 1;
        }
    }
    (path, remaining)
}

fn run_calypso_init(cwd: &std::path::Path, allow_reinit: bool) {
    match run_init_interactive(cwd, allow_reinit, &HostInitEnvironment) {
        Ok(progress) => {
            println!("Init complete: {}", progress.current_step);
            println!("Completed steps:");
            for step in &progress.completed_steps {
                println!("  [x] {step}");
            }
        }
        Err(error) => {
            eprintln!("init error: {error}");
            std::process::exit(1);
        }
    }
}

fn run_init_state_show(cwd: &std::path::Path) {
    let state_path = cwd.join(".calypso").join("init-state.json");
    match std::fs::read_to_string(&state_path) {
        Ok(contents) => println!("{contents}"),
        Err(_) => {
            println!("No init state found — run `calypso-cli init` to set up this repository.");
        }
    }
}

fn looks_like_path(arg: &str) -> bool {
    arg.starts_with('.')
        || arg.starts_with('/')
        || arg.starts_with('~')
        || std::path::Path::new(arg).is_dir()
}

fn run_doctor_fix(check_id: &str, cwd: &std::path::Path) {
    let repo_root = calypso_cli::app::resolve_repo_root(cwd).unwrap_or_else(|| cwd.to_path_buf());
    let report = collect_doctor_report(&calypso_cli::doctor::HostDoctorEnvironment, &repo_root);

    let check = report
        .checks
        .iter()
        .find(|check| check.id.label() == check_id);

    match check {
        None => {
            eprintln!("doctor fix: unknown check id '{check_id}'");
            std::process::exit(1);
        }
        Some(check) => {
            if check.status == DoctorStatus::Passing {
                println!("Check '{check_id}' is already passing — no fix needed.");
                return;
            }
            match &check.fix {
                None => {
                    eprintln!("No fix available for '{check_id}'.");
                    std::process::exit(1);
                }
                Some(fix) => match apply_fix(fix) {
                    Ok(output) => {
                        if matches!(fix, DoctorFix::Manual { .. }) {
                            println!("Manual fix required:");
                            println!("{output}");
                        } else {
                            println!("Fix applied successfully:");
                            println!("{output}");
                        }
                    }
                    Err(error) => {
                        eprintln!("Fix failed: {error}");
                        std::process::exit(1);
                    }
                },
            }
        }
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
    let config = ExecutionConfig::default();

    match run_supervised_session(std::path::Path::new(state_path), role, &config) {
        Err(err) => {
            eprintln!("execution error: {err}");
            std::process::exit(1);
        }
        Ok(outcome) => match outcome {
            ExecutionOutcome::Ok {
                summary,
                artifact_refs,
                advanced_to,
            } => {
                println!("Outcome: OK");
                println!("Summary: {summary}");
                if !artifact_refs.is_empty() {
                    println!("Artifacts: {}", artifact_refs.join(", "));
                }
                if let Some(next) = advanced_to {
                    println!("State advanced to: {}", next.as_str());
                }
            }
            ExecutionOutcome::Nok { summary, reason } => {
                println!("Outcome: NOK");
                println!("Summary: {summary}");
                println!("Reason: {reason}");
                eprintln!("Session NOK: {reason}");
                std::process::exit(1);
            }
            ExecutionOutcome::Aborted { reason } => {
                println!("Outcome: ABORTED");
                println!("Reason: {reason}");
                std::process::exit(3);
            }
            ExecutionOutcome::ClarificationRequired(req) => {
                println!("Outcome: CLARIFICATION");
                println!("Question: {}", req.question);
                eprintln!("Operator input required: {}", req.question);
                std::process::exit(2);
            }
            ExecutionOutcome::ProviderFailure { detail } => {
                eprintln!("Provider failure: {detail}");
                std::process::exit(1);
            }
        },
    }
}

fn run_state_machine_auto(state_path: &std::path::Path) {
    use calypso_cli::driver::{DriverMode, DriverStepResult, StateMachineDriver};
    use calypso_cli::execution::ExecutionConfig;
    use calypso_cli::template::load_embedded_template_set;

    let template = load_embedded_template_set().expect("embedded templates should be valid");
    let driver = StateMachineDriver {
        mode: DriverMode::Auto,
        state_path: state_path.to_path_buf(),
        template,
        config: ExecutionConfig::default(),
    };

    let results = driver.run_auto();
    for result in &results {
        match result {
            DriverStepResult::Advanced(state) => {
                println!("→ {}", state.as_str());
            }
            DriverStepResult::Terminal => {
                println!("done");
            }
            DriverStepResult::Unchanged => {
                println!("unchanged");
            }
            DriverStepResult::ClarificationRequired(q) => {
                println!("clarification required: {q}");
                eprintln!("operator input required: {q}");
                std::process::exit(2);
            }
            DriverStepResult::Failed { reason } => {
                eprintln!("step failed: {reason}");
                std::process::exit(1);
            }
            DriverStepResult::Error(e) => {
                eprintln!("driver error: {e}");
                std::process::exit(1);
            }
        }
    }
}

fn run_state_machine_step(state_path: &std::path::Path) {
    use calypso_cli::driver::{DriverMode, DriverStepResult, StateMachineDriver};
    use calypso_cli::execution::ExecutionConfig;
    use calypso_cli::state::RepositoryState;
    use calypso_cli::template::load_embedded_template_set;

    let template = load_embedded_template_set().expect("embedded templates should be valid");
    let driver = StateMachineDriver {
        mode: DriverMode::Step,
        state_path: state_path.to_path_buf(),
        template,
        config: ExecutionConfig::default(),
    };

    loop {
        match RepositoryState::load_from_path(state_path) {
            Ok(state) => {
                let current = state.current_feature.workflow_state.as_str();
                println!("state: {current} — press Enter to step, q to quit");
            }
            Err(e) => {
                eprintln!("error loading state: {e}");
                std::process::exit(1);
            }
        }

        let mut input = String::new();
        std::io::stdin().read_line(&mut input).ok();
        let trimmed = input.trim();
        if trimmed == "q" || trimmed == "quit" {
            break;
        }

        match driver.step() {
            DriverStepResult::Advanced(state) => {
                println!("→ advanced to: {}", state.as_str());
            }
            DriverStepResult::Terminal => {
                println!("done");
                break;
            }
            DriverStepResult::Unchanged => {
                println!("step complete (state unchanged)");
            }
            DriverStepResult::ClarificationRequired(q) => {
                println!("clarification required: {q}");
            }
            DriverStepResult::Failed { reason } => {
                println!("step failed: {reason}");
                println!("press Enter to retry, q to quit");
            }
            DriverStepResult::Error(e) => {
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_path_flag, looks_like_path};

    #[test]
    fn looks_like_path_recognises_dot_relative() {
        assert!(looks_like_path("./my-project"));
        assert!(looks_like_path("../sibling"));
        assert!(looks_like_path("."));
    }

    #[test]
    fn looks_like_path_recognises_absolute() {
        assert!(looks_like_path("/home/user/project"));
        assert!(looks_like_path("/tmp"));
    }

    #[test]
    fn looks_like_path_recognises_tilde() {
        assert!(looks_like_path("~/projects/calypso"));
    }

    #[test]
    fn looks_like_path_rejects_subcommands() {
        assert!(!looks_like_path("doctor"));
        assert!(!looks_like_path("status"));
        assert!(!looks_like_path("watch"));
        assert!(!looks_like_path("--version"));
        assert!(!looks_like_path("-v"));
    }

    #[test]
    fn looks_like_path_accepts_existing_directory() {
        let tmp = std::env::temp_dir();
        assert!(looks_like_path(
            tmp.to_str().expect("temp dir should be valid utf-8")
        ));
    }

    #[test]
    fn extract_path_flag_strips_short_flag() {
        let args = s(&["-p", "/my/project", "doctor"]);
        let (path, remaining) = extract_path_flag(&args);
        assert_eq!(path, Some(std::path::PathBuf::from("/my/project")));
        assert_eq!(remaining, s(&["doctor"]));
    }

    #[test]
    fn extract_path_flag_strips_long_flag() {
        let args = s(&["--path", "/my/project", "--step"]);
        let (path, remaining) = extract_path_flag(&args);
        assert_eq!(path, Some(std::path::PathBuf::from("/my/project")));
        assert_eq!(remaining, s(&["--step"]));
    }

    #[test]
    fn extract_path_flag_flag_at_end_is_ignored() {
        // -p with no following argument — not consumed
        let args = s(&["doctor", "-p"]);
        let (path, remaining) = extract_path_flag(&args);
        assert!(path.is_none());
        assert_eq!(remaining, s(&["doctor", "-p"]));
    }

    #[test]
    fn extract_path_flag_returns_none_when_absent() {
        let args = s(&["doctor"]);
        let (path, remaining) = extract_path_flag(&args);
        assert!(path.is_none());
        assert_eq!(remaining, s(&["doctor"]));
    }

    #[test]
    fn extract_path_flag_works_with_empty_args() {
        let (path, remaining) = extract_path_flag(&[]);
        assert!(path.is_none());
        assert!(remaining.is_empty());
    }

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }
}
