#!/bin/bash
# Calypso CLI Installation Script
# Usage: curl https://github.com/dot-matrix-labs/calypso/releases/download/latest/install.sh | bash
#        curl install.sh | bash -s -- 0.1.0           # Install specific version
#        curl install.sh | bash -s -- canary          # Install latest canary

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# Configuration
REPO="dot-matrix-labs/calypso"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"
BINARY_NAME="calypso-cli"
TMP_DIR=$(mktemp -d)
trap "rm -rf ${TMP_DIR}" EXIT

# Detect OS
detect_os() {
    case "$(uname -s)" in
        Linux*)     echo "linux";;
        Darwin*)    echo "macos";;
        *)          echo "unsupported";;
    esac
}

# Detect architecture
detect_arch() {
    case "$(uname -m)" in
        x86_64)     echo "x86_64";;
        arm64|aarch64) echo "aarch64";;
        *)          echo "unsupported";;
    esac
}

# Log functions
log_info() {
    echo -e "${GREEN}→${NC} $1"
}

log_error() {
    echo -e "${RED}✗${NC} $1" >&2
}

log_warn() {
    echo -e "${YELLOW}⚠${NC} $1"
}

# Main installation
main() {
    local os=$(detect_os)
    local arch=$(detect_arch)

    if [ "$os" = "unsupported" ] || [ "$arch" = "unsupported" ]; then
        log_error "Unsupported OS or architecture: $os/$arch"
        exit 1
    fi

    log_info "Detected OS: $os, Architecture: $arch"

    # Determine version to install
    local version="${1:-latest}"
    if [ "$version" = "canary" ]; then
        # Fetch latest canary release
        version=$(get_latest_canary_version)
        if [ -z "$version" ]; then
            log_error "No canary releases found"
            exit 1
        fi
        log_info "Installing canary version: $version"
    elif [ "$version" = "latest" ]; then
        # Fetch latest production release
        version=$(get_latest_production_version)
        if [ -z "$version" ]; then
            log_error "No releases found"
            exit 1
        fi
        log_info "Installing latest version: $version"
    else
        log_info "Installing pinned version: $version"
    fi

    # Construct artifact name and download URL
    local artifact_name="calypso-cli-${os}-${arch}"
    local download_url="https://github.com/${REPO}/releases/download/${version}/${artifact_name}-${version}.tar.gz"
    local checksum_url="https://github.com/${REPO}/releases/download/${version}/${artifact_name}-${version}.tar.gz.sha256"

    log_info "Downloading from: $download_url"

    # Download binary archive
    if ! curl -fsSL "$download_url" -o "${TMP_DIR}/${artifact_name}.tar.gz"; then
        log_error "Failed to download binary"
        exit 1
    fi

    # Download and verify checksum
    log_info "Verifying checksum..."
    if ! curl -fsSL "$checksum_url" -o "${TMP_DIR}/${artifact_name}.sha256"; then
        log_warn "Could not download checksum file, skipping verification"
    else
        # Verify checksum
        cd "${TMP_DIR}"
        if ! sha256sum -c "${artifact_name}.sha256" >/dev/null 2>&1; then
            log_error "Checksum verification failed"
            exit 1
        fi
        log_info "Checksum verified"
    fi

    # Extract binary
    log_info "Extracting binary..."
    tar -xzf "${TMP_DIR}/${artifact_name}.tar.gz" -C "${TMP_DIR}"

    if [ ! -f "${TMP_DIR}/${BINARY_NAME}" ]; then
        log_error "Binary not found in archive"
        exit 1
    fi

    # Check if we need sudo
    need_sudo=false
    if [ ! -w "$INSTALL_DIR" ]; then
        need_sudo=true
        log_warn "Root permissions required to install to $INSTALL_DIR"
    fi

    # Install binary
    log_info "Installing to $INSTALL_DIR/$BINARY_NAME"
    if [ "$need_sudo" = true ]; then
        if ! sudo cp "${TMP_DIR}/${BINARY_NAME}" "$INSTALL_DIR/$BINARY_NAME"; then
            log_error "Failed to install binary"
            exit 1
        fi
        sudo chmod +x "$INSTALL_DIR/$BINARY_NAME"
    else
        if ! cp "${TMP_DIR}/${BINARY_NAME}" "$INSTALL_DIR/$BINARY_NAME"; then
            log_error "Failed to install binary"
            exit 1
        fi
        chmod +x "$INSTALL_DIR/$BINARY_NAME"
    fi

    # Verify installation
    log_info "Verifying installation..."
    if ! "$INSTALL_DIR/$BINARY_NAME" --version >/dev/null 2>&1; then
        log_error "Binary verification failed"
        exit 1
    fi

    local installed_version=$("$INSTALL_DIR/$BINARY_NAME" --version)
    log_info "✓ Installation complete! Version: $installed_version"
    log_info "Binary location: $INSTALL_DIR/$BINARY_NAME"

    return 0
}

# Get latest production version from GitHub API
get_latest_production_version() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | \
        grep -o '"tag_name": "[^"]*"' | head -1 | cut -d'"' -f4
}

# Get latest canary version from GitHub API
get_latest_canary_version() {
    curl -fsSL "https://api.github.com/repos/${REPO}/releases" | \
        grep -o '"tag_name": "[^"]*"' | grep -i canary | head -1 | cut -d'"' -f4
}

# Run installation
main "$@"
exit $?
