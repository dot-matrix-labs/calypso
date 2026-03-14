use calypso_cli::app::{run_doctor, run_status};
use calypso_cli::doctor::{DoctorFix, DoctorStatus, apply_fix, collect_doctor_report};
use calypso_cli::execution::{ExecutionConfig, ExecutionOutcome, run_supervised_session};
use calypso_cli::feature_start::{FeatureStartRequest, run_feature_start};
use calypso_cli::state::RepositoryState;
use calypso_cli::template::TemplateSet;
use calypso_cli::tui::{OperatorSurface, run_terminal_surface, run_watch};
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
        [command, flag, check_id] if command == "doctor" && flag == "--fix" => {
            run_doctor_fix(check_id);
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
        // calypso watch — live TUI from current working directory state file
        [command] if command == "watch" => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            let state_path = cwd.join(".calypso").join("state.json");
            run_watch(&state_path.to_string_lossy());
        }
        // calypso watch --state <path>
        [command, flag, path] if command == "watch" && flag == "--state" => {
            run_watch(path);
        }
        [command, subcommand] if command == "template" && subcommand == "validate" => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            run_template_validate(&cwd);
        }
        // calypso <path> — launch TUI for a specific project directory
        [path] if looks_like_path(path) => {
            let project_dir = std::path::Path::new(path);
            let state_path = project_dir.join(".calypso").join("state.json");
            if state_path.exists() {
                run_watch(&state_path.to_string_lossy());
            } else {
                println!("{}", render_help(info));
            }
        }
        // calypso — no args, launch TUI from current working directory
        [] => {
            let cwd = std::env::current_dir().expect("current directory should resolve");
            let state_path = cwd.join(".calypso").join("state.json");
            if state_path.exists() {
                run_watch(&state_path.to_string_lossy());
            } else {
                println!("{}", render_help(info));
            }
        }
        _ => println!("{}", render_help(info)),
    }
}

fn looks_like_path(arg: &str) -> bool {
    arg.starts_with('.')
        || arg.starts_with('/')
        || arg.starts_with('~')
        || std::path::Path::new(arg).is_dir()
}

fn run_doctor_fix(check_id: &str) {
    let cwd = std::env::current_dir().expect("current directory should resolve");
    let repo_root = calypso_cli::app::resolve_repo_root(&cwd).unwrap_or_else(|| cwd.clone());
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

#[cfg(test)]
mod tests {
    use super::looks_like_path;

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
}
