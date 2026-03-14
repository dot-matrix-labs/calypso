# Development Guide

This guide covers local development, testing, and releasing the Calypso CLI.

## Prerequisites

- **Rust toolchain** — Install from [rustup.rs](https://rustup.rs/)
- **MSRV** — Calypso CLI targets Rust 1.75+ (check `rust-toolchain.toml` in the cli directory)
- **Git** — For version tracking and release tagging
- **Standard build tools** — For cross-platform compilation

### Verify Setup

```bash
rustc --version
cargo --version
git --version
```

## Building

### Development Build

```bash
cd cli
cargo build
./target/debug/calypso-cli --help
```

### Release Build

```bash
cd cli
cargo build --release
./target/release/calypso-cli --version
```

The version output includes the git hash: `0.1.0+a1b2c3`.

## Testing

### Run All Tests

```bash
cd cli
cargo test
```

### Run Tests for CLI Only

```bash
cargo test -p calypso-cli
```

### Run Specific Test

```bash
cargo test --lib <test_name>
```

### Run with Logging

```bash
RUST_LOG=debug cargo test -- --nocapture
```

## Running the CLI

### From Development Build

```bash
./target/debug/calypso-cli <command> [args]
```

### From Release Build

```bash
./target/release/calypso-cli <command> [args]
```

### With Environment Variables

```bash
CALYPSO_LOG_LEVEL=debug ./target/release/calypso-cli <command>
```

## Code Quality

### Linting

```bash
cd cli
cargo clippy --all-targets --all-features -- -D warnings
```

### Formatting

```bash
cd cli
cargo fmt --check          # Check formatting
cargo fmt                  # Auto-format code
```

### Type Checking

```bash
cd cli
cargo check
```

## Cross-Platform Compilation

The release process builds for multiple platforms. To test cross-platform builds locally:

### Linux x86_64
```bash
cd cli
cargo build --release --target x86_64-unknown-linux-gnu
```

### Linux aarch64
```bash
cd cli
cargo build --release --target aarch64-unknown-linux-gnu
# Requires: sudo apt-get install gcc-aarch64-linux-gnu
```

### macOS x86_64
```bash
cd cli
cargo build --release --target x86_64-apple-darwin
```

### macOS aarch64
```bash
cd cli
cargo build --release --target aarch64-apple-darwin
```

## Releasing

### Semantic Versioning

Calypso CLI uses semantic versioning: `MAJOR.MINOR.PATCH`, e.g., `0.1.0`.

**Version numbering:**
- Tags are plain semver without `v` prefix (e.g., `0.1.0`, not `v0.1.0`)
- Binaries embed git hash automatically (e.g., `0.1.0+a1b2c3`)
- Canary releases use pre-release tag: `0.1.0-canary+a1b2c3`

### Release Checklist

1. **Update version in `cli/Cargo.toml`:**
   ```toml
   [package]
   version = "0.2.0"
   ```

2. **Create annotated git tag** (plain semver, no `v` prefix):
   ```bash
   git tag -a 0.2.0 -m "Release version 0.2.0"
   ```

3. **Push tag to trigger CI/CD:**
   ```bash
   git push origin 0.2.0
   ```

4. **Monitor GitHub Actions:**
   - Go to [Actions](https://github.com/dot-matrix-labs/calypso/actions)
   - Watch "Release — cli" workflow
   - Verify builds complete for all 4 platforms
   - Confirm GitHub Release is published with binaries and checksums

5. **Post-Release:**
   - CI/CD automatically creates a follow-up commit bumping `cli/Cargo.toml` version
   - Verify the bump commit appears on main branch

### Canary Releases

For pre-release testing:

```bash
git tag -a 0.2.0-canary+abc123 -m "Canary release for 0.2.0"
git push origin 0.2.0-canary+abc123
```

Users can install with: `curl install.sh | bash -s -- canary`

### Verifying a Release

After the release workflow completes:

1. Visit [GitHub Releases](https://github.com/dot-matrix-labs/calypso/releases)
2. Verify release contains:
   - 4 binary artifacts: `calypso-cli-{linux,macos}-{x86_64,aarch64}-0.2.0.tar.gz`
   - 4 SHA256 checksums: `calypso-cli-*.tar.gz.sha256`
   - Combined checksums file: `CHECKSUMS.txt`
   - Installation script: `install.sh`
   - GitHub SLSA provenance signatures (visible on release page)

3. Test installation:
   ```bash
   curl -fsSL https://github.com/dot-matrix-labs/calypso/releases/download/0.2.0/install.sh | bash
   calypso-cli --version  # Should show 0.2.0+<hash>
   ```

## Build Metadata

The `cli/build.rs` script captures:

- **Git hash** (6 chars) → `CALYPSO_BUILD_GIT_HASH`
- **Git tags at HEAD** → `CALYPSO_BUILD_GIT_TAGS`
- **Build timestamp** (UTC) → `CALYPSO_BUILD_TIME`

These are embedded at compile-time and accessible via:
```rust
const GIT_HASH: &str = env!("CALYPSO_BUILD_GIT_HASH");
const GIT_TAGS: &str = env!("CALYPSO_BUILD_GIT_TAGS");
const BUILD_TIME: &str = env!("CALYPSO_BUILD_TIME");
```

## Troubleshooting

### Build Fails on macOS

Ensure Xcode command-line tools are installed:
```bash
xcode-select --install
```

### Cross-compilation Fails

For aarch64 Linux builds on x86_64 systems, install the cross-compilation toolchain:
```bash
sudo apt-get install gcc-aarch64-linux-gnu
```

### Tests Fail

Clear build cache and rebuild:
```bash
cd cli
cargo clean
cargo test
```

### Installation Script Fails

Verify the release was published correctly:
1. Check GitHub Release page has the expected binaries
2. Verify checksum files exist and are readable
3. Check network connectivity to GitHub releases CDN

## CI/CD Pipeline

Release process is automated via `.github/workflows/release-cli.yml`:

1. **Trigger:** Git tag matching semver pattern `[0-9]+.[0-9]+.[0-9]+*`
2. **Build:** Compiles for 4 platforms in parallel
3. **Package:** Creates tar.gz archives with SHA256 checksums
4. **Publish:** Creates GitHub Release with artifacts
5. **Post-Release:** Bumps `cli/Cargo.toml` version on main branch

See [release-cli.yml](.github/workflows/release-cli.yml) for full workflow definition.
