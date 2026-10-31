# One-Line Installer for PDN

## Quick Install

```bash
# Full installer (recommended)
curl -fsSL https://raw.githubusercontent.com/michaelamoetoevsky/PDN/main/install.sh | bash

# Short installer
curl -fsSL https://raw.githubusercontent.com/michaelamoetoevsky/PDN/main/install-short.sh | bash
```

## What It Does

1. **Detects architecture** (aarch64, x86_64, armv7)
2. **Installs dependencies** (proot, proot-distro, curl, dpkg)
3. **Downloads latest release** (.deb preferred, binary fallback)
4. **Installs via dpkg** or to `~/.local/bin`
5. **Generates shell completions** (bash, zsh, fish)
6. **Creates default config** and service directories
7. **Adds to PATH** automatically

## Manual Install

```bash
# Download .deb directly
wget https://github.com/michaelamoetoevsky/PDN/releases/latest/download/pdn_0.1.0_aarch64-linux-android.deb
dpkg -i pdn_*.deb
apt install -f

# Or binary
wget https://github.com/michaelamoetoevsky/PDN/releases/latest/download/podnext-aarch64-linux-android
chmod +x podnext-aarch64-linux-android
mv podnext-aarch64-linux-android ~/.local/bin/podnext
```

## Post-Install

```bash
# Verify installation
podnext --version
podnext --help

# Search and run containers
podnext search ubuntu
podnext install ubuntu
podnext run ubuntu

# Service supervisor
podnext daemon start
podnext service create myservice
podnext service start myservice
```

## Custom Domain (pdn.dev)

Once configured, users can also use:
```bash
curl -fsSL https://pdn.dev/install.sh | bash
```

## GitHub Actions Auto-Publish

The installer scripts are automatically synced to the repo on each release via `.github/workflows/release.yml`.