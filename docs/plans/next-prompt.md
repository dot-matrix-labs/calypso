Continue PR #27 on branch `feat/cli-feature-start`.

Current state:
- `calypso-cli` now has a `feature-start <feature-id> --worktree-base <path>` command wired in [cli/src/main.rs](/tmp/calypso-worktrees/feat-cli-feature-start/cli/src/main.rs).
- The orchestration lives in [cli/src/feature_start.rs](/tmp/calypso-worktrees/feat-cli-feature-start/cli/src/feature_start.rs) and covers branch naming, clean-`main` validation, branch/worktree creation, branch push, draft PR creation, state bootstrap, and rollback/recovery handling.
- New tests live in [cli/tests/feature_start.rs](/tmp/calypso-worktrees/feat-cli-feature-start/cli/tests/feature_start.rs), plus the CLI help assertion in [cli/tests/cli.rs](/tmp/calypso-worktrees/feat-cli-feature-start/cli/tests/cli.rs).
- [cli/Cargo.lock](/tmp/calypso-worktrees/feat-cli-feature-start/cli/Cargo.lock) was reconciled to offline-cached crate versions (`zmij 1.0.19`, `ryu 1.0.22`, `libc 0.2.180`, `syn 2.0.114`, `unicode-ident 1.0.22`, `quote 1.0.44`, `bitflags 2.10.0`, `memchr 2.7.6`) so Cargo can resolve dependencies without network access.

Blocking issue:
- `RUSTC_WRAPPER= cargo test -p calypso-cli --offline` now resolves and starts compiling, but Rust fails while writing artifacts with `Invalid cross-device link (os error 18)`, e.g. `failed to write .../liblibc-*.rmeta`.
- This still happens with:
  - `TMPDIR=/tmp/calypso-cli-target/tmp CARGO_TARGET_DIR=/tmp/calypso-cli-target`
  - `TMPDIR=$(pwd)/target/tmp CARGO_TARGET_DIR=$(pwd)/target`
  - `-j1`

Next steps:
1. Solve the compiler artifact-write `EXDEV` issue in this sandbox so `cargo test -p calypso-cli --offline` can complete.
2. Re-run `RUSTC_WRAPPER= cargo fmt --check` and `RUSTC_WRAPPER= cargo test -p calypso-cli --offline`.
3. If tests pass, update PR #27 body with a checklist showing implemented items and remaining validation status.
