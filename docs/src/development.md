# Development Guide

## Building from Source

### Prerequisites

```bash
# Host (Linux/macOS/Windows WSL)
rustup default stable
rustup component add rustfmt clippy

# Android cross-compilation
rustup target add aarch64-linux-android
# Install Android NDK r27c
```

### Clone and Build

```bash
git clone https://github.com/michaelamoetoevsky/PDN.git
cd PDN

# Build all crates (host)
cargo build --workspace --release

# Build for Android
cargo build --workspace --release --target aarch64-linux-android

# Build specific crate
cargo build --release --package podnext
```

### Development Build

```bash
# Fast debug build
cargo build --workspace

# Run tests
cargo test --workspace

# Run specific test
cargo test --package proot-compat security::tests::test_security_profile_default

# Run with output
cargo test --workspace -- --nocapture
```

## Project Structure

```
PDN/
├── .github/
│   └── workflows/          # CI/CD workflows
├── crates/
│   ├── proot-core/         # Core PRoot implementation
│   ├── proot-distro-core/  # Container/image/network/storage mgmt
│   ├── proot-compat/       # Termux/Android compatibility
│   ├── pn-gateway/         # Gateway layer (fail-safe, guardrails, health, metrics)
│   ├── pn-next/            # Advanced runtime, scheduler, security, API
│   └── podnext/            # CLI
├── docs/                   # mdBook documentation
├── fuzz/                   # cargo-fuzz targets
├── Cargo.toml              # Workspace manifest
├── README.md
└── LICENSE
```

## Running Tests

### Unit Tests

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test --package proot-compat
cargo test --package pn-next

# Specific test
cargo test --package proot-compat security::tests::test_seccomp_profile_default

# With output
cargo test --workspace -- --nocapture

# With threads
cargo test --workspace -- --test-threads=4
```

### Integration Tests

```bash
# Run integration tests (requires Termux environment)
cargo test --workspace --test integration

# Or use the CI script
./scripts/integration-test.sh
```

### Fuzzing

```bash
# Install cargo-fuzz
cargo install cargo-fuzz

# Run fuzz targets (short)
cd fuzz
timeout 300 cargo fuzz run fuzz_security_deserialize -- -max_total_time=60

# Run all fuzz targets
for target in fuzz_security_deserialize fuzz_seccomp_deserialize fuzz_capability_deserialize; do
    timeout 300 cargo fuzz run $target -- -max_total_time=60
done
```

### Benchmarks

```bash
# Run benchmarks
cargo bench --workspace

# Specific benchmark
cargo bench --package pn-gateway guardrails
```

## Code Quality

### Formatting

```bash
# Check formatting
cargo fmt --all --check

# Apply formatting
cargo fmt --all
```

### Linting

```bash
# Check clippy
cargo clippy --workspace --all-targets --all-features -- -D warnings

# Fix clippy suggestions
cargo clippy --workspace --fix --allow-dirty --allow-staged
```

### Security Audit

```bash
# Install tools
cargo install cargo-audit cargo-deny cargo-outdated cargo-geiger

# Run audit
cargo audit --workspace

# Check dependencies
cargo deny check advisories bans licenses sources

# Check outdated
cargo outdated --workspace --exit-code 1

# Check unsafe code
cargo geiger --workspace --output-format json > geiger-report.json
```

## Development Workflow

### Branching Model

```
main (protected)
  │
  ├── develop (default branch)
  │     │
  │     ├── feature/xxx
  │     ├── bugfix/yyy
  │     └── docs/zzz
  │
  └── release/v1.0.0
```

### Commit Messages

Follow Conventional Commits:

```
<type>(<scope>): <description>

[optional body]

[optional footer]
```

Types:
- `feat`: New feature
- `fix`: Bug fix
- `docs`: Documentation
- `style`: Formatting
- `refactor`: Code restructuring
- `perf`: Performance
- `test`: Tests
- `chore`: Maintenance
- `build`: Build system
- `ci`: CI/CD
- `security`: Security

Examples:
```
feat(pn-next): add GPU acceleration support
fix(proot-compat): resolve symlink chain resolution bug
docs: update migration guide for v1.0
security(pn-gateway): add rate limiting to guardrails
```

### Pull Requests

1. Create feature branch from `develop`
2. Make changes with tests
3. Run quality checks: `cargo fmt --check && cargo clippy && cargo test`
4. Update documentation if needed
5. Create PR against `develop`
6. CI must pass
7. Code review required
8. Squash and merge

## Release Process

### Versioning

Semantic Versioning (SemVer): `MAJOR.MINOR.PATCH`

- MAJOR: Breaking changes
- MINOR: New features (backward compatible)
- PATCH: Bug fixes (backward compatible)

### Release Steps

```bash
# 1. Update version in Cargo.toml files
# Workspace version in Cargo.toml
# Individual crate versions in crates/*/Cargo.toml

# 2. Update CHANGELOG.md
# Generate from commits
git log --pretty=format:"- %s (%h)" v0.9.0..HEAD > CHANGELOG.md

# 3. Create release commit
git add .
git commit -m "chore: release v1.0.0"

# 4. Create tag
git tag -a v1.0.0 -m "PDN v1.0.0"

# 5. Push tag (triggers release workflow)
git push origin v1.0.0

# 6. GitHub Actions builds and publishes release
# 7. Optionally publish to crates.io
cargo publish --package proot-core
cargo publish --package proot-distro-core
cargo publish --package proot-compat
cargo publish --package pn-gateway
cargo publish --package pn-next
cargo publish --package podnext
```

### Release Automation

The release is automated via GitHub Actions (`.github/workflows/release.yml`):

1. Tag push triggers workflow
2. Build for host and Android
3. Run tests and security audit
4. Create release artifacts
5. Generate changelog
6. Create GitHub Release
7. Upload artifacts

## Adding New Features

### 1. Design Document

Create design doc in `docs/design/`:
```markdown
# Feature: <Name>

## Motivation
## Design
## API
## Security Considerations
## Testing Plan
## Migration Path
```

### 2. Implementation

Follow the crate structure:

```
# For core functionality
crates/proot-core/src/

# For container management
crates/proot-distro-core/src/

# For compatibility
crates/proot-compat/src/

# For gateway
crates/pn-gateway/src/

# For advanced features
crates/pn-next/src/

# For CLI
crates/podnext/src/
```

### 3. Tests

```rust
// Unit tests in same file
#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_my_feature() {
        // Test implementation
    }
}

// Integration tests in tests/ directory
// tests/integration_test.rs
```

### 4. Documentation

- Update relevant `.md` files in `docs/src/`
- Add CLI help text
- Update `CHANGELOG.md`

### 5. Example

```rust
// examples/my_feature.rs
use pdn_next::RuntimeManager;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let runtime = RuntimeManager::new().await?;
    // Use feature
    Ok(())
}
```

Run with: `cargo run --example my_feature`

## Debugging

### Logging

```bash
# Enable debug logging
export PN_LOG_LEVEL=debug
podnext run ubuntu -- cmd

# Structured logging
export PN_LOG_FORMAT=json
podnext run ubuntu -- cmd | jq .
```

### Tracing

```bash
# Enable OpenTelemetry
export PN_TRACING_ENABLED=true
export PN_TRACING_ENDPOINT=http://localhost:4317
podnext run ubuntu -- cmd
```

### Debug Build

```bash
# Build with debug symbols
cargo build --workspace

# Run with gdb
gdb --args target/debug/podnext run ubuntu -- cmd

# Run with lldb
lldb target/debug/podnext -- run ubuntu -- cmd
```

### Common Issues

#### Build Failures

```bash
# Clean and rebuild
cargo clean
cargo build --workspace

# Check dependencies
cargo tree -d

# Update dependencies
cargo update
```

#### Test Failures

```bash
# Run single test with output
cargo test --package proot-compat test_name -- --nocapture

# Run with backtrace
RUST_BACKTRACE=1 cargo test --package proot-compat test_name
```

#### Cross-compilation Issues

```bash
# Check target
rustup target list | grep android

# Verify NDK
ls $ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android35-clang

# Set linker manually
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER=$ANDROID_NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin/aarch64-linux-android35-clang
```

## Contributing

### Code of Conduct

Be respectful, inclusive, and constructive. See `CODE_OF_CONDUCT.md`.

### First Contribution

1. Find `good first issue` label
2. Comment on issue to claim
3. Follow development workflow
4. Ask for help in discussions

### Review Guidelines

- Check for security implications
- Verify tests pass
- Ensure documentation updated
- Look for performance regressions
- Check for breaking changes

### Getting Help

- GitHub Discussions
- Discord/Slack (if available)
- Email maintainers

## Tools and Scripts

### Useful Scripts

```bash
# scripts/dev-setup.sh - Set up development environment
# scripts/test-all.sh - Run all tests
# scripts/bench.sh - Run benchmarks
# scripts/security-audit.sh - Run security checks
# docs/scripts/build-docs.sh - Build documentation
```

### VS Code Configuration

```json
// .vscode/settings.json
{
    "rust-analyzer.checkOnSave.command": "clippy",
    "rust-analyzer.cargo.allFeatures": true,
    "editor.formatOnSave": true,
    "editor.codeActionsOnSave": {
        "source.fixAll.rust": true
    }
}
```

```json
// .vscode/launch.json
{
    "version": "0.2.0",
    "configurations": [
        {
            "type": "lldb",
            "request": "launch",
            "name": "Debug podnext",
            "cargo": {
                "args": ["build", "--package", "podnext", "--bin", "podnext"],
                "filter": { "name": "podnext", "kind": "bin" }
            },
            "args": ["run", "ubuntu", "--", "echo", "hello"],
            "cwd": "${workspaceFolder}"
        }
    ]
}
```

## Performance Profiling

### CPU Profiling

```bash
# Build with profiling
cargo build --workspace --release --features profiling

# Run with perf
perf record --call-graph=dwarf target/release/podnext run ubuntu -- cmd
perf report
```

### Memory Profiling

```bash
# Use heaptrack
heaptrack target/release/podnext run ubuntu -- cmd
heaptrack_gui heaptrack.podnext.*.gz
```

### Flame Graphs

```bash
# Install flamegraph
cargo install flamegraph

# Generate
cargo flamegraph --package podnext --bin podnext -- run ubuntu -- cmd
```

## See Also

- [Architecture](architecture.md) - System architecture
- [API Reference](api-reference.md) - Programmatic APIs
- [CLI Reference](cli-reference.md) - Command-line interface
- [Security](security.md) - Security considerations