Resume the CLI hook policy enforcement PR by unblocking Cargo dependency resolution in this worktree, then run `cargo test -p calypso-cli` from `cli/`.

If the suite fails, fix exactly one failing test-first issue in the new policy-gate code before changing anything else. If the suite passes, update the PR body/checklist to reflect the completed policy-gate work, then commit the current slice with the synchronized planning docs.
