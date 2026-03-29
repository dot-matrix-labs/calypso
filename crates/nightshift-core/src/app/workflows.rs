/// Return a newline-separated list of all embedded blueprint workflow name stems.
pub fn run_workflows_list() -> String {
    crate::blueprint_workflows::BlueprintWorkflowLibrary::list()
        .iter()
        .map(|(stem, _)| *stem)
        .collect::<Vec<_>>()
        .join("\n")
}

/// Return the raw YAML content for a named workflow, or an error message.
pub fn run_workflows_show(name: &str) -> Result<String, String> {
    crate::blueprint_workflows::BlueprintWorkflowLibrary::get(name)
        .map(|yaml| yaml.to_string())
        .ok_or_else(|| format!("workflow not found: {name}"))
}

/// Parse the named workflow and return `Ok("OK")` or `Err(parse_error_string)`.
pub fn run_workflows_validate(name: &str) -> Result<String, String> {
    let yaml = crate::blueprint_workflows::BlueprintWorkflowLibrary::get(name)
        .ok_or_else(|| format!("workflow not found: {name}"))?;
    crate::blueprint_workflows::BlueprintWorkflowLibrary::parse(yaml)
        .map(|_| "OK".to_string())
        .map_err(|e| e.to_string())
}
