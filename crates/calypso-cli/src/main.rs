mod daemon;
mod run_control;

use calypso_runtime::operator_surface::OperatorSurface;
use calypso_runtime::state::RepositoryState;
use calypso_runtime::workflow_run::WorkflowRun;
use calypso_web::run_webview;
use nightshift_core::app::{
    render_fix_results, run_doctor, run_doctor_fix_all, run_doctor_fix_single, run_doctor_json,
    run_doctor_verbose, run_status, run_workflows_list, run_workflows_show, run_workflows_validate,
};
use nightshift_core::telemetry::{Component, LogEvent, LogFormat, LogLevel, Logger};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct BuildInfo<'a> {
    version: &'a str,
    git_hash: &'a str,
    build_time: &'a str,
    git_tags: &'a str,
}

fn render_version(info: BuildInfo<'_>) -> String {
    format!(
        "calypso-cli {} git:{} built:{} tags:{}",
        info.version, info.git_hash, info.build_time, info.git_tags
    )
}

fn render_help(info: BuildInfo<'_>) -> String {
    format!(
        "\
calypso-cli {}

Usage:
  calypso [OPTIONS] [COMMAND]

Options:
  -p, --path <dir>    Project directory (default: current working directory)
  --select-flow       Interactively pick a workflow with a manual or cron entry point
  -h, --help          Show this help output
  -v, --version       Show build version information
  -v, -vv             Verbosity: -v = info, -vv = debug (default: debug)
  --json              Emit JSON-lines instead of human-readable text

Daemon commands:
  (none)              Start the daemon (continuous scheduling, non-interactive)
  daemon start        Start the daemon explicitly
  daemon start --single-pass
                      Run one scheduling pass and exit (CI/test mode)

Run inspection and control:
  run list            List workflow runs
  run inspect [run-id]
                      Show details for the current or specified workflow run
  run retry           Retry the current step of the active workflow run
  run abort           Abort the active workflow run
  run clarify <message>
                      Provide clarification to a stuck agent step
  run force-transition <state> --reason <reason>
                      Force-transition to a state with recorded operator intent

Diagnostics:
  doctor              Check local prerequisites and environment
  doctor --verbose    Show detailed remediation steps for failing checks
  doctor --json       Output doctor results as JSON (exit 1 if any failing)
  doctor --fix        Apply auto-fixes for all failing checks
  doctor --fix <id>   Apply an available fix for a specific doctor check
  status              Render the feature status for the project directory

Workflows:
  workflows list      List effective workflow names for the project directory
  workflows show <name>
                      Print the raw YAML for the named workflow
  workflows validate <name>
                      Parse the named workflow and report OK or the parse error

Git hash: {}  Built: {}  Tags: {}",
        info.version, info.git_hash, info.build_time, info.git_tags
    )
}

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

fn build_logger() -> Logger {
    Logger::new().with_format(LogFormat::Text)
}

fn emit_dispatch_log(logger: &Logger, cwd: &std::path::Path, args: &[String]) {
    if matches!(args, [flag] if flag == "-h" || flag == "--help" || flag == "-v" || flag == "--version")
    {
        return;
    }

    let command = if args.is_empty() {
        "(default)".to_string()
    } else {
        args.join(" ")
    };

    logger
        .entry(
            LogLevel::Info,
            &format!("dispatching calypso-cli {command} in {}", cwd.display()),
        )
        .component(Component::Cli)
        .event(LogEvent::Startup)
        .emit();
}

fn main() {
    let info = build_info();
    let logger = build_logger();
    let raw_args: Vec<String> = std::env::args().skip(1).collect();

    // Strip -p / --path <dir> from args before dispatching.
    // This makes --path a global flag that works with every subcommand.
    let (path_override, args_after_path) = extract_path_flag(&raw_args);

    // If no --path flag was given, absorb a bare positional path argument so that
    // `calypso ~/some-dir` and `calypso --path ~/some-dir` behave identically and
    // both route through daemon::run_daemon_default.  Any invocation that does not
    // explicitly include --select-flow therefore reaches daemon::run_daemon_default
    // regardless of how the project directory was supplied.
    let (positional_path_override, args_after_positional) = if path_override.is_none() {
        extract_positional_path_flag(&args_after_path)
    } else {
        (None, args_after_path)
    };
    let cwd = path_override
        .or(positional_path_override)
        .unwrap_or_else(|| std::env::current_dir().expect("current directory should resolve"));

    // Strip --select-flow from args before dispatching.
    let (select_flow, args_after_select_flow) = extract_select_flow_flag(&args_after_positional);

    let args = args_after_select_flow;
    emit_dispatch_log(&logger, &cwd, &args);

    match args.as_slice() {
        [flag] if flag == "-h" || flag == "--help" => println!("{}", render_help(info)),
        [flag] if flag == "-v" || flag == "--version" => println!("{}", render_version(info)),
        // ── Daemon commands ──────────────────────────────────────────────
        [command, subcommand] if command == "daemon" && subcommand == "start" => {
            daemon::run_daemon_start(&cwd, false);
        }
        [command, subcommand, flag]
            if command == "daemon" && subcommand == "start" && flag == "--single-pass" =>
        {
            daemon::run_daemon_start(&cwd, true);
        }
        // ── Run inspection and control ───────────────────────────────────
        [command, subcommand] if command == "run" && subcommand == "list" => {
            run_control::run_list(&cwd);
        }
        [command, subcommand] if command == "run" && subcommand == "inspect" => {
            run_control::run_inspect(&cwd, None);
        }
        [command, subcommand, run_id] if command == "run" && subcommand == "inspect" => {
            run_control::run_inspect(&cwd, Some(run_id));
        }
        [command, subcommand] if command == "run" && subcommand == "retry" => {
            run_control::run_retry(&cwd);
        }
        [command, subcommand] if command == "run" && subcommand == "abort" => {
            run_control::run_abort(&cwd);
        }
        [command, subcommand, message] if command == "run" && subcommand == "clarify" => {
            run_control::run_clarify(&cwd, message);
        }
        [command, subcommand, target_state, flag, reason]
            if command == "run" && subcommand == "force-transition" && flag == "--reason" =>
        {
            run_control::run_force_transition(&cwd, target_state, reason);
        }
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
        [command, flag, path] if command == "status" && flag == "--run" => render_run_status(path),
        [command, flag, path, _headless] if command == "status" && flag == "--run" => {
            render_run_status(path)
        }
        // calypso webview
        [command] if command == "webview" => {
            if let Err(error) = run_webview(&cwd, 7373) {
                eprintln!("webview error: {error}");
                std::process::exit(1);
            }
        }
        // calypso webview --port <N>
        [command, flag, port_str] if command == "webview" && flag == "--port" => {
            let port: u16 = port_str.parse().unwrap_or(7373);
            if let Err(error) = run_webview(&cwd, port) {
                eprintln!("webview error: {error}");
                std::process::exit(1);
            }
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
        // calypso — no args: daemon-first continuous scheduling if initialized,
        // else show doctor output. The --select-flow flag falls back to legacy
        // interactive selection for backward compatibility.
        [] => {
            if select_flow {
                run_project_dir(&cwd, select_flow);
            } else {
                daemon::run_daemon_default(&cwd);
            }
        }
        _ => println!("{}", render_help(info)),
    }
}

fn run_project_dir(project_dir: &std::path::Path, select_flow: bool) {
    // Resolve --select-flow before dispatching so the interactive selector is
    // shown even on an uninitialised project directory.
    let flow = resolve_select_flow(select_flow, project_dir);
    match flow {
        Some(SelectedFlow::Workflow(name)) => {
            run_workflow_auto(&name, project_dir);
        }
        None => {
            // --select-flow was used but user cancelled or no eligible workflows.
            println!("{}", run_doctor(project_dir));
        }
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

/// When `--select-flow` was requested, interactively list workflows that have a
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
    use calypso_workflows::WorkflowCatalog;
    use std::io::{BufRead, Write};

    // ── 1. Collect candidates ─────────────────────────────────────────────────
    //
    // Local project files are listed first so they are easiest to reach;
    // embedded workflow catalog entries follow as fallbacks.

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

fn looks_like_path(arg: &str) -> bool {
    arg.starts_with('.')
        || arg.starts_with('/')
        || arg.starts_with('~')
        || std::path::Path::new(arg).is_dir()
}

/// If the argument list consists solely of a single path-like argument (optionally
/// combined with `--select-flow` which is stripped later), extract that argument as
/// a project-directory override and return the remaining args with it removed.
///
/// This lets `calypso ~/some-dir` behave identically to `calypso --path ~/some-dir`:
/// the directory is resolved as `cwd` and the dispatch falls through to
/// `daemon::run_daemon_default` (or `run_project_dir` when `--select-flow` is set).
fn extract_positional_path_flag(args: &[String]) -> (Option<std::path::PathBuf>, Vec<String>) {
    // Only absorb the positional arg when it is the sole non-select-flow token.
    // Multi-arg invocations (e.g. `calypso ./dir doctor`) are left to fall through
    // to the help output so the user sees a clear error rather than a silent mis-route.
    let non_select_flow: Vec<&String> = args
        .iter()
        .filter(|a| a.as_str() != "--select-flow")
        .collect();
    if non_select_flow.len() == 1 && looks_like_path(non_select_flow[0]) {
        let path = std::path::PathBuf::from(non_select_flow[0]);
        let remaining: Vec<String> = args
            .iter()
            .filter(|a| *a != non_select_flow[0])
            .cloned()
            .collect();
        (Some(path), remaining)
    } else {
        (None, args.to_vec())
    }
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

fn render_status(path: &str) {
    let state = RepositoryState::load_from_path(std::path::Path::new(path))
        .expect("status state file should load");
    let surface = OperatorSurface::from_feature_state(&state.current_feature);
    println!("{}", surface.render());
}

fn render_run_status(path: &str) {
    let run = WorkflowRun::load(std::path::Path::new(path))
        .expect("workflow run file should load")
        .expect("workflow run file should exist");
    let surface = OperatorSurface::from_workflow_run(&run);
    println!("{}", surface.render());
}

/// Run a workflow selected from the effective catalog through the shared interpreter.
fn run_workflow_auto(workflow_name: &str, cwd: &std::path::Path) {
    use calypso_workflow_exec::{StepOutcome, WorkflowInterpreter};
    use calypso_workflows::{StateKind, WorkflowCatalog};
    use nightshift_core::claude::{ClaudeConfig, ClaudeOutcome, ClaudeSession, SessionContext};

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
                let prompt = workflow_agent_prompt(&state_name, cfg);
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

/// Build the Claude prompt for an agent state in a workflow.
fn workflow_agent_prompt(state_name: &str, cfg: &calypso_workflows::StateConfig) -> String {
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

#[cfg(test)]
mod tests {
    use super::{
        BuildInfo, extract_path_flag, extract_positional_path_flag, extract_select_flow_flag,
        render_help, render_version,
    };

    // ── extract_positional_path_flag ─────────────────────────────────────────
    // The positional [path] dispatch arm was removed from main(). A bare path-like
    // argument is now absorbed by extract_positional_path_flag so that
    // `calypso ~/some-dir` behaves identically to `calypso --path ~/some-dir`,
    // routing through daemon::run_daemon_default rather than run_project_dir.

    #[test]
    fn positional_path_absorbed_for_dot_relative() {
        let args = s(&["./my-project"]);
        let (path, remaining) = extract_positional_path_flag(&args);
        assert_eq!(path, Some(std::path::PathBuf::from("./my-project")));
        assert!(remaining.is_empty());
    }

    #[test]
    fn positional_path_absorbed_for_absolute() {
        let args = s(&["/home/user/project"]);
        let (path, remaining) = extract_positional_path_flag(&args);
        assert_eq!(path, Some(std::path::PathBuf::from("/home/user/project")));
        assert!(remaining.is_empty());
    }

    #[test]
    fn positional_path_absorbed_for_tilde() {
        let args = s(&["~/projects/calypso"]);
        let (path, remaining) = extract_positional_path_flag(&args);
        assert_eq!(path, Some(std::path::PathBuf::from("~/projects/calypso")));
        assert!(remaining.is_empty());
    }

    #[test]
    fn positional_path_not_absorbed_for_subcommand() {
        // Subcommand names must not be consumed as paths.
        for cmd in &["doctor", "status", "watch", "--version", "-v"] {
            let args = s(&[cmd]);
            let (path, remaining) = extract_positional_path_flag(&args);
            assert!(path.is_none(), "should not absorb subcommand '{cmd}'");
            assert_eq!(remaining, args);
        }
    }

    #[test]
    fn positional_path_not_absorbed_when_path_flag_already_set() {
        // When --path was already extracted, extract_positional_path_flag is
        // skipped entirely in main(). This test confirms that a lone subcommand
        // after --path stripping is left untouched.
        let args = s(&["doctor"]);
        let (path, remaining) = extract_positional_path_flag(&args);
        assert!(path.is_none());
        assert_eq!(remaining, args);
    }

    #[test]
    fn positional_path_absorbed_alongside_select_flow() {
        // `calypso ./my-dir --select-flow` should absorb ./my-dir as cwd while
        // leaving --select-flow for later extraction.
        let args = s(&["./my-project", "--select-flow"]);
        let (path, remaining) = extract_positional_path_flag(&args);
        assert_eq!(path, Some(std::path::PathBuf::from("./my-project")));
        assert_eq!(remaining, s(&["--select-flow"]));
    }

    #[test]
    fn positional_path_not_absorbed_for_multi_arg_invocation() {
        // `calypso ./dir doctor` — ambiguous; leave it for the help/default arm.
        let args = s(&["./my-project", "doctor"]);
        let (path, remaining) = extract_positional_path_flag(&args);
        assert!(
            path.is_none(),
            "should not absorb path when multiple non-select-flow args are present"
        );
        assert_eq!(remaining, args);
    }

    #[test]
    fn positional_path_absorbed_for_existing_directory() {
        let tmp = std::env::temp_dir();
        let tmp_str = tmp.to_str().expect("temp dir should be valid utf-8");
        let args = s(&[tmp_str]);
        let (path, remaining) = extract_positional_path_flag(&args);
        assert_eq!(path, Some(tmp));
        assert!(remaining.is_empty());
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

    #[test]
    fn version_output_contains_required_build_metadata() {
        let output = render_version(sample_info());

        assert!(output.contains("0.1.0+abc123"), "missing semver+hash");
        assert!(output.contains("abc123"), "missing git hash");
        assert!(output.contains("2026-03-13T12:00:00Z"), "missing timestamp");
        assert!(output.contains("v0.1.0"), "missing git tag");
    }

    #[test]
    fn version_output_is_a_single_line() {
        let output = render_version(sample_info());
        assert_eq!(output.lines().count(), 1, "version output must be one line");
    }

    #[test]
    fn help_output_exposes_version_information() {
        let output = render_help(sample_info());

        assert!(output.contains("calypso-cli"));
        assert!(output.contains("0.1.0+abc123"));
        assert!(output.contains("Git hash: abc123"));
        assert!(output.contains("Daemon commands:"));
        assert!(output.contains("--path"));
        assert!(output.contains("-h, --help"));
    }

    #[test]
    fn help_output_documents_json_flag() {
        let output = render_help(sample_info());

        assert!(output.contains("--json"), "missing --json flag");
    }

    fn s(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn sample_info() -> BuildInfo<'static> {
        BuildInfo {
            version: "0.1.0+abc123",
            git_hash: "abc123",
            build_time: "2026-03-13T12:00:00Z",
            git_tags: "v0.1.0",
        }
    }
}
