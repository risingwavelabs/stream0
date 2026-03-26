#!/bin/sh
set -e

# Box0 installer
# Usage: curl -sSL https://box0.dev/install.sh | sh

REPO="risingwavelabs/box0"
INSTALL_DIR="${B0_INSTALL_DIR:-/usr/local/bin}"

# Detect OS
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
case "$OS" in
  darwin) OS="darwin" ;;
  linux) OS="linux" ;;
  *) echo "Unsupported OS: $OS"; exit 1 ;;
esac

# Detect architecture
ARCH=$(uname -m)
case "$ARCH" in
  x86_64|amd64) ARCH="x64" ;;
  aarch64|arm64) ARCH="arm64" ;;
  *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

BINARY="b0-${OS}-${ARCH}"

# Get latest version
VERSION=$(curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed 's/.*"tag_name": "//;s/".*//')
if [ -z "$VERSION" ]; then
  echo "Failed to fetch latest version"
  exit 1
fi

URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY}"

echo "Installing Box0 ${VERSION} (${OS}/${ARCH})"
echo "  From: ${URL}"
echo "  To:   ${INSTALL_DIR}/b0"

# Download
TMPFILE=$(mktemp)
curl -sSL -o "$TMPFILE" "$URL"
chmod +x "$TMPFILE"

# Install
if [ -w "$INSTALL_DIR" ]; then
  mv "$TMPFILE" "${INSTALL_DIR}/b0"
else
  echo "  Using sudo to install to ${INSTALL_DIR}"
  sudo cp "$TMPFILE" "${INSTALL_DIR}/b0"
  sudo chmod +x "${INSTALL_DIR}/b0"
  rm -f "$TMPFILE"
fi

echo "  Installed: $(b0 --version 2>/dev/null || echo 'b0 not in PATH')"
echo ""
echo "Get started:"
echo "  b0 server"
echo "  b0 skill install claude-code"
