//! Embedded content for the six required GitHub Actions workflow files.
//! These are the canonical Calypso CI workflow templates.

pub const RUST_QUALITY: &str = r#"name: Rust quality — cli

on:
  push:
    branches: [main, "feat/**"]
  pull_request:
  merge_group:
  workflow_dispatch:

jobs:
  format:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v5

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy

      - name: Cache cargo artifacts
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            crates/calypso-cli -> target

      - name: Check formatting
        working-directory: crates/calypso-cli
        run: cargo fmt --check

  clippy:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v5

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          components: clippy

      - name: Cache cargo artifacts
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            crates/calypso-cli -> target

      - name: Run clippy
        working-directory: crates/calypso-cli
        run: cargo clippy --all-targets -- -D warnings

  build:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v5

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo artifacts
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            crates/calypso-cli -> target

      - name: Build
        working-directory: crates/calypso-cli
        run: cargo build
"#;

pub const RUST_UNIT: &str = r#"name: Rust unit tests — cli

on:
  push:
    branches: [main, "feat/**"]
  pull_request:
  merge_group:
  schedule:
    - cron: "15 1 * * *"
  workflow_dispatch:

jobs:
  unit:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v5

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo artifacts
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            crates/calypso-cli -> target

      - name: Run unit tests
        working-directory: crates/calypso-cli
        run: cargo test --lib
"#;

pub const RUST_INTEGRATION: &str = r#"name: Rust integration tests — cli

on:
  push:
    branches: [main, "feat/**"]
  pull_request:
  merge_group:
  schedule:
    - cron: "25 1 * * *"
  workflow_dispatch:

jobs:
  integration:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v5

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo artifacts
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            crates/calypso-cli -> target

      - name: Run integration tests
        working-directory: crates/calypso-cli
        run: cargo test --test '*'
"#;

pub const RUST_E2E: &str = r#"name: Rust end-to-end tests — cli

on:
  push:
    branches: [main, "feat/**"]
  pull_request:
  merge_group:
  schedule:
    - cron: "35 1 * * *"
  workflow_dispatch:

jobs:
  e2e:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v5

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo artifacts
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            crates/calypso-cli -> target

      - name: Run end-to-end tests
        working-directory: crates/calypso-cli
        run: cargo test --test '*' -- --ignored
"#;

pub const RUST_COVERAGE: &str = r#"name: Rust coverage — cli

on:
  push:
    branches: [main, "feat/**"]
  pull_request:
  merge_group:
  workflow_dispatch:

jobs:
  coverage:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v5

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Install cargo-llvm-cov
        uses: taiki-e/install-action@cargo-llvm-cov

      - name: Cache cargo artifacts
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            crates/calypso-cli -> target

      - name: Run coverage
        working-directory: crates/calypso-cli
        run: cargo llvm-cov --lcov --output-path lcov.info

      - name: Upload coverage artifact
        uses: actions/upload-artifact@v6
        with:
          name: cli-coverage-lcov
          path: crates/calypso-cli/lcov.info
"#;

pub const RELEASE_CLI: &str = r#"name: Release — cli

on:
  push:
    tags:
      - "[0-9]+.[0-9]+.[0-9]+*"
  workflow_dispatch:

jobs:
  build:
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: ubuntu-latest
            target: x86_64-unknown-linux-gnu
            artifact_name: calypso-cli-linux-x86_64
            binary_name: calypso-cli
          - os: macos-latest
            target: x86_64-apple-darwin
            artifact_name: calypso-cli-macos-x86_64
            binary_name: calypso-cli
          - os: macos-latest
            target: aarch64-apple-darwin
            artifact_name: calypso-cli-macos-aarch64
            binary_name: calypso-cli

    runs-on: ${{ matrix.os }}

    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0

      - name: Set up Rust
        uses: dtolnay/rust-toolchain@stable
        with:
          targets: ${{ matrix.target }}

      - name: Cache cargo artifacts
        uses: Swatinem/rust-cache@v2
        with:
          workspaces: |
            crates/calypso-cli -> target

      - name: Build release binary
        working-directory: crates/calypso-cli
        run: cargo build --release --target ${{ matrix.target }}

      - name: Package release artifact
        shell: bash
        env:
          VERSION: ${{ github.ref_name }}
        run: |
          mkdir -p dist
          cp crates/calypso-cli/target/${{ matrix.target }}/release/${{ matrix.binary_name }} dist/${{ matrix.binary_name }}
          tar -czf dist/${{ matrix.artifact_name }}-${VERSION}.tar.gz -C dist ${{ matrix.binary_name }}
          cd dist
          sha256sum ${{ matrix.artifact_name }}-${VERSION}.tar.gz > ${{ matrix.artifact_name }}-${VERSION}.tar.gz.sha256

      - name: Upload packaged artifact
        uses: actions/upload-artifact@v6
        with:
          name: ${{ matrix.artifact_name }}
          path: |
            dist/${{ matrix.artifact_name }}-${{ github.ref_name }}.tar.gz
            dist/${{ matrix.artifact_name }}-${{ github.ref_name }}.tar.gz.sha256

  publish:
    if: startsWith(github.ref, 'refs/tags/')
    needs: build
    runs-on: ubuntu-latest
    permissions:
      contents: write

    steps:
      - uses: actions/checkout@v5
        with:
          fetch-depth: 0
          token: ${{ secrets.GITHUB_TOKEN }}

      - name: Download build artifacts
        uses: actions/download-artifact@v4
        with:
          path: dist

      - name: Publish GitHub release
        env:
          GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
        shell: bash
        run: |
          mkdir -p release-assets
          find dist -name "*.tar.gz" -o -name "*.sha256" | while read file; do
            cp "$file" release-assets/
          done
          cat release-assets/*.sha256 > release-assets/CHECKSUMS.txt
          gh release create "${{ github.ref_name }}" \
            --title "${{ github.ref_name }}" \
            --notes "Automated CLI release for ${{ github.ref_name }}" \
            release-assets/*
"#;

/// Returns the canonical content for a required workflow file by filename.
pub fn content_for(filename: &str) -> Option<&'static str> {
    match filename {
        "rust-quality.yml" => Some(RUST_QUALITY),
        "rust-unit.yml" => Some(RUST_UNIT),
        "rust-integration.yml" => Some(RUST_INTEGRATION),
        "rust-e2e.yml" => Some(RUST_E2E),
        "rust-coverage.yml" => Some(RUST_COVERAGE),
        "release-cli.yml" => Some(RELEASE_CLI),
        _ => None,
    }
}
