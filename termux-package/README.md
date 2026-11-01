# PDN Termux Package

This directory contains the Termux package build recipe for PDN (PRoot Distro Next).

## Package Information

- **Name**: pdn
- **Version**: 0.1.0
- **Description**: PRoot Distro Next - Next-generation container platform for Termux/Android
- **License**: GPL-2.0
- **Homepage**: https://github.com/michaelamoetoevsky/PDN

## Build Dependencies

- rust
- clang
- lld
- cmake
- ninja
- protobuf
- pkg-config
- libssl-dev
- libclang-dev

## Runtime Dependencies

- proot
- proot-distro

## Installation

The package installs:
- `podnext` - Main CLI binary
- `podnext-daemon` - Service supervisor daemon
- `pn-gateway` - Gateway binary (if available)
- Shell completions (bash, zsh, fish)
- Termux:Boot integration script
- Default configuration files

## Building

This package is designed to be built within the termux-packages build infrastructure:

```bash
# In termux-packages repo
./build-package.sh pdn
```

## Configuration

Default configuration files are installed to:
- `${PREFIX}/etc/pn-next/supervisor.toml` - Supervisor configuration

Runtime directories:
- `${PREFIX}/etc/pn-next/services` - Service definitions
- `${PREFIX}/var/run/pn-next` - Runtime files
- `${PREFIX}/var/log/pn-next` - Log files

## Termux:Boot Integration

A boot script is installed at `${PREFIX}/etc/termux/boot/pdn-services` to auto-start services on boot.

## Shell Completions

Completions are installed for:
- bash: `${PREFIX}/share/bash-completion/completions/podnext`
- zsh: `${PREFIX}/share/zsh/site-functions/_podnext`
- fish: `${PREFIX}/share/fish/vendor_completions.d/podnext.fish`

## Submitting to termux-packages

1. Fork the termux-packages repository
2. Create a new directory `packages/pdn` with the build.sh script
2. Submit a PR to termux-packages
3. Wait for review and merge
3. The package will be built and published to Termux repositories