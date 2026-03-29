mod helpers;

use helpers::spawned_calypso::spawned_calypso;

#[test]
fn local_workflows_list_overrides_embedded_defaults() {
    let turnstile = include_str!("fixtures/workflows/turnstile.yaml");
    let out = spawned_calypso()
        .args(["workflows", "list"])
        .calypso_file("turnstile.yaml", turnstile)
        .run();

    assert_eq!(out.exit_code, 0, "stderr:\n{}", out.stderr);
    assert!(
        out.stdout.contains("turnstile.yaml"),
        "expected local workflow file in list; stdout:\n{}",
        out.stdout
    );

    for embedded in &[
        "calypso-orchestrator-startup",
        "calypso-planning",
        "calypso-implementation-loop",
    ] {
        assert!(
            !out.stdout.contains(embedded),
            "embedded workflow '{embedded}' must not appear when local workflows exist; stdout:\n{}",
            out.stdout
        );
    }
}

#[test]
fn workflows_validate_accepts_local_file_names() {
    let turnstile = include_str!("fixtures/workflows/turnstile.yaml");
    let out = spawned_calypso()
        .args(["workflows", "validate", "turnstile.yaml"])
        .calypso_file("turnstile.yaml", turnstile)
        .run();

    assert_eq!(out.exit_code, 0, "stderr:\n{}", out.stderr);
    assert_eq!(out.stdout.trim(), "OK", "stdout:\n{}", out.stdout);
}
