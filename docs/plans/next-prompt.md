Continue the `feat/cli-github-gate-evaluators` worktree from the current state.

Start with validation. Run:

- `RUSTC_WRAPPER= cargo test -p calypso-cli --test github -- --nocapture`
- `RUSTC_WRAPPER= cargo test -p calypso-cli --test state -- --nocapture`
- `RUSTC_WRAPPER= cargo test -p calypso-cli --test app -- --nocapture`
- `RUSTC_WRAPPER= cargo test -p calypso-cli --test tui -- --nocapture`

If `cargo` still fails before compilation, resolve the dependency-cache mismatch called out in [docs/plans/implementation-plan.md](/tmp/calypso-worktrees/feat-cli-github-gate-evaluators/docs/plans/implementation-plan.md) without undoing the GitHub gate evaluator changes.

Once validation is green:

1. Re-read the live PR body and update it with a checklist that reflects the implemented GitHub evaluator behavior.
2. Verify the default template gate layout and operator-surface output still match [docs/plans/cli-github-gate-evaluators-feature.md](/tmp/calypso-worktrees/feat-cli-github-gate-evaluators/docs/plans/cli-github-gate-evaluators-feature.md).
3. Commit the finished increment(s) in small units.
