use calypso_cli::app::{
    render_fix_results, run_agents_json, run_agents_plain, run_dev_status, run_dev_status_json,
    run_doctor, run_doctor_fix_all, run_doctor_fix_single, run_doctor_json, run_doctor_verbose,
    run_keys_list, run_keys_list_json, run_keys_revoke, run_keys_rotate, run_state_status_json,
    run_state_status_plain, run_status, run_workflows_list, run_workflows_show,
    run_workflows_validate,
};
use calypso_cli::execution::{ExecutionConfig, ExecutionOutcome, run_supervised_session};
use calypso_cli::feature_start::{FeatureStartRequest, run_feature_start};
use calypso_cli::init::{
    HostInitEnvironment, InitProgress, RepoInitStatus, detect_repo_status, refresh_workflows,
    render_init_status, run_init_interactive, run_init_step,
};
use calypso_cli::operator_surface::OperatorSurface;
use calypso_cli::state::RepositoryState;
use calypso_cli::template::TemplateSet;
use calypso_cli::{BuildInfo, render_help, render_version};
use calypso_web::run_webview;

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
    let (path_override, args_after_path) = extract_path_flag(&raw_args);
    let cwd = path_override
        .unwrap_or_else(|| std::env::current_dir().expect("current directory should resolve"));

    // Strip --select-flow from args before dispatching.
    let (select_flow, args_after_select_flow) = extract_select_flow_flag(&args_after_path);

    let args = args_after_select_flow;

    match args.as_slice() {
        [flag] if flag == "-h" || flag == "--help" => println!("{}", render_help(info)),
        [flag] if flag == "-v" || flag == "--version" => println!("{}", render_version(info)),
        [command] if command == "doctor" => {
            println!("{}", run_doctor(&cwd));
        }
        [command, flag] if command == "doctor" && flag == "--verbose" => {
            println!("{}", run_doctor_verbose(&cwd));
        }
        [command, flag] if command == "doctor" && flag == "--json" => match run_doctor_json(&cwd) {
            Ok(json) => println!("{json}"),
            Err(json) => {
                println!("{json}");
                std::process::exit(1);
            }
        },
        [command, flag] if command == "doctor" && flag == "--fix" => {
            let results = run_doctor_fix_all(&cwd);
            println!("{}", render_fix_results(&results));
            let any_failed = results.iter().any(|r| r.validated == Some(false));
            if any_failed {
                std::process::exit(1);
            }
        }
        [command, flag, check_id] if command == "doctor" && flag == "--fix" => {
            run_doctor_fix(check_id, &cwd);
        }
        [command] if command == "status" => match run_status(&cwd) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("status error: {error}");
                std::process::exit(1);
            }
        },
        [command, flag, path] if command == "status" && flag == "--state" => render_status(path),
        [command, flag, path, _headless] if command == "status" && flag == "--state" => {
            render_status(path)
        }
        // calypso dev-status [--json]
        [command] if command == "dev-status" => match run_dev_status(&cwd) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("dev-status error: {error}");
                std::process::exit(1);
            }
        },
        [command, flag] if command == "dev-status" && flag == "--json" => {
            match run_dev_status_json(&cwd) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("dev-status error: {error}");
                    std::process::exit(1);
                }
            }
        }
        [command] if command == "init" => {
            run_calypso_init(&cwd, false, None, None, false);
        }
        [command, flag] if command == "init" && flag == "--reinit" => {
            run_calypso_init(&cwd, true, None, None, false);
        }
        [command, flag] if command == "init" && flag == "--hello-world" => {
            run_calypso_init(&cwd, false, None, None, true);
        }
        [command, flag] if command == "init" && flag == "--json" => {
            run_calypso_init_json(&cwd, false, None, None, false);
        }
        [command, flag] if command == "init" && flag == "--status" => {
            run_init_status(&cwd);
        }
        [command, flag] if command == "init" && flag == "--refresh" => {
            run_init_refresh(&cwd);
        }
        [command, flag, step_name] if command == "init" && flag == "--step" => {
            run_init_step_cmd(&cwd, step_name);
        }
        [command, flag] if command == "init" && flag == "--state" => {
            run_init_state_show(&cwd);
        }
        // calypso init --org <org> --repo <name>
        args if args.first().is_some_and(|c| c == "init") && args.len() >= 2 => {
            let mut allow_reinit = false;
            let mut org: Option<String> = None;
            let mut repo_name: Option<String> = None;
            let mut json = false;
            let mut hello_world = false;
            let mut i = 1;
            while i < args.len() {
                match args[i].as_str() {
                    "--reinit" => allow_reinit = true,
                    "--json" => json = true,
                    "--org" if i + 1 < args.len() => {
                        org = Some(args[i + 1].clone());
                        i += 1;
                    }
                    "--repo" if i + 1 < args.len() => {
                        repo_name = Some(args[i + 1].clone());
                        i += 1;
                    }
                    "--hello-world" => hello_world = true,
                    _ => {}
                }
                i += 1;
            }
            if json {
                run_calypso_init_json(&cwd, allow_reinit, org, repo_name, hello_world);
            } else {
                run_calypso_init(&cwd, allow_reinit, org, repo_name, hello_world);
            }
        }
        // calypso state (no subcommand) — alias for `calypso state status`
        [command] if command == "state" => match run_state_status_plain(&cwd) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("state status error: {error}");
                std::process::exit(1);
            }
        },
        [command, flag] if command == "state" && flag == "--json" => {
            match run_state_status_json(&cwd) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("state status error: {error}");
                    std::process::exit(1);
                }
            }
        }
        [command, subcommand] if command == "state" && subcommand == "show" => {
            let state_path = cwd.join(".calypso").join("repository-state.json");
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
        // calypso state status [--json]
        [command, subcommand] if command == "state" && subcommand == "status" => {
            match run_state_status_plain(&cwd) {
                Ok(output) => println!("{output}"),
                Err(error) => {
                    eprintln!("state status error: {error}");
                    std::process::exit(1);
                }
            }
        }
        [command, subcommand, flag]
            if command == "state" && subcommand == "status" && flag == "--json" =>
        {
            match run_state_status_json(&cwd) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("state status error: {error}");
                    std::process::exit(1);
                }
            }
        }
        // calypso agents [--json]
        [command] if command == "agents" => match run_agents_plain(&cwd) {
            Ok(output) => println!("{output}"),
            Err(error) => {
                eprintln!("agents error: {error}");
                std::process::exit(1);
            }
        },
        [command, flag] if command == "agents" && flag == "--json" => match run_agents_json(&cwd) {
            Ok(json) => println!("{json}"),
            Err(error) => {
                eprintln!("agents error: {error}");
                std::process::exit(1);
            }
        },
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
        // calypso webview
        [command] if command == "webview" => {
            run_webview(&cwd, 7373);
        }
        // calypso webview --port <N>
        [command, flag, port_str] if command == "webview" && flag == "--port" => {
            let port: u16 = port_str.parse().unwrap_or(7373);
            run_webview(&cwd, port);
        }
        [command, subcommand] if command == "template" && subcommand == "validate" => {
            run_template_validate(&cwd);
        }
        // calypso workflows list
        [command, subcommand] if command == "workflows" && subcommand == "list" => {
            println!("{}", run_workflows_list(&cwd));
        }
        // calypso workflows show <name>
        [command, subcommand, name] if command == "workflows" && subcommand == "show" => {
            match run_workflows_show(&cwd, name) {
                Ok(yaml) => print!("{yaml}"),
                Err(error) => {
                    eprintln!("workflows show error: {error}");
                    std::process::exit(1);
                }
            }
        }
        // calypso workflows validate <name>
        [command, subcommand, name] if command == "workflows" && subcommand == "validate" => {
            match run_workflows_validate(&cwd, name) {
                Ok(message) => println!("{message}"),
                Err(error) => {
                    eprintln!("{error}");
                    std::process::exit(1);
                }
            }
        }
        // calypso keys list [--json]
        [command, subcommand] if command == "keys" && subcommand == "list" => {
            match run_keys_list(&cwd) {
                Ok(output) => println!("{output}"),
                Err(error) => {
                    eprintln!("keys list error: {error}");
                    std::process::exit(1);
                }
            }
        }
        [command, subcommand, flag]
            if command == "keys" && subcommand == "list" && flag == "--json" =>
        {
            match run_keys_list_json(&cwd) {
                Ok(json) => println!("{json}"),
                Err(error) => {
                    eprintln!("keys list error: {error}");
                    std::process::exit(1);
                }
            }
        }
        // calypso keys rotate <name>
        [command, subcommand, name] if command == "keys" && subcommand == "rotate" => {
            match run_keys_rotate(&cwd, name) {
                Ok(output) => println!("{output}"),
                Err(error) => {
                    eprintln!("keys rotate error: {error}");
                    std::process::exit(1);
                }
            }
        }
        // calypso keys revoke <name>
        [command, subcommand, name] if command == "keys" && subcommand == "revoke" => {
            match run_keys_revoke(&cwd, name) {
                Ok(output) => println!("{output}"),
                Err(error) => {
                    eprintln!("keys revoke error: {error}");
                    std::process::exit(1);
                }
            }
        }
        // calypso <path> — positional project directory (kept for backward compatibility)
        [path] if looks_like_path(path) => {
            let project_dir = std::path::Path::new(path);
            let state_path = project_dir.join(".calypso").join("repository-state.json");
            let flow = resolve_select_flow(select_flow, project_dir);
            match flow {
                Some(SelectedFlow::Workflow(name)) => {
                    run_workflow_auto(&name, project_dir);
                }
                None => {
                    if state_path.exists() {
                        run_state_machine_auto(&state_path, None);
                    } else {
                        println!("{}", run_doctor(project_dir));
                    }
                }
            }
        }
        // calypso --step — step mode: one step per Enter keypress
        [flag] if flag == "--step" => {
            let state_path = cwd.join(".calypso").join("repository-state.json");
            if state_path.exists() {
                run_state_machine_step(&state_path);
            } else {
                println!("{}", run_doctor(&cwd));
            }
        }
        // calypso — no args: drive state machine if initialized, else show doctor output
        [] => {
            let state_path = cwd.join(".calypso").join("repository-state.json");
            // Resolve --select-flow before checking whether the state file exists so that
            // the interactive selector is shown even on an uninitialised project directory.
            let flow = resolve_select_flow(select_flow, &cwd);
            match flow {
                Some(SelectedFlow::Workflow(name)) => {
                    run_workflow_auto(&name, &cwd);
                }
                None => {
                    if state_path.exists() {
                        run_state_machine_auto(&state_path, None);
                    } else {
                        println!("{}", run_doctor(&cwd));
                    }
                }
            }
        }
        _ => println!("{}", render_help(info)),
    }
}

/// Strip `--select-flow` from `args` and return whether the flag was present plus the remaining args.
fn extract_select_flow_flag(args: &[String]) -> (bool, Vec<String>) {
    let mut remaining = Vec::new();
    let mut found = false;
    for arg in args {
        if arg == "--select-flow" {
            found = true;
        } else {
            remaining.push(arg.clone());
        }
    }
    (found, remaining)
}

/// The result of an interactive workflow selection.
enum SelectedFlow {
    /// A workflow selected from the effective catalog.
    Workflow(String),
}

/// When `--select-flow` was requested, interactively list blueprint workflows that have a
/// `workflow_dispatch` or `cron` entry point plus any `.yml`/`.yaml` files found in
/// `{cwd}/.calypso/`, then return the selected flow.
///
/// Returns `None` if the flag was not set, the user cancelled, or no eligible workflows exist.
fn resolve_select_flow(select_flow: bool, cwd: &std::path::Path) -> Option<SelectedFlow> {
    if !select_flow {
        return None;
    }
    select_workflow_interactively(cwd)
}

/// One selectable entry in the interactive flow list.
struct FlowEntry {
    /// Human-readable label: `initial_state (trigger_type) -- filename.yaml`
    label: String,
    /// Catalog workflow name used by the executor registry.
    workflow_name: String,
}

/// Enumerate entrypoint workflows and prompt the user to pick one.
///
/// An "entrypoint workflow" is one with a `workflow_dispatch` (manual) or `cron` (scheduled)
/// trigger in its `on:` block.  Each (file, trigger-type) pair becomes a separate list entry
/// so that a file with both triggers appears twice.
fn select_workflow_interactively(cwd: &std::path::Path) -> Option<SelectedFlow> {
    use calypso_cli::blueprint_workflows::WorkflowCatalog;
    use std::io::{BufRead, Write};

    // ── 1. Collect candidates ─────────────────────────────────────────────────
    //
    // Local project files are listed first so they are easiest to reach;
    // embedded blueprint library entries follow as fallbacks.

    let mut entries: Vec<FlowEntry> = Vec::new();
    let catalog = WorkflowCatalog::load(cwd);

    for entry in catalog.entries() {
        let Ok(wf) = entry.parse() else {
            continue;
        };
        let filename = entry.handle.file_name.clone();
        let entry_name = wf.initial_state.as_deref().unwrap_or(&filename).to_string();

        if let Some(ref sched) = wf.schedule {
            entries.push(FlowEntry {
                label: format!("{entry_name} (cron: {}) -- {filename}", sched.cron),
                workflow_name: entry.handle.name.clone(),
            });
        }
        if wf.trigger.is_some() {
            entries.push(FlowEntry {
                label: format!("{entry_name} (workflow_dispatch) -- {filename}"),
                workflow_name: entry.handle.name.clone(),
            });
        }
    }

    if entries.is_empty() {
        eprintln!(
            "No entrypoint workflows found (no workflow_dispatch or cron triggers).\n\
             Catalog has {count} workflow(s) — none have user-facing entry points.",
            count = catalog.len()
        );
        return None;
    }

    // ── 2. Display numbered list ───────────────────────────────────────────────

    println!("Available workflows:");
    for (i, entry) in entries.iter().enumerate() {
        println!("  {}) {}", i + 1, entry.label);
    }

    // ── 3. Read user choice ────────────────────────────────────────────────────

    print!("Select workflow [1-{}]: ", entries.len());
    std::io::stdout().flush().ok();

    let stdin = std::io::stdin();
    let line = stdin.lock().lines().next()?.ok()?;
    let choice: usize = line.trim().parse().ok()?;

    if choice == 0 || choice > entries.len() {
        eprintln!("Invalid selection.");
        return None;
    }

    let selected = &entries[choice - 1];
    println!("Selected: {}", selected.label);

    Some(SelectedFlow::Workflow(selected.workflow_name.clone()))
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

fn run_calypso_init(
    cwd: &std::path::Path,
    allow_reinit: bool,
    org: Option<String>,
    repo_name: Option<String>,
    hello_world: bool,
) {
    // Print detected status.
    let status = detect_repo_status(cwd, &HostInitEnvironment);
    println!("Detected: {status}");

    // For fully configured repos without --reinit, just validate.
    if status == RepoInitStatus::FullyConfigured && !allow_reinit {
        println!("Repository is already fully configured.");
        println!(
            "Use `calypso init --reinit` to re-initialise or `calypso init --refresh` to update workflows."
        );
        return;
    }

    let mut progress =
        match run_init_interactive(cwd, allow_reinit, &HostInitEnvironment, hello_world) {
            Ok(p) => p,
            Err(error) => {
                eprintln!("init error: {error}");
                std::process::exit(1);
            }
        };

    // Store org/repo if provided (used for upstream creation).
    if org.is_some() {
        progress.github_org = org;
    }
    if repo_name.is_some() {
        progress.github_repo = repo_name;
    }

    println!("Init complete: {}", progress.current_step);
    println!("Completed steps:");
    for step in &progress.completed_steps {
        println!("  [x] {step}");
    }
}

fn run_calypso_init_json(
    cwd: &std::path::Path,
    allow_reinit: bool,
    org: Option<String>,
    repo_name: Option<String>,
    hello_world: bool,
) {
    let status = detect_repo_status(cwd, &HostInitEnvironment);

    // For fully configured repos without --reinit, report and exit.
    if status == RepoInitStatus::FullyConfigured && !allow_reinit {
        let report = serde_json::json!({
            "status": status,
            "message": "already configured",
            "completed": true
        });
        println!(
            "{}",
            serde_json::to_string_pretty(&report).expect("json serialization")
        );
        return;
    }

    let mut progress =
        match run_init_interactive(cwd, allow_reinit, &HostInitEnvironment, hello_world) {
            Ok(p) => p,
            Err(error) => {
                let report = serde_json::json!({
                    "status": status,
                    "error": error.to_string(),
                    "completed": false
                });
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report).expect("json serialization")
                );
                std::process::exit(1);
            }
        };

    if org.is_some() {
        progress.github_org = org;
    }
    if repo_name.is_some() {
        progress.github_repo = repo_name;
    }

    let report = serde_json::json!({
        "status": status,
        "current_step": progress.current_step.as_str(),
        "completed_steps": progress.completed_steps.iter().map(|s| s.as_str()).collect::<Vec<_>>(),
        "completed": progress.current_step.is_complete()
    });
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("json serialization")
    );
}

fn run_init_status(cwd: &std::path::Path) {
    let status = detect_repo_status(cwd, &HostInitEnvironment);
    println!("Repository status: {status}");

    // Show saved init progress if available.
    match InitProgress::load(cwd) {
        Ok(Some(progress)) => {
            println!("{}", render_init_status(&progress));
            if let Some(ref org) = progress.github_org {
                println!("GitHub org: {org}");
            }
            if let Some(ref repo) = progress.github_repo {
                println!("GitHub repo: {repo}");
            }
        }
        Ok(None) => {
            println!("No init state found — run `calypso init` to set up this repository.");
        }
        Err(error) => {
            eprintln!("Error loading init state: {error}");
        }
    }
}

fn run_init_refresh(cwd: &std::path::Path) {
    let status = detect_repo_status(cwd, &HostInitEnvironment);
    if status == RepoInitStatus::NoGit {
        eprintln!("Cannot refresh: not a git repository.");
        std::process::exit(1);
    }

    match refresh_workflows(cwd, &HostInitEnvironment) {
        Ok(refreshed) => {
            println!("Refreshed {} workflow files:", refreshed.len());
            for name in &refreshed {
                println!("  {name}");
            }
        }
        Err(error) => {
            eprintln!("refresh error: {error}");
            std::process::exit(1);
        }
    }
}

fn run_init_step_cmd(cwd: &std::path::Path, step_name: &str) {
    match run_init_step(cwd, step_name, &HostInitEnvironment) {
        Ok(progress) => {
            println!("{}", render_init_status(&progress));
        }
        Err(error) => {
            eprintln!("init step error: {error}");
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
    match run_doctor_fix_single(cwd, check_id) {
        Ok(result) => {
            if !result.applied {
                println!("Check '{check_id}' is already passing — no fix needed.");
                return;
            }

            println!("{}", result.output);

            match result.validated {
                Some(true) => {
                    println!("Validation: check '{check_id}' is now passing.");
                }
                Some(false) => {
                    eprintln!("Validation: check '{check_id}' is still failing after fix.");
                    std::process::exit(1);
                }
                None => {
                    println!("Manual fix — re-run `calypso doctor` to verify.");
                }
            }
        }
        Err(error) => {
            eprintln!("doctor fix: {error}");
            std::process::exit(1);
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

fn run_state_machine_auto(state_path: &std::path::Path, flow_override: Option<&std::path::Path>) {
    use calypso_cli::driver::{DriverMode, DriverStepResult, StateMachineDriver};
    use calypso_cli::execution::ExecutionConfig;
    use calypso_cli::template::{load_embedded_template_set, load_template_set_with_state_machine};

    let template = match flow_override {
        Some(path) => load_template_set_with_state_machine(path).unwrap_or_else(|_| {
            let name = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("selected file");
            eprintln!(
                "note: {name} is a GitHub Actions workflow file; \
                 running with the project's state machine template instead."
            );
            load_embedded_template_set().expect("embedded templates should be valid")
        }),
        None => load_embedded_template_set().expect("embedded templates should be valid"),
    };
    let driver = StateMachineDriver {
        mode: DriverMode::Auto,
        state_path: state_path.to_path_buf(),
        template,
        config: ExecutionConfig::default(),
        executor: None,
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

/// Run a workflow selected from the effective catalog through the shared interpreter.
fn run_workflow_auto(workflow_name: &str, cwd: &std::path::Path) {
    use calypso_cli::blueprint_workflows::StateKind;
    use calypso_cli::blueprint_workflows::WorkflowCatalog;
    use calypso_cli::claude::{ClaudeConfig, ClaudeOutcome, ClaudeSession, SessionContext};
    use calypso_cli::interpreter::{StepOutcome, WorkflowInterpreter};

    let catalog = WorkflowCatalog::load(cwd);
    let interp = match WorkflowInterpreter::from_catalog(&catalog) {
        Ok(i) => i,
        Err(e) => {
            eprintln!("error: failed to load workflow registry: {e}");
            std::process::exit(1);
        }
    };
    let mut exec = match interp.start(workflow_name) {
        Ok(e) => e,
        Err(e) => {
            eprintln!("error: {e}");
            std::process::exit(1);
        }
    };

    println!("Starting workflow: {workflow_name}");

    let session = ClaudeSession::new(ClaudeConfig::default());
    let context = SessionContext {
        working_directory: Some(cwd.to_string_lossy().into_owned()),
    };

    loop {
        let state_name = exec.position.state.clone();
        let wf_name = exec.position.workflow.clone();
        let cfg = match interp.current_state_config(&exec) {
            Some(c) => c,
            None => {
                eprintln!("error: state '{state_name}' not found in workflow '{wf_name}'");
                std::process::exit(1);
            }
        };

        // Determine the event to fire after executing this state.
        let event: String = match &cfg.kind {
            Some(StateKind::Terminal) | None => {
                println!("→ {state_name} (done)");
                break;
            }
            Some(StateKind::Agent) => {
                println!("→ {state_name}");
                let prompt = blueprint_agent_prompt(&state_name, cfg);
                match session.invoke(&prompt, &context, None) {
                    Ok(ClaudeOutcome::Ok {
                        suggested_next_state,
                        summary,
                        ..
                    }) => {
                        println!("  {summary}");
                        suggested_next_state.unwrap_or_else(|| "on_success".to_string())
                    }
                    Ok(ClaudeOutcome::Nok { reason, .. }) => {
                        eprintln!("  step failed: {reason}");
                        "on_failure".to_string()
                    }
                    Ok(ClaudeOutcome::Aborted { reason }) => {
                        eprintln!("  aborted: {reason}");
                        std::process::exit(1);
                    }
                    Err(e) => {
                        eprintln!("  provider error: {e}");
                        std::process::exit(1);
                    }
                }
            }
            Some(StateKind::Human) => {
                println!("→ {state_name} (human gate)");
                if let Some(p) = &cfg.prompt {
                    println!("{p}");
                }
                let available = cfg
                    .next
                    .as_ref()
                    .map(|n| n.all_event_keys().join(", "))
                    .unwrap_or_default();
                if !available.is_empty() {
                    println!("Available responses: {available}");
                }
                use std::io::{BufRead, Write};
                print!("Enter event: ");
                std::io::stdout().flush().ok();
                std::io::stdin()
                    .lock()
                    .lines()
                    .next()
                    .and_then(|l| l.ok())
                    .map(|l| l.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .unwrap_or_else(|| "abort".to_string())
            }
            Some(StateKind::Deterministic) => {
                println!("→ {state_name} (deterministic)");
                if let Some(cmd) = &cfg.command {
                    let ok = std::process::Command::new("sh")
                        .arg("-c")
                        .arg(cmd)
                        .current_dir(cwd)
                        .status()
                        .is_ok_and(|s| s.success());
                    if ok { "on_pass" } else { "on_fail" }.to_string()
                } else {
                    "on_pass".to_string()
                }
            }
            Some(StateKind::Github) => {
                // GitHub-Actions-hosted states cannot run locally; skip with a note.
                println!(
                    "→ {state_name} (note: this state normally runs via GitHub Actions hosted runners; skipping in local mode)"
                );
                "on_pass".to_string()
            }
            _ => {
                // Workflow-delegation and other kinds are handled by the interpreter
                // automatically when we call advance(); just supply a generic event.
                "on_complete".to_string()
            }
        };

        match interp.advance(&mut exec, &event) {
            StepOutcome::Advanced(_) => {
                // exec.position is already updated; continue the loop.
            }
            StepOutcome::EnteredSubWorkflow { child, .. } => {
                println!(
                    "  → entering sub-workflow: {} at {}",
                    child.workflow, child.state
                );
            }
            StepOutcome::ReturnedToParent {
                terminal_state,
                parent,
            } => {
                println!(
                    "  → sub-workflow complete ({terminal_state}), returned to {}",
                    parent.state
                );
            }
            StepOutcome::Terminal(pos) => {
                println!("→ {} (terminal: workflow complete)", pos.state);
                break;
            }
            StepOutcome::Error(e) => {
                eprintln!("transition error: {e}");
                std::process::exit(1);
            }
        }
    }
}

/// Build the Claude prompt for an agent state in a blueprint workflow.
fn blueprint_agent_prompt(
    state_name: &str,
    cfg: &calypso_cli::blueprint_workflows::StateConfig,
) -> String {
    let task = cfg
        .prompt
        .as_deref()
        .unwrap_or("Complete your assigned task.");
    let role = cfg.role.as_deref().unwrap_or("agent");
    let events = cfg
        .next
        .as_ref()
        .map(|n| n.all_event_keys().join(", "))
        .unwrap_or_default();
    let events_section = if events.is_empty() {
        String::new()
    } else {
        format!("\nAvailable outcome events (set `suggested_next_state` to one): {events}\n")
    };
    format!(
        "You are the `{role}` agent at workflow state `{state_name}`.\n\n\
         {task}\n\
         {events_section}\n\
         When complete, emit exactly one outcome marker on its own line:\n\
           [CALYPSO:OK]{{\"summary\":\"...\",\"artifact_refs\":[],\"suggested_next_state\":\"<event>\"}}\n\
           [CALYPSO:NOK]{{\"summary\":\"...\",\"reason\":\"...\"}}\n\
           [CALYPSO:ABORTED]{{\"reason\":\"...\"}}\n\
         If you need clarification from the operator, emit:\n\
           [CALYPSO:CLARIFICATION]<your question here>"
    )
}

fn run_state_machine_step(state_path: &std::path::Path) {
    use calypso_cli::driver::{DriverMode, DriverStepResult, StateMachineDriver};
    use calypso_cli::execution::ExecutionConfig;
    use calypso_cli::pinned_prompt::{
        Confirmation, PinnedPrompt, format_initial_prompt, format_transition_prompt,
    };
    use calypso_cli::state::RepositoryState;
    use calypso_cli::template::load_embedded_template_set;

    let template = load_embedded_template_set().expect("embedded templates should be valid");
    let driver = StateMachineDriver {
        mode: DriverMode::Step,
        state_path: state_path.to_path_buf(),
        template,
        config: ExecutionConfig::default(),
        executor: None,
    };

    let mut prompt = match PinnedPrompt::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("failed to initialize pinned prompt: {e}");
            std::process::exit(1);
        }
    };

    loop {
        let current = match RepositoryState::load_from_path(state_path) {
            Ok(state) => state.current_feature.workflow_state.as_str().to_string(),
            Err(e) => {
                let _ = prompt.cleanup();
                eprintln!("error loading state: {e}");
                std::process::exit(1);
            }
        };

        let prompt_text = format_initial_prompt(&current);
        if let Err(e) = prompt.show_prompt(&prompt_text) {
            let _ = prompt.cleanup();
            eprintln!("prompt error: {e}");
            std::process::exit(1);
        }

        match prompt.read_confirmation() {
            Ok(Confirmation::Yes) => {}
            Ok(Confirmation::No | Confirmation::Quit) => break,
            Err(e) => {
                let _ = prompt.cleanup();
                eprintln!("input error: {e}");
                std::process::exit(1);
            }
        }

        match driver.step() {
            DriverStepResult::Advanced(next_state) => {
                let next = next_state.as_str();
                let _ = prompt.log(&format!("→ advanced to: {next}"));
                let transition_prompt = format_transition_prompt(&current, next);
                let _ = prompt.show_prompt(&transition_prompt);
            }
            DriverStepResult::Terminal => {
                let _ = prompt.log("done");
                break;
            }
            DriverStepResult::Unchanged => {
                let _ = prompt.log("step complete (state unchanged)");
            }
            DriverStepResult::ClarificationRequired(q) => {
                let _ = prompt.log(&format!("clarification required: {q}"));
            }
            DriverStepResult::Failed { reason } => {
                let _ = prompt.log(&format!("step failed: {reason}"));
            }
            DriverStepResult::Error(e) => {
                let _ = prompt.cleanup();
                eprintln!("error: {e}");
                std::process::exit(1);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_path_flag, extract_select_flow_flag, looks_like_path};

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

    // ── extract_select_flow_flag ──────────────────────────────────────────────

    #[test]
    fn select_flow_flag_is_stripped_from_args() {
        let args = s(&["--path", "/my/project", "--select-flow", "doctor"]);
        let (_, after_path) = extract_path_flag(&args);
        let (found, remaining) = extract_select_flow_flag(&after_path);
        assert!(found, "expected --select-flow to be detected");
        assert_eq!(remaining, s(&["doctor"]));
    }

    #[test]
    fn select_flow_flag_absent_returns_false() {
        let args = s(&["doctor", "--verbose"]);
        let (found, remaining) = extract_select_flow_flag(&args);
        assert!(!found);
        assert_eq!(remaining, s(&["doctor", "--verbose"]));
    }

    #[test]
    fn select_flow_flag_can_appear_anywhere_in_args() {
        let args = s(&["--select-flow"]);
        let (found, remaining) = extract_select_flow_flag(&args);
        assert!(found);
        assert!(remaining.is_empty());
    }

    #[test]
    fn select_flow_flag_does_not_consume_adjacent_args() {
        let args = s(&["--select-flow", "status"]);
        let (found, remaining) = extract_select_flow_flag(&args);
        assert!(found);
        assert_eq!(remaining, s(&["status"]));
    }

    #[test]
    fn select_flow_flag_works_with_empty_args() {
        let (found, remaining) = extract_select_flow_flag(&[]);
        assert!(!found);
        assert!(remaining.is_empty());
    }

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }
}
