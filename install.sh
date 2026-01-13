#!/bin/sh
# ClipSync Install Script
# Usage: curl -fsSL https://raw.githubusercontent.com/locchung/clipsync/main/install.sh | sh

set -e

REPO="locchung/clipsync"
INSTALL_DIR="${HOME}/.local/bin"
BINARY_NAME="clipsync"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    printf "${GREEN}[INFO]${NC} %s\n" "$1"
}

warn() {
    printf "${YELLOW}[WARN]${NC} %s\n" "$1"
}

error() {
    printf "${RED}[ERROR]${NC} %s\n" "$1"
    exit 1
}

# Detect OS and architecture
detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        linux)
            OS="linux"
            ;;
        darwin)
            OS="macos"
            ;;
        mingw*|msys*|cygwin*)
            OS="windows"
            ;;
        *)
            error "Unsupported operating system: $OS"
            ;;
    esac

    case "$ARCH" in
        x86_64|amd64)
            ARCH="x86_64"
            ;;
        aarch64|arm64)
            ARCH="aarch64"
            ;;
        *)
            error "Unsupported architecture: $ARCH"
            ;;
    esac

    echo "${OS}-${ARCH}"
}

# Check if Rust is installed, offer cargo install as option
check_rust() {
    if command -v cargo >/dev/null 2>&1; then
        return 0
    else
        return 1
    fi
}

# Install from source using cargo
install_from_source() {
    info "Installing from source using cargo..."
    
    if ! check_rust; then
        error "Rust is not installed. Please install Rust first: https://rustup.rs"
    fi

    cargo install --git "https://github.com/${REPO}" --bin clipsync
    
    info "ClipSync installed successfully!"
    info "Run 'clipsync' to start syncing clipboards."
}

# Download and install pre-built binary
install_binary() {
    PLATFORM=$(detect_platform)
    
    info "Detected platform: $PLATFORM"
    info "Downloading ClipSync..."

    # Get latest release URL
    DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/clipsync-${PLATFORM}"
    
    if [ "$OS" = "windows" ]; then
        DOWNLOAD_URL="${DOWNLOAD_URL}.exe"
        BINARY_NAME="clipsync.exe"
    fi

    # Create install directory if needed
    mkdir -p "$INSTALL_DIR"

    # Download binary
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL "$DOWNLOAD_URL" -o "${INSTALL_DIR}/${BINARY_NAME}"
    elif command -v wget >/dev/null 2>&1; then
        wget -q "$DOWNLOAD_URL" -O "${INSTALL_DIR}/${BINARY_NAME}"
    else
        error "Neither curl nor wget found. Please install one of them."
    fi

    # Make executable
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"

    info "ClipSync installed to ${INSTALL_DIR}/${BINARY_NAME}"
}

# Add to PATH if needed
ensure_path() {
    case ":$PATH:" in
        *":$INSTALL_DIR:"*)
            # Already in PATH
            ;;
        *)
            warn "$INSTALL_DIR is not in your PATH"
            info "Add this to your shell profile:"
            echo ""
            echo "  export PATH=\"\$PATH:$INSTALL_DIR\""
            echo ""
            ;;
    esac
}

main() {
    echo ""
    echo "🔗 ClipSync Installer"
    echo "====================="
    echo ""

    # Try to download binary first, fall back to source
    if install_binary 2>/dev/null; then
        ensure_path
        echo ""
        info "Installation complete!"
        info "Run 'clipsync' to start syncing clipboards."
    else
        warn "Pre-built binary not available for your platform."
        info "Falling back to source installation..."
        install_from_source
    fi

    echo ""
}

main "$@"
