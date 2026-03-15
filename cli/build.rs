use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=.git/HEAD");

    // Blueprint workflow files embedded at compile time.
    println!("cargo:rerun-if-changed=../.gitmodules");
    println!("cargo:rerun-if-changed=../calypso-blueprint/examples/workflows/calypso-default-deployment-workflow.yaml");
    println!("cargo:rerun-if-changed=../calypso-blueprint/examples/workflows/calypso-default-feature-workflow.yaml");
    println!("cargo:rerun-if-changed=../calypso-blueprint/examples/workflows/calypso-deployment-request.yaml");
    println!("cargo:rerun-if-changed=../calypso-blueprint/examples/workflows/calypso-feature-request.yaml");
    println!("cargo:rerun-if-changed=../calypso-blueprint/examples/workflows/calypso-implementation-loop.yaml");
    println!("cargo:rerun-if-changed=../calypso-blueprint/examples/workflows/calypso-orchestrator-startup.yaml");
    println!("cargo:rerun-if-changed=../calypso-blueprint/examples/workflows/calypso-planning.yaml");
    println!("cargo:rerun-if-changed=../calypso-blueprint/examples/workflows/calypso-pr-review-merge.yaml");
    println!("cargo:rerun-if-changed=../calypso-blueprint/examples/workflows/calypso-release-request.yaml");
    println!("cargo:rerun-if-changed=../calypso-blueprint/examples/workflows/calypso-save-state.yaml");
    println!("cargo:rerun-if-env-changed=SOURCE_DATE_EPOCH");
    println!("cargo:rustc-check-cfg=cfg(coverage)");

    let git_hash =
        run_git(["rev-parse", "--short=6", "HEAD"]).unwrap_or_else(|| "unknown".to_string());
    let git_tags = run_git(["tag", "--points-at", "HEAD"])
        .map(|tags| {
            let trimmed = tags
                .lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .collect::<Vec<_>>()
                .join(",");
            if trimmed.is_empty() {
                "none".to_string()
            } else {
                trimmed
            }
        })
        .unwrap_or_else(|| "none".to_string());
    let build_time = run_date().unwrap_or_else(|| "unknown".to_string());

    println!("cargo:rustc-env=CALYPSO_BUILD_GIT_HASH={git_hash}");
    println!("cargo:rustc-env=CALYPSO_BUILD_GIT_TAGS={git_tags}");
    println!("cargo:rustc-env=CALYPSO_BUILD_TIME={build_time}");
}

fn run_git<const N: usize>(args: [&str; N]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    Some(value.trim().to_string())
}

fn run_date() -> Option<String> {
    let output = Command::new("date")
        .args(["-u", "+%Y-%m-%dT%H:%M:%SZ"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let value = String::from_utf8(output.stdout).ok()?;
    Some(value.trim().to_string())
}
