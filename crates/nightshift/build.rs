fn main() {
    // Blueprint workflow files embedded at compile time.
    // Paths are relative to this crate's Cargo.toml (crates/nightshift/).
    println!(
        "cargo:rerun-if-changed=../../calypso-blueprint/examples/workflows/calypso-default-deployment-workflow.yaml"
    );
    println!(
        "cargo:rerun-if-changed=../../calypso-blueprint/examples/workflows/calypso-default-feature-workflow.yaml"
    );
    println!(
        "cargo:rerun-if-changed=../../calypso-blueprint/examples/workflows/calypso-deployment-request.yaml"
    );
    println!(
        "cargo:rerun-if-changed=../../calypso-blueprint/examples/workflows/calypso-feature-request.yaml"
    );
    println!(
        "cargo:rerun-if-changed=../../calypso-blueprint/examples/workflows/calypso-implementation-loop.yaml"
    );
    println!(
        "cargo:rerun-if-changed=../../calypso-blueprint/examples/workflows/calypso-orchestrator-startup.yaml"
    );
    println!(
        "cargo:rerun-if-changed=../../calypso-blueprint/examples/workflows/calypso-planning.yaml"
    );
    println!(
        "cargo:rerun-if-changed=../../calypso-blueprint/examples/workflows/calypso-pr-review-merge.yaml"
    );
    println!(
        "cargo:rerun-if-changed=../../calypso-blueprint/examples/workflows/calypso-release-request.yaml"
    );
    println!(
        "cargo:rerun-if-changed=../../calypso-blueprint/examples/workflows/calypso-save-state.yaml"
    );
    println!("cargo:rustc-check-cfg=cfg(coverage)");
}
