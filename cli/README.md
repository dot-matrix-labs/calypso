# Calypso CLI

The command-line interface for Calypso, built in Rust.

## Prerequisites

- **Rust 1.94.0+** — install via [rustup](https://rustup.rs/)
- **Git** — for cloning the repository

## Running

```sh
cargo run                        # launch TUI from current directory
cargo run -- ./my-project        # launch TUI for a specific project directory
cargo run -- /abs/path/to/proj   # absolute path also works
cargo run -- ~/projects/calypso  # tilde paths also work

cargo run -- doctor              # run prerequisite checks
cargo run -- status              # print feature gate summary
cargo run -- watch               # live-reload TUI from current directory
```

`cargo run` defaults to the `calypso-cli` binary (via `default-run` in `Cargo.toml`), so no `--bin` flag is needed.

## Path argument

`calypso [path]` accepts an optional project directory. If the argument looks like a path (starts with `.`, `/`, or `~`, or is an existing directory), Calypso launches the TUI for that project's `.calypso/state.json`. If no argument is given, the current working directory is used.

```sh
calypso                  # uses $PWD/.calypso/state.json
calypso ./my-project     # uses ./my-project/.calypso/state.json
calypso /abs/path        # uses /abs/path/.calypso/state.json
```

## Commands

| Command | Description |
|---------|-------------|
| `calypso [path]` | Launch TUI for a project directory (defaults to cwd) |
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
cd calypso/cli
cargo build --release
```

The binary will be at `target/release/calypso-cli`.

To install globally:
```bash
cargo install --path .
```

## Development

The project defines cargo aliases in `.cargo/config.toml` for common tasks.
All commands below should be run from the `cli/` directory.

### Testing

```bash
cargo test               # run all tests
cargo test-unit          # unit tests only (--lib)
cargo test-integration   # integration tests (cli, doctor, github, state, templates)
cargo test-e2e           # end-to-end tests (--nocapture)
```

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

See [spec.md](./spec.md) for the full specification.

## License

Same as parent Calypso project.
