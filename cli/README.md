# Calypso CLI

The command-line interface for Calypso, built in Rust.

## Prerequisites

- **Rust 1.94.0+** — install via [rustup](https://rustup.rs/)
- **Git** — for cloning the repository

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

### Common Tasks

**Run unit tests:**
```bash
cargo test --lib
```

**Run integration tests:**
```bash
cargo test --test '*'
```

**Run end-to-end tests:**
```bash
cargo test --test e2e -- --nocapture
```

**Run all tests:**
```bash
cargo test
```

**Check code quality:**
```bash
cargo lint    # clippy with strict warnings
cargo fmt-check  # format check
cargo build-check  # ensure all targets compile
```

**Code coverage:**
```bash
cargo coverage
```

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
