//! Local HTTP server for the `calypso webview` command.
//!
//! Serves a single-page application that visualises the active state machine,
//! lists cron workflows, and provides trigger buttons for human states.
//!
//! No external HTTP crate is required — the server uses only `std::net::TcpListener`
//! with one thread per connection.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::event_log::{self, EventKind};

// ── Embedded HTML page ────────────────────────────────────────────────────────

const INDEX_HTML: &str = include_str!("webview_index.html");

// ── Public API ────────────────────────────────────────────────────────────────

/// Returns the first non-loopback IPv4 address of this machine, if any.
fn public_ip() -> Option<std::net::Ipv4Addr> {
    use std::net::{SocketAddr, UdpSocket};
    // Connect to an external address (no data is sent) to discover the local
    // source IP the OS would use for outbound traffic.
    let socket = UdpSocket::bind("0.0.0.0:0").ok()?;
    socket.connect("8.8.8.8:80").ok()?;
    match socket.local_addr().ok()? {
        SocketAddr::V4(addr) => Some(*addr.ip()),
        SocketAddr::V6(_) => None,
    }
}

/// Start the local webview HTTP server on `0.0.0.0:{port}` (all interfaces).
///
/// Prints the local URL and the public/LAN IP if detectable. Blocks forever
/// (until the process is killed). Each connection is handled on a dedicated thread.
pub fn run_webview(cwd: &Path, port: u16) {
    let addr = format!("0.0.0.0:{port}");
    let listener = TcpListener::bind(&addr).unwrap_or_else(|_| {
        eprintln!("error: port {port} is already in use. Try: calypso webview --port <N>");
        std::process::exit(1);
    });
    println!("Calypso webview running at http://localhost:{port}");
    if let Some(ip) = public_ip() {
        println!("                       http://{ip}:{port}  (network)");
    }
    println!("Press Ctrl+C to stop.");
    for stream in listener.incoming().flatten() {
        let cwd = cwd.to_path_buf();
        std::thread::spawn(move || handle_connection(stream, &cwd));
    }
}

// ── Connection handler ────────────────────────────────────────────────────────

fn handle_connection(mut stream: std::net::TcpStream, cwd: &Path) {
    let mut reader = BufReader::new(&stream);
    let mut request_line = String::new();
    reader.read_line(&mut request_line).ok();

    // Read headers to find Content-Length.
    let mut content_length = 0usize;
    loop {
        let mut line = String::new();
        reader.read_line(&mut line).ok();
        let trimmed = line.trim();
        if trimmed.is_empty() {
            break;
        }
        if let Some(rest) = trimmed.to_ascii_lowercase().strip_prefix("content-length:") {
            content_length = rest.trim().parse().unwrap_or(0);
        }
    }

    // Read body for POST requests.
    let mut body = vec![0u8; content_length];
    if content_length > 0 {
        reader.read_exact(&mut body).ok();
    }

    let parts: Vec<&str> = request_line.trim().splitn(3, ' ').collect();
    let method = parts.first().copied().unwrap_or("");
    let path = parts.get(1).copied().unwrap_or("/");

    let (status, content_type, body_bytes) = route(method, path, &body, cwd);

    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n",
        body_bytes.len()
    );
    stream.write_all(response.as_bytes()).ok();
    stream.write_all(&body_bytes).ok();
}

// ── Router ────────────────────────────────────────────────────────────────────

fn route(
    method: &str,
    path: &str,
    body: &[u8],
    cwd: &Path,
) -> (&'static str, &'static str, Vec<u8>) {
    match (method, path) {
        ("GET", "/") | ("GET", "/index.html") => (
            "200 OK",
            "text/html; charset=utf-8",
            INDEX_HTML.as_bytes().to_vec(),
        ),
        ("GET", "/api/state") => {
            let json = read_state_json(cwd);
            ("200 OK", "application/json", json.into_bytes())
        }
        ("GET", "/api/workflows") => {
            let json = read_workflows_json();
            ("200 OK", "application/json", json.into_bytes())
        }
        ("POST", "/api/trigger") => {
            handle_trigger(body, cwd);
            ("200 OK", "application/json", b"{\"ok\":true}".to_vec())
        }
        ("POST", "/api/cron-now") => {
            handle_cron_now(body, cwd);
            ("200 OK", "application/json", b"{\"ok\":true}".to_vec())
        }
        ("GET", "/api/log") => {
            let json = read_event_log_json(cwd);
            ("200 OK", "application/json", json.into_bytes())
        }
        _ => ("404 Not Found", "text/plain", b"Not found".to_vec()),
    }
}

// ── API handlers ──────────────────────────────────────────────────────────────

/// Read combined state from `.calypso/` directory and return as JSON.
///
/// Returns a JSON object with:
/// - `workflow_state`: content of `workflow-state.json` or `null`
/// - `feature_state`: content of `state.json` or `null`
/// - `cron_workflows`: array of `{ name, cron, description }` from embedded workflows
/// - `active_transitions`: outgoing event keys from the active state, or `[]`
/// - `active_state_kind`: kind of the active state, or `null`
fn read_state_json(cwd: &Path) -> String {
    let calypso_dir = cwd.join(".calypso");

    // Read workflow-state.json
    let workflow_state: Value =
        read_json_file(&calypso_dir.join("workflow-state.json")).unwrap_or(Value::Null);

    // Read state.json
    let feature_state: Value =
        read_json_file(&calypso_dir.join("state.json")).unwrap_or(Value::Null);

    // Collect cron workflows from the embedded library.
    let cron_workflows = collect_cron_workflows();

    // Determine the active workflow name and state from workflow_state.
    let active_workflow_name = workflow_state
        .get("workflow")
        .or_else(|| workflow_state.get("active_workflow"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let active_state_name = workflow_state
        .get("state")
        .or_else(|| workflow_state.get("current_state"))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Determine transitions and kind for the active state.
    // Priority: local .calypso/workflows/ files first, then the embedded library.
    let (active_transitions, active_state_kind) = if let (Some(wf_name), Some(state_name)) = (
        active_workflow_name.as_deref(),
        active_state_name.as_deref(),
    ) {
        resolve_active_state_info_with_local(&calypso_dir.join("workflows"), wf_name, state_name)
    } else {
        (vec![], None)
    };

    let result = serde_json::json!({
        "workflow_state": workflow_state,
        "feature_state": feature_state,
        "cron_workflows": cron_workflows,
        "active_transitions": active_transitions,
        "active_state_kind": active_state_kind,
    });

    serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string())
}

/// Return all embedded workflows as a JSON array of `{ name, yaml }` objects.
fn read_workflows_json() -> String {
    use crate::blueprint_workflows::BlueprintWorkflowLibrary;

    let entries: Vec<Value> = BlueprintWorkflowLibrary::list()
        .iter()
        .map(|(name, yaml)| {
            serde_json::json!({
                "name": name,
                "yaml": yaml,
            })
        })
        .collect();

    serde_json::to_string(&Value::Array(entries)).unwrap_or_else(|_| "[]".to_string())
}

/// Parse `{ "event": "..." }` from body and write to `.calypso/pending-event.json`.
fn handle_trigger(body: &[u8], cwd: &Path) {
    let calypso_dir = cwd.join(".calypso");
    if let Ok(parsed) = serde_json::from_slice::<Value>(body) {
        if let Some(name) = parsed.get("event").and_then(|v| v.as_str()) {
            event_log::append(&calypso_dir, EventKind::Trigger, name);
        }
        let out = serde_json::to_string_pretty(&parsed).unwrap_or_default();
        let _ = std::fs::write(calypso_dir.join("pending-event.json"), out);
    }
}

/// Parse `{ "workflow": "..." }` from body and write to `.calypso/pending-cron.json`.
fn handle_cron_now(body: &[u8], cwd: &Path) {
    let calypso_dir = cwd.join(".calypso");
    if let Ok(parsed) = serde_json::from_slice::<Value>(body) {
        if let Some(name) = parsed.get("workflow").and_then(|v| v.as_str()) {
            event_log::append(&calypso_dir, EventKind::Cron, name);
        }
        let out = serde_json::to_string_pretty(&parsed).unwrap_or_default();
        let _ = std::fs::write(calypso_dir.join("pending-cron.json"), out);
    }
}

/// Return the event log as a JSON array.
fn read_event_log_json(cwd: &Path) -> String {
    let calypso_dir = cwd.join(".calypso");
    let entries = event_log::read_log(&calypso_dir);
    serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn read_json_file(path: &Path) -> Option<Value> {
    let text = std::fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

/// Collect all embedded workflows that declare a `schedule.cron` field.
fn collect_cron_workflows() -> Vec<Value> {
    use crate::blueprint_workflows::BlueprintWorkflowLibrary;

    let mut result = Vec::new();
    for (name, yaml) in BlueprintWorkflowLibrary::list() {
        if let Ok(wf) = BlueprintWorkflowLibrary::parse(yaml)
            && let Some(schedule) = &wf.schedule
        {
            result.push(serde_json::json!({
                "name": name,
                "cron": schedule.cron,
                "description": schedule.description,
            }));
        }
    }
    result
}

/// Like `resolve_active_state_info` but checks local YAML files in `workflows_dir` first.
///
/// Tries every `.yml` / `.yaml` file whose stem matches `workflow_name` (or whose
/// `name:` field matches). Falls back to the embedded blueprint library.
fn resolve_active_state_info_with_local(
    workflows_dir: &Path,
    workflow_name: &str,
    state_name: &str,
) -> (Vec<String>, Option<String>) {
    use crate::blueprint_workflows::BlueprintWorkflowLibrary;

    // Try local files first.
    if let Ok(entries) = std::fs::read_dir(workflows_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let is_yaml = path
                .extension()
                .map(|e| e == "yml" || e == "yaml")
                .unwrap_or(false);
            if !is_yaml {
                continue;
            }
            let yaml = match std::fs::read_to_string(&path) {
                Ok(y) => y,
                Err(_) => continue,
            };
            let wf = match BlueprintWorkflowLibrary::parse(&yaml) {
                Ok(w) => w,
                Err(_) => continue,
            };
            // Match by name field or by file stem.
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or_default();
            let matches = wf.name.as_deref() == Some(workflow_name) || stem == workflow_name;
            if !matches {
                continue;
            }
            if let Some(state) = wf.states.get(state_name) {
                let kind = state.kind.as_ref().map(|k| {
                    use crate::blueprint_workflows::StateKind;
                    match k {
                        StateKind::Deterministic => "deterministic",
                        StateKind::Agent => "agent",
                        StateKind::Human => "human",
                        StateKind::Github => "github",
                        StateKind::Function => "function",
                        StateKind::Workflow => "workflow",
                        StateKind::Terminal => "terminal",
                        StateKind::GitHook => "git-hook",
                        StateKind::Ci => "ci",
                    }
                    .to_string()
                });
                let transitions = state
                    .next
                    .as_ref()
                    .map(|n| n.all_event_keys().iter().map(|s| s.to_string()).collect())
                    .unwrap_or_default();
                return (transitions, kind);
            }
        }
    }

    // Fall back to the embedded library.
    resolve_active_state_info(workflow_name, state_name)
}

/// Given a workflow name and state name, return (transitions, kind) for that state.
fn resolve_active_state_info(
    workflow_name: &str,
    state_name: &str,
) -> (Vec<String>, Option<String>) {
    use crate::blueprint_workflows::BlueprintWorkflowLibrary;

    let yaml = match BlueprintWorkflowLibrary::get(workflow_name) {
        Some(y) => y,
        None => return (vec![], None),
    };
    let wf = match BlueprintWorkflowLibrary::parse(yaml) {
        Ok(w) => w,
        Err(_) => return (vec![], None),
    };

    let state = match wf.states.get(state_name) {
        Some(s) => s,
        None => return (vec![], None),
    };

    let kind = state.kind.as_ref().map(|k| {
        use crate::blueprint_workflows::StateKind;
        match k {
            StateKind::Deterministic => "deterministic",
            StateKind::Agent => "agent",
            StateKind::Human => "human",
            StateKind::Github => "github",
            StateKind::Function => "function",
            StateKind::Workflow => "workflow",
            StateKind::Terminal => "terminal",
            StateKind::GitHook => "git-hook",
            StateKind::Ci => "ci",
        }
        .to_string()
    });

    let transitions = state
        .next
        .as_ref()
        .map(|n| n.all_event_keys().iter().map(|s| s.to_string()).collect())
        .unwrap_or_default();

    (transitions, kind)
}

// ── Worktree-local path used only in tests ────────────────────────────────────

#[allow(dead_code)]
fn calypso_dir(cwd: &Path) -> PathBuf {
    cwd.join(".calypso")
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── route tests ──────────────────────────────────────────────────────────

    #[test]
    fn route_get_root_returns_html() {
        let tmp = std::env::temp_dir().join("calypso-webview-route-root");
        let (status, ct, body) = route("GET", "/", &[], &tmp);
        assert_eq!(status, "200 OK");
        assert!(ct.contains("text/html"));
        assert!(!body.is_empty());
    }

    #[test]
    fn route_get_index_html_returns_html() {
        let tmp = std::env::temp_dir().join("calypso-webview-route-index");
        let (status, ct, _body) = route("GET", "/index.html", &[], &tmp);
        assert_eq!(status, "200 OK");
        assert!(ct.contains("text/html"));
    }

    #[test]
    fn route_unknown_path_returns_404() {
        let tmp = std::env::temp_dir().join("calypso-webview-route-404");
        let (status, _ct, _body) = route("GET", "/not-found", &[], &tmp);
        assert_eq!(status, "404 Not Found");
    }

    #[test]
    fn route_get_api_state_returns_json() {
        let tmp = std::env::temp_dir().join("calypso-webview-api-state");
        std::fs::create_dir_all(tmp.join(".calypso")).unwrap();
        let (status, ct, body) = route("GET", "/api/state", &[], &tmp);
        assert_eq!(status, "200 OK");
        assert!(ct.contains("application/json"));
        let parsed: Value = serde_json::from_slice(&body).expect("should be valid JSON");
        assert!(parsed.get("cron_workflows").is_some());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn route_get_api_workflows_returns_json_array() {
        let tmp = std::env::temp_dir().join("calypso-webview-api-workflows");
        let (status, ct, body) = route("GET", "/api/workflows", &[], &tmp);
        assert_eq!(status, "200 OK");
        assert!(ct.contains("application/json"));
        let parsed: Value = serde_json::from_slice(&body).expect("should be valid JSON");
        assert!(parsed.is_array());
        let arr = parsed.as_array().unwrap();
        assert!(!arr.is_empty(), "expected at least one workflow");
        assert!(arr[0].get("name").is_some());
        assert!(arr[0].get("yaml").is_some());
    }

    #[test]
    fn route_post_trigger_writes_pending_event() {
        let tmp = std::env::temp_dir().join("calypso-webview-trigger");
        std::fs::create_dir_all(tmp.join(".calypso")).unwrap();
        let body = br#"{"event":"planning-task-identified"}"#;
        let (status, _, _) = route("POST", "/api/trigger", body, &tmp);
        assert_eq!(status, "200 OK");
        let written = std::fs::read_to_string(tmp.join(".calypso").join("pending-event.json"));
        assert!(written.is_ok());
        assert!(written.unwrap().contains("planning-task-identified"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn route_post_cron_now_writes_pending_cron() {
        let tmp = std::env::temp_dir().join("calypso-webview-cron-now");
        std::fs::create_dir_all(tmp.join(".calypso")).unwrap();
        let body = br#"{"workflow":"calypso-orchestrator-startup"}"#;
        let (status, _, _) = route("POST", "/api/cron-now", body, &tmp);
        assert_eq!(status, "200 OK");
        let written = std::fs::read_to_string(tmp.join(".calypso").join("pending-cron.json"));
        assert!(written.is_ok());
        assert!(written.unwrap().contains("calypso-orchestrator-startup"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── read_state_json tests ────────────────────────────────────────────────

    #[test]
    fn read_state_json_returns_valid_json_when_no_files() {
        let tmp = std::env::temp_dir().join("calypso-webview-state-no-files");
        std::fs::create_dir_all(tmp.join(".calypso")).unwrap();
        let json = read_state_json(&tmp);
        let parsed: Value = serde_json::from_str(&json).expect("should be valid JSON");
        assert!(parsed.get("workflow_state").is_some());
        assert!(parsed.get("feature_state").is_some());
        assert!(parsed.get("cron_workflows").is_some());
        assert!(parsed.get("active_transitions").is_some());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn read_state_json_includes_cron_workflows_from_embedded_library() {
        let tmp = std::env::temp_dir().join("calypso-webview-state-cron");
        std::fs::create_dir_all(tmp.join(".calypso")).unwrap();
        let json = read_state_json(&tmp);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        let crons = parsed["cron_workflows"].as_array().unwrap();
        // calypso-orchestrator-startup has a schedule block
        assert!(
            crons
                .iter()
                .any(|c| c["name"] == "calypso-orchestrator-startup"),
            "expected calypso-orchestrator-startup in cron workflows"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn read_state_json_parses_active_state_info_when_workflow_state_present() {
        let tmp = std::env::temp_dir().join("calypso-webview-state-active");
        std::fs::create_dir_all(tmp.join(".calypso")).unwrap();
        let workflow_state = serde_json::json!({
            "workflow": "calypso-default-feature-workflow",
            "state": "write-failing-tests"
        });
        std::fs::write(
            tmp.join(".calypso").join("workflow-state.json"),
            serde_json::to_string_pretty(&workflow_state).unwrap(),
        )
        .unwrap();
        let json = read_state_json(&tmp);
        let parsed: Value = serde_json::from_str(&json).unwrap();
        let transitions = parsed["active_transitions"].as_array().unwrap();
        assert!(
            !transitions.is_empty(),
            "expected transitions for write-failing-tests"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── collect_cron_workflows tests ─────────────────────────────────────────

    #[test]
    fn collect_cron_workflows_is_non_empty() {
        let crons = collect_cron_workflows();
        assert!(!crons.is_empty(), "expected at least one cron workflow");
    }

    #[test]
    fn collect_cron_workflows_entries_have_required_fields() {
        for entry in collect_cron_workflows() {
            assert!(entry.get("name").is_some(), "missing 'name' field");
            assert!(entry.get("cron").is_some(), "missing 'cron' field");
        }
    }

    // ── resolve_active_state_info tests ──────────────────────────────────────

    #[test]
    fn resolve_active_state_info_returns_empty_for_unknown_workflow() {
        let (transitions, kind) = resolve_active_state_info("no-such-workflow", "some-state");
        assert!(transitions.is_empty());
        assert!(kind.is_none());
    }

    #[test]
    fn resolve_active_state_info_returns_empty_for_unknown_state() {
        let (transitions, kind) =
            resolve_active_state_info("calypso-default-feature-workflow", "no-such-state");
        assert!(transitions.is_empty());
        assert!(kind.is_none());
    }

    #[test]
    fn resolve_active_state_info_returns_kind_for_known_state() {
        let (_transitions, kind) =
            resolve_active_state_info("calypso-default-feature-workflow", "write-failing-tests");
        assert!(kind.is_some(), "expected a kind for write-failing-tests");
    }

    // ── resolve_active_state_info_with_local tests ───────────────────────────

    #[test]
    fn resolve_active_state_info_with_local_falls_back_to_embedded_when_no_local_dir() {
        let tmp = std::env::temp_dir().join("calypso-webview-local-no-dir");
        let workflows_dir = tmp.join("no-such-dir");
        // No local dir — should fall back to embedded library.
        let (_transitions, kind) = resolve_active_state_info_with_local(
            &workflows_dir,
            "calypso-default-feature-workflow",
            "write-failing-tests",
        );
        assert!(kind.is_some());
    }

    #[test]
    fn resolve_active_state_info_with_local_matches_local_yml_file() {
        let tmp = std::env::temp_dir().join("calypso-webview-local-yml");
        let workflows_dir = tmp.join(".calypso").join("workflows");
        std::fs::create_dir_all(&workflows_dir).unwrap();

        // Write a minimal workflow YAML with a known state.
        let yaml = r#"
name: my-local-workflow
states:
  review:
    kind: human
    next:
      on:
        approve: deploy
        reject: review
  deploy:
    kind: deterministic
"#;
        std::fs::write(workflows_dir.join("my-local-workflow.yml"), yaml).unwrap();

        let (transitions, kind) =
            resolve_active_state_info_with_local(&workflows_dir, "my-local-workflow", "review");
        assert_eq!(kind.as_deref(), Some("human"));
        assert!(
            transitions.contains(&"approve".to_string()),
            "expected approve transition"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn resolve_active_state_info_with_local_skips_non_yaml_files() {
        let tmp = std::env::temp_dir().join("calypso-webview-local-skip");
        let workflows_dir = tmp.join(".calypso").join("workflows");
        std::fs::create_dir_all(&workflows_dir).unwrap();

        // Write a non-YAML file that should be ignored.
        std::fs::write(workflows_dir.join("readme.txt"), "not yaml").unwrap();
        // Write a YAML that does NOT match the requested workflow.
        let yaml = "name: other-workflow\nstates:\n  start:\n    kind: agent\n";
        std::fs::write(workflows_dir.join("other-workflow.yml"), yaml).unwrap();

        // Should fall back to embedded library since no match.
        let (transitions, kind) = resolve_active_state_info_with_local(
            &workflows_dir,
            "calypso-default-feature-workflow",
            "write-failing-tests",
        );
        assert!(kind.is_some(), "expected embedded library fallback");
        assert!(!transitions.is_empty() || kind.is_some());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── public_ip test ────────────────────────────────────────────────────────

    #[test]
    fn public_ip_does_not_panic_and_returns_valid_address() {
        // The function either returns a valid IPv4 address or None (e.g. in
        // offline environments). It must never panic.
        let ip = public_ip();
        if let Some(addr) = ip {
            // Must not be an unspecified address.
            assert!(!addr.is_unspecified(), "public_ip returned 0.0.0.0");
        }
    }

    // ── StateKind variants and parse-error path ───────────────────────────────

    #[test]
    fn resolve_active_state_info_with_local_covers_state_kind_variants() {
        use std::time::{SystemTime, UNIX_EPOCH};
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let tmp = std::env::temp_dir().join(format!("calypso-webview-kinds-{nanos}"));
        let workflows_dir = tmp.join(".calypso").join("workflows");
        std::fs::create_dir_all(&workflows_dir).unwrap();

        // An invalid YAML file — forces line 275 (parse-error continue path).
        std::fs::write(workflows_dir.join("aaa-broken.yml"), "{unclosed").unwrap();

        // Workflow with deterministic, agent, and github states.
        let yaml = "name: kind-test\nstates:\n  s1:\n    kind: deterministic\n  s2:\n    kind: agent\n  s3:\n    kind: github\n";
        std::fs::write(workflows_dir.join("kind-test.yml"), yaml).unwrap();

        let (_, k1) = resolve_active_state_info_with_local(&workflows_dir, "kind-test", "s1");
        assert_eq!(k1.as_deref(), Some("deterministic"));
        let (_, k2) = resolve_active_state_info_with_local(&workflows_dir, "kind-test", "s2");
        assert_eq!(k2.as_deref(), Some("agent"));
        let (_, k3) = resolve_active_state_info_with_local(&workflows_dir, "kind-test", "s3");
        assert_eq!(k3.as_deref(), Some("github"));

        let _ = std::fs::remove_dir_all(&tmp);
    }

    // ── handle_connection test ────────────────────────────────────────────────

    #[test]
    fn handle_connection_returns_html_for_get_root() {
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let tmp = std::env::temp_dir().join("calypso-webview-handle-conn");
        std::fs::create_dir_all(tmp.join(".calypso")).unwrap();

        // Bind a listener on a random port, connect a client, hand the server
        // side off to handle_connection(), then read the response.
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let cwd = tmp.clone();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, &cwd);
        });

        let mut client = std::net::TcpStream::connect(addr).unwrap();
        client
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();

        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        handle.join().unwrap();

        assert!(response.contains("200 OK"), "expected 200 OK in response");
        assert!(
            response.contains("text/html"),
            "expected text/html content type"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn handle_connection_parses_content_length_and_reads_body() {
        // Covers lines for Content-Length header parsing (line 69) and body
        // read_exact (line 76) inside handle_connection.
        use std::io::{Read, Write};
        use std::net::TcpListener;

        let tmp = std::env::temp_dir().join("calypso-webview-handle-conn-post");
        std::fs::create_dir_all(tmp.join(".calypso")).unwrap();

        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();

        let cwd = tmp.clone();
        let handle = std::thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            handle_connection(stream, &cwd);
        });

        let body = br#"{"event":"test"}"#;
        let request = format!(
            "POST /api/trigger HTTP/1.1\r\nHost: localhost\r\nContent-Length: {}\r\n\r\n",
            body.len()
        );

        let mut client = std::net::TcpStream::connect(addr).unwrap();
        client.write_all(request.as_bytes()).unwrap();
        client.write_all(body).unwrap();

        let mut response = String::new();
        client.read_to_string(&mut response).unwrap();
        handle.join().unwrap();

        assert!(
            response.contains("200 OK"),
            "expected 200 OK for POST trigger"
        );
        let _ = std::fs::remove_dir_all(&tmp);
    }
}
