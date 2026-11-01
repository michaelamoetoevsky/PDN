# PDN Termux Package - Submission Guide

## Package Information
- **Name**: pdn
- **Version**: 0.1.0
- **Description**: PRoot Distro Next - Next-generation container platform for Termux/Android
- **License**: GPL-2.0
- **Homepage**: https://github.com/michaelamoetoevsky/PDN
- **Source**: https://github.com/michaelamoetoevsky/PDN/archive/refs/tags/v0.1.0.tar.gz

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

## Package Contents

### Binaries
- `podnext` - Main CLI binary
- `podnext-daemon` - Service supervisor daemon
- `pn-gateway` - Gateway binary

### Shell Completions
- bash: `share/bash-completion/completions/podnext`
- zsh: `share/zsh/site-functions/_podnext`
- fish: `share/fish/vendor_completions.d/podnext.fish`

### Configuration
- `/etc/pn-next/supervisor.toml` - Supervisor configuration
- `/etc/termux/boot/pdn-services` - Termux:Boot auto-start script

### Runtime Directories
- `/etc/pn-next/services` - Service definitions
- `/var/run/pn-next` - Runtime files
- `/var/log/pn-next` - Log files

## Submission Steps for termux-packages

1. Fork the termux/termux-packages repository:
   ```bash
   git clone https://github.com/termux/termux-packages.git
   cd termux-packages
   ```

2. Create the package directory:
   ```bash
   mkdir -p packages/pdn
   cp /path/to/PDN/termux-package/recipe/build.sh packages/pdn/
   ```

3. Add the package to the build system:
   ```bash
   # Add pdn to packages.txt or appropriate package list
   echo "pdn" >> packages.txt
   ```

4. Test the build:
   ```bash
   ./build-package.sh pdn
   ```

5. Submit PR to termux/termux-packages with:
   - Title: "Add pdn package - PRoot Distro Next container platform"
   - Description: "Adds PDN (PRoot Distro Next) - a next-generation container platform for Termux/Android"

## Verification Checklist

- [ ] Package builds successfully in termux-packages CI
- [ ] Binary runs correctly on Termux/Android
- [ ] All subcommands work (install, list, service, daemon, etc.)
- [ ] Shell completions work
- [ ] Termux:Boot integration works
- [ ] Service supervisor daemon works

## Binary Verification

The release binaries are available at:
https://github.com/michaelamoetoevsky/PDN/releases/tag/v0.1.0

- `podnext-aarch64-linux-android` - Main CLI
- `pn-gateway-aarch64-linux-android` - Gateway service

## Build Verification

All tests pass:
```
cargo test --workspace
# 93 tests pass across all crates
```

Workspace compiles cleanly:
```
cargo check --workspace
# Clean compilation (only warnings)
```