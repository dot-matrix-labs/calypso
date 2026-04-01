# Calypso CLI

The command-line interface for Calypso, built in Rust.

## Prerequisites

- **Rust 1.94.0+** — install via [rustup](https://rustup.rs/)
- **Git** — for cloning the repository

## Running

`cargo run` works from both the repository root and the `crates/calypso-cli/` directory — the workspace `Cargo.toml` at the repo root sets `default-members = ["crates/calypso-cli"]` and the crate's `default-run = "calypso-cli"`, so no `--bin` flag is needed.

```sh
# From the repository root or crates/calypso-cli/ directory:
cargo run                              # start daemon from current directory
cargo run -- --path ./my-project       # start daemon for a specific project directory
cargo run -- --path /abs/path/to/proj  # absolute path also works
cargo run -- --path ~/projects/calypso # tilde paths also work

cargo run -- doctor              # run prerequisite checks
cargo run -- status              # print feature gate summary

# Release builds work the same way:
cargo run --release              # start daemon (optimized build)
cargo run --release -- doctor    # run doctor (optimized build)
```

## Path argument

Use `--path <dir>` (or `-p <dir>`) to specify the project directory. If omitted, the current working directory is used. All invocations that do not include `--select-flow` route to the headless workflow daemon regardless of how the directory was supplied.

```sh
calypso                        # daemon uses $PWD
calypso --path ./my-project    # daemon uses ./my-project
calypso --path /abs/path       # daemon uses /abs/path
```

The positional `[path]` argument form (`calypso ./my-dir`) was removed in favour of the explicit `--path` flag. Using a bare path as a positional argument previously activated a legacy code path; it now has no special meaning and falls through to the help output.

## Commands

| Command | Description |
|---------|-------------|
| `calypso` | Start the headless workflow daemon (uses cwd) |
| `calypso --path <dir>` | Start the daemon for a specific project directory |
| `calypso doctor` | Run prerequisite health checks |
| `calypso doctor --fix <check-id>` | Apply fix for a specific check |
| `calypso status` | Print feature gate summary |
| `calypso status --state <file>` | Open interactive TUI from state file |
| `calypso status --state <file> --headless` | Render operator surface without TUI |
| `calypso state show` | Print current state as JSON |
| `calypso init` | Initialize repository for Calypso |
| `calypso watch` | Live-reload TUI from cwd state file |
| `calypso watch --state <file>` | Live-reload TUI from a specific state file |
| `calypso feature-start <id> --worktree-base <path>` | Start a new feature worktree |
| `calypso run <id> --role <role>` | Run an agent session |
| `calypso template validate` | Validate template coherence |
| `calypso --version` / `-v` | Print version information |

## Building from Source

```bash
git clone https://github.com/dot-matrix-labs/calypso.git
cd calypso
cargo build --release
```

The binary will be at `target/release/calypso-cli`.

To install globally:
```bash
cargo install --path crates/calypso-cli
```

## Development

The project defines cargo aliases in `.cargo/config.toml` for common tasks.
All commands below should be run from the `crates/calypso-cli/` directory.

### Testing

```bash
cargo test               # run all tests
cargo test-unit          # unit tests only (--lib)
cargo test-integration   # integration tests (cli, doctor, github, state, templates)
cargo test-e2e           # end-to-end tests (--nocapture)
```

### Code Quality

```bash
cargo test --test e2e -- --nocapture
```

**Run PTY-based TUI end-to-end tests:**
```bash
cargo test --test e2e_tui -- --ignored --nocapture
```

PTY tests spawn the binary in a real pseudo-terminal using `expectrl`, send
keystrokes, and assert on rendered screen output.  They are marked `#[ignore]`
because they require a real TTY (not available under all CI runners or coverage
tools) and are gated behind `#[cfg(unix)]`.

### Code Quality

```bash
cargo lint         # clippy with strict warnings (-D warnings)
cargo fmt-check    # verify formatting (rustfmt --check)
cargo build-check  # ensure all targets compile
```

### Code Coverage

```bash
cargo coverage
```

This runs the `coverage-driver` binary (requires the `__dev_only` feature).
Coverage reports are saved to `lcov.info`.

### Debugging

Run the CLI locally for testing:
```bash
cargo run -- --help
```

## Architecture

See [../docs/prd.md](../docs/prd.md) for the full product requirements document.

## License

Same as parent Calypso project.
