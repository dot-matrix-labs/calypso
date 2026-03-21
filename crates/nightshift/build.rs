use std::path::{Path, PathBuf};

fn main() {
    let blueprint_root = find_blueprint_root().expect("calypso-blueprint examples must exist");

    for relative in [
        "examples/workflows/calypso-default-deployment-workflow.yaml",
        "examples/workflows/calypso-default-feature-workflow.yaml",
        "examples/workflows/calypso-deployment-request.yaml",
        "examples/workflows/calypso-feature-request.yaml",
        "examples/workflows/calypso-implementation-loop.yaml",
        "examples/workflows/calypso-orchestrator-startup.yaml",
        "examples/workflows/calypso-planning.yaml",
        "examples/workflows/calypso-pr-review-merge.yaml",
        "examples/workflows/calypso-release-request.yaml",
        "examples/workflows/calypso-save-state.yaml",
        "examples/github-workflows/rust-quality.yml",
        "examples/github-workflows/rust-unit.yml",
        "examples/github-workflows/rust-integration.yml",
        "examples/github-workflows/rust-e2e.yml",
        "examples/github-workflows/rust-coverage.yml",
        "examples/github-workflows/release-cli.yml",
        "examples/github-workflows/merge-queue.yml",
        "examples/github-workflows/pr-issue-checklist.yml",
        "examples/github-workflows/pr-conflicts.yml",
        "examples/github-workflows/pr-single-issue.yml",
    ] {
        println!(
            "cargo:rerun-if-changed={}",
            blueprint_root.join(relative).display()
        );
    }

    println!(
        "cargo:rustc-env=CALYPSO_BLUEPRINT_ROOT={}",
        blueprint_root.display()
    );
    println!("cargo:rustc-check-cfg=cfg(coverage)");
}

fn find_blueprint_root() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").ok()?);
    manifest_dir
        .ancestors()
        .map(Path::to_path_buf)
        .find(|ancestor| {
            ancestor
                .join("calypso-blueprint/examples/workflows/calypso-planning.yaml")
                .exists()
        })
        .map(|ancestor| ancestor.join("calypso-blueprint"))
}
