# Contributing to PDN

Thank you for your interest in contributing to PDN (PRoot Distro Next)! This document provides guidelines for contributing to the project.

## Ways to Contribute

- **Code contributions**: Bug fixes, new features, performance improvements
- **Documentation**: Tutorials, API docs, examples, translations
- **Testing**: Bug reports, test cases, integration testing
- **Community**: Answering questions, helping users, reviewing PRs
- **Examples**: Service configs, tutorials, integration guides

## Getting Started

1. Fork the repository
2. Clone your fork: `git clone https://github.com/your-username/PDN.git`
3. Create a branch: `git checkout -b feature/your-feature-name`
4. Make your changes
4. Run tests: `cargo test --workspace`
5. Run clippy: `cargo clippy --workspace`
6. Submit a pull request

## Development Setup

### Prerequisites

- Rust 1.70+ (install via `rustup`)
- Android NDK r27c (for Android builds)
- Termux environment (for testing on Android)

### Building

```bash
# Build all crates
cargo build --workspace --release

# Build for Android (on Linux)
cargo build --workspace --release --target aarch64-linux-android

# Run tests
cargo test --workspace

# Run clippy
cargo clippy --workspace --all-targets --all-features

# Check formatting
cargo fmt --all --check
```

## Code Style

- Follow Rust standard style (`cargo fmt`)
- Pass clippy lints (`cargo clippy`)
- Write tests for new functionality
- Document public APIs with `///` comments
- Use meaningful commit messages

## Pull Request Process

1. Ensure all tests pass
2. Update documentation if needed
3. Add tests for new features
4. Reference related issues in PR description
5. Request review from maintainers

## Good First Issues

Look for issues labeled `good first issue` - these are specifically designed for newcomers:

- [Add shell completion for PowerShell](https://github.com/michaelamoetoevsky/PDN/issues/1)
- [Add example service configurations](https://github.com/michaelamoetoevsky/PDN/issues/2)
- [Create full-stack development tutorial](https://github.com/michaelamoetoevsky/PDN/issues/3)

## Reporting Bugs

When reporting bugs, please include:

- PDN version (`podnext --version`)
- Termux/Android version
- Steps to reproduce
- Expected vs actual behavior
- Relevant logs (`podnext service logs <service>`)

## Feature Requests

Feature requests are welcome! Please:

- Check existing issues first
- Describe the use case
- Explain the expected behavior
- Consider implementation complexity

## Code of Conduct

Please read our [Code of Conduct](CODE_OF_CONDUCT.md) before participating.

## Security

For security issues, please see [SECURITY.md](SECURITY.md) for responsible disclosure.

## License

By contributing, you agree that your contributions will be licensed under the GPL-2.0-or-later license.

## Questions?

- Open a [GitHub Discussion](https://github.com/michaelamoetoevsky/PDN/discussions)
- Open an issue with the `question` label
- Check existing documentation in `/docs`

Thank you for contributing to PDN! 🚀