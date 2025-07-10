#!/bin/bash

# Solana Validator Switch CLI Installer
# This script automatically detects your platform and installs the latest version

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

# GitHub repository
REPO="huiskylabs/solana-validator-switch"
BINARY_NAME="svs"

# Detect OS and architecture
detect_platform() {
    local os=$(uname -s | tr '[:upper:]' '[:lower:]')
    local arch=$(uname -m)
    
    case "$os" in
        linux)
            case "$arch" in
                x86_64) echo "x86_64-unknown-linux-gnu" ;;
                aarch64|arm64) echo "aarch64-unknown-linux-gnu" ;;
                *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
            esac
            ;;
        darwin)
            case "$arch" in
                x86_64) echo "x86_64-apple-darwin" ;;
                aarch64|arm64) echo "aarch64-apple-darwin" ;;
                *) echo "Unsupported architecture: $arch" >&2; exit 1 ;;
            esac
            ;;
        *)
            echo "Unsupported OS: $os" >&2
            echo "Please download manually from https://github.com/$REPO/releases" >&2
            exit 1
            ;;
    esac
}

# Get latest release version from GitHub API
get_latest_version() {
    curl -s "https://api.github.com/repos/$REPO/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"([^"]+)".*/\1/'
}

main() {
    echo -e "${GREEN}Installing Solana Validator Switch CLI...${NC}"
    
    # Detect platform
    PLATFORM=$(detect_platform)
    echo -e "Detected platform: ${YELLOW}$PLATFORM${NC}"
    
    # Get latest version
    VERSION=$(get_latest_version)
    if [ -z "$VERSION" ]; then
        echo -e "${RED}Failed to get latest version. Please check your internet connection.${NC}" >&2
        exit 1
    fi
    echo -e "Latest version: ${YELLOW}$VERSION${NC}"
    
    # Construct download URL
    DOWNLOAD_URL="https://github.com/$REPO/releases/download/$VERSION/$BINARY_NAME-$PLATFORM.tar.gz"
    echo -e "Download URL: $DOWNLOAD_URL"
    
    # Create temporary directory
    TMP_DIR=$(mktemp -d)
    trap "rm -rf $TMP_DIR" EXIT
    
    # Download binary
    echo -e "\n${GREEN}Downloading...${NC}"
    if ! curl -L --progress-bar "$DOWNLOAD_URL" -o "$TMP_DIR/$BINARY_NAME.tar.gz"; then
        echo -e "${RED}Download failed!${NC}" >&2
        echo "Please download manually from https://github.com/$REPO/releases" >&2
        exit 1
    fi
    
    # Extract binary
    echo -e "${GREEN}Extracting...${NC}"
    tar -xzf "$TMP_DIR/$BINARY_NAME.tar.gz" -C "$TMP_DIR"
    
    # Check if we need sudo for installation
    INSTALL_DIR="/usr/local/bin"
    NEED_SUDO=""
    if [ ! -w "$INSTALL_DIR" ]; then
        NEED_SUDO="sudo"
    fi
    
    # Install binary
    echo -e "${GREEN}Installing to $INSTALL_DIR...${NC}"
    $NEED_SUDO mv "$TMP_DIR/$BINARY_NAME" "$INSTALL_DIR/"
    $NEED_SUDO chmod +x "$INSTALL_DIR/$BINARY_NAME"
    
    # Verify installation
    if command -v "$BINARY_NAME" >/dev/null 2>&1; then
        INSTALLED_VERSION=$("$BINARY_NAME" --version 2>&1 | head -n1)
        echo -e "\n${GREEN}✅ Successfully installed!${NC}"
        echo -e "Version: ${YELLOW}$INSTALLED_VERSION${NC}"
        echo -e "\nRun '${YELLOW}svs${NC}' to get started"
    else
        echo -e "${RED}Installation may have succeeded but $BINARY_NAME is not in PATH${NC}" >&2
        echo -e "Try adding $INSTALL_DIR to your PATH or run: $INSTALL_DIR/$BINARY_NAME"
    fi
}

# Run main function
main