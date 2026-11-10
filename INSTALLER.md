# One-Line Installer for PDN

## Quick Install

```bash
# Full installer (recommended)
curl -fsSL https://raw.githubusercontent.com/michaelamoetoevsky/PDN/main/install.sh | bash

# Short installer
curl -fsSL https://raw.githubusercontent.com/michaelamoetoevsky/PDN/main/install-short.sh | bash
```

## What It Does

1. **Detects architecture**: aarch64, x86_64, armv7
2. **Installs dependencies**: proot, proot-distro, curl and dpkg
3. **Downloads the latest release**, with a .deb file first and a binary as the fallback
4. **Installs via dpkg** or into `~/.local/bin`
5. **Generates shell completions** for bash, zsh and fish
6. **Creates the default config** and the service directories
7. **Adds the binary to PATH** automatically

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

## Custom Domain pdn.dev

After configuration, users can also use:
```bash
curl -fsSL https://pdn.dev/install.sh | bash
```

## GitHub Actions Auto-Publish

The release workflow syncs the installer scripts to the repository on each release. The workflow file is `.github/workflows/release.yml`.
