#!/bin/bash
# PDN One-Line Installer (Short version for README)
# Usage: curl -fsSL https://pdn.dev/install.sh | bash
#        or: curl -fsSL https://raw.githubusercontent.com/michaelamoetoevsky/PDN/main/install.sh | bash

set -e

REPO="michaelamoetoevsky/PDN"
INSTALL_DIR="${HOME}/.local/bin"

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
TEMP_DEB="/tmp/pdn_${VERSION#v}_${TARGET}.deb"

if curl -fsSL -o "$TEMP_DEB" "$DEB_URL" 2>/dev/null; then
    dpkg -i "$TEMP_DEB" 2>/dev/null || apt install -f -y
    rm -f "$TEMP_DEB"
else
    # Fallback to binary
    mkdir -p "$INSTALL_DIR"
    for BIN in podnext podnext-daemon pn-gateway; do
        curl -fsSL -o "$INSTALL_DIR/$BIN" "https://github.com/${REPO}/releases/download/${VERSION}/${BIN}-${TARGET}" 2>/dev/null && chmod +x "$INSTALL_DIR/$BIN"
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