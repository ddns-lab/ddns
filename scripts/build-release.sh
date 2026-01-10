#!/bin/bash
#
# Build release packages for ddnsd
#

set -e

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
VERSION="${1:-v0.1.0}"
BUILD_OUTPUT="${REPO_ROOT}/target/release"

echo "Building ddnsd ${VERSION}..."

# Build release binaries
cd "${REPO_ROOT}"

echo "Building for Linux x86_64..."
cargo build --release --target x86_64-unknown-linux-gnu

echo "Building for Linux aarch64..."
cargo build --release --target aarch64-unknown-linux-gnu

echo "Building for macOS x86_64..."
cargo build --release --target x86_64-apple-darwin

# Create release packages
echo "Creating release packages..."

mkdir -p "${REPO_ROOT}/dist"

# Linux x86_64
echo "Packaging ddnsd-${VERSION}-linux-x86_64.tar.gz"
cd "${REPO_ROOT}"
tar -czf "dist/ddnsd-${VERSION}-linux-x86_64.tar.gz" \
    -C "target/x86_64-unknown-linux-gnu/release" ddnsd \
    -C . examples/ddnsd.env.example \
    -C . INSTALL.md \
    -C . README.md

# Linux aarch64
echo "Packaging ddnsd-${VERSION}-linux-aarch64.tar.gz"
tar -czf "dist/ddnsd-${VERSION}-linux-aarch64.tar.gz" \
    -C "target/aarch64-unknown-linux-gnu/release" ddnsd \
    -C . examples/ddnsd.env.example \
    -C . INSTALL.md \
    -C . README.md

# macOS x86_64
echo "Packaging ddnsd-${VERSION}-macos-x86_64.tar.gz"
tar -czf "dist/ddnsd-${VERSION}-macos-x86_64.tar.gz" \
    -C "target/x86_64-apple-darwin/release" ddnsd \
    -C . examples/ddnsd.env.example \
    -C . INSTALL.md \
    -C . README.md

echo ""
echo "Release packages created in ${REPO_ROOT}/dist/"
ls -lh "${REPO_ROOT}/dist/"
