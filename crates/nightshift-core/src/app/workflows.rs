use calypso_workflows::WorkflowCatalog;
use std::path::Path;

/// Return a newline-separated list of all effective workflow names for the repository.
pub fn run_workflows_list(cwd: &Path) -> String {
    WorkflowCatalog::load(cwd)
        .entries()
        .iter()
        .map(|entry| entry.handle.display_name())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Return the raw YAML content for a named workflow, or an error message.
pub fn run_workflows_show(cwd: &Path, name: &str) -> Result<String, String> {
    WorkflowCatalog::load(cwd)
        .find(name)
        .map(|entry| entry.yaml.clone())
        .ok_or_else(|| format!("workflow not found: {name}"))
}

/// Parse the named workflow and return `Ok("OK")` or `Err(parse_error_string)`.
pub fn run_workflows_validate(cwd: &Path, name: &str) -> Result<String, String> {
    let catalog = WorkflowCatalog::load(cwd);
    let entry = catalog
        .find(name)
        .ok_or_else(|| format!("workflow not found: {name}"))?;
    entry
        .parse()
        .map(|_| "OK".to_string())
        .map_err(|e| e.to_string())
}
