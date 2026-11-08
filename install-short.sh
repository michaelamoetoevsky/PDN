#!/bin/bash
# PDN One-Line Installer (Short version for README)
# Usage: curl -fsSL https://pdn.dev/install.sh | bash
#        or: curl -fsSL https://raw.githubusercontent.com/michaelamoetoevsky/PDN/main/install.sh | bash

set -e

REPO="michaelamoetoevsky/PDN"
INSTALL_DIR="${HOME}/.local/bin"

# Writable temp dir: $TMPDIR, else $PREFIX/tmp on Termux, else /tmp. This script used to hardcode
# /tmp, which does not exist for a Termux app uid, so the .deb download always failed there
# (termux-tmpdir-hygiene); the fallback matches install.sh.
: "${TMPDIR:=${PREFIX:+$PREFIX/tmp}}"
: "${TMPDIR:=/tmp}"

# Verify a download against the checksum the release publishes beside it: a mismatch always refuses,
# and a missing checksum refuses unless the operator sets PDN_ALLOW_UNVERIFIED=1 (which says so).
# Before this, this script downloaded whatever the URL served and ran it (ASEC3-SUPPLY-01, issue #74).
verify_download() {
    _file=$1
    _url=$2
    _sum="$_file.sha256"
    if ! curl -fsSL -o "$_sum" "$_url.sha256" 2>/dev/null; then
        if [ "${PDN_ALLOW_UNVERIFIED:-}" = "1" ]; then
            echo "No checksum for $(basename "$_url"); PDN_ALLOW_UNVERIFIED=1: installing UNVERIFIED"
            return 0
        fi
        echo "ERROR: no checksum published for $(basename "$_url"); refusing to install an unverified download"
        echo "       Releases publish <asset>.sha256 beside every asset. To install anyway: PDN_ALLOW_UNVERIFIED=1"
        rm -f "$_sum"
        return 1
    fi
    _expected=$(cut -d' ' -f1 < "$_sum")
    _actual=$(sha256sum "$_file" | cut -d' ' -f1)
    if [ "$_expected" != "$_actual" ]; then
        echo "ERROR: checksum mismatch for $(basename "$_url")"
        echo "       expected: $_expected"
        echo "       actual:   $_actual"
        rm -f "$_sum"
        return 1
    fi
    rm -f "$_sum"
    echo "Checksum verified: $(basename "$_url")"
    return 0
}

# Detect architecture
ARCH=$(uname -m)
case $ARCH in
    aarch64|arm64) TARGET="aarch64-linux-android" ;;
    x86_64|amd64) TARGET="x86_64-linux-android" ;;
    armv7l|armhf) TARGET="armv7-linux-androideabi" ;;
    *) echo "Unsupported architecture: $ARCH"; exit 1 ;;
esac

# Install dependencies
pkg install -y proot proot-distro curl dpkg 2>/dev/null || true

# Get latest version
VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | cut -d'"' -f4)

# Try .deb first
DEB_URL="https://github.com/${REPO}/releases/download/${VERSION}/pdn_${VERSION#v}_${TARGET}.deb"
TEMP_DEB="$TMPDIR/pdn_${VERSION#v}_${TARGET}.deb"

if curl -fsSL -o "$TEMP_DEB" "$DEB_URL" 2>/dev/null; then
    verify_download "$TEMP_DEB" "$DEB_URL" || { rm -f "$TEMP_DEB"; exit 1; }
    dpkg -i "$TEMP_DEB" 2>/dev/null || apt install -f -y
    rm -f "$TEMP_DEB"
else
    # Fallback to binary
    mkdir -p "$INSTALL_DIR"
    for BIN in podnext podnext-daemon pn-gateway; do
        _url="https://github.com/${REPO}/releases/download/${VERSION}/${BIN}-${TARGET}"
        if curl -fsSL -o "$INSTALL_DIR/$BIN" "$_url" 2>/dev/null; then
            if verify_download "$INSTALL_DIR/$BIN" "$_url"; then
                chmod +x "$INSTALL_DIR/$BIN"
            else
                rm -f "$INSTALL_DIR/$BIN"
                exit 1
            fi
        fi
    done
    export PATH="$PATH:$INSTALL_DIR"
    echo "export PATH=\"\$PATH:$INSTALL_DIR\"" >> "$HOME/.bashrc"
fi

# Setup completions and config
command -v podnext &>/dev/null && {
    podnext completion bash > "$HOME/.bash_completion.d/podnext" 2>/dev/null || true
    podnext completion zsh > "$HOME/.zsh/completions/_podnext" 2>/dev/null || true
    podnext completion fish > "$HOME/.config/fish/completions/podnext.fish" 2>/dev/null || true
}

mkdir -p "$HOME/.config/pn-next/services" "$HOME/.local/run/pn-next" "$HOME/.local/log/pn-next"

cat > "$HOME/.config/pn-next/supervisor.toml" << 'EOF'
services_dir = "/data/data/com.termux/files/home/.config/pn-next/services"
runtime_dir = "/data/data/com.termux/files/home/.local/run/pn-next"
log_dir = "/data/data/com.termux/files/home/.local/log/pn-next"
supervision_interval_ms = 1000
shutdown_timeout_secs = 30
enable_cgroups = true
max_concurrent_ops = 10
EOF

echo "PDN installed! Run 'podnext --help' to get started."