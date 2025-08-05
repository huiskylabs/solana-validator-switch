# Contributing to Solana Validator Switch

First off, thank you for considering contributing to Solana Validator Switch! It's people like you that make SVS such a great tool for the Solana validator community.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [How Can I Contribute?](#how-can-i-contribute)
- [Development Setup](#development-setup)
- [Pull Request Process](#pull-request-process)
- [Style Guidelines](#style-guidelines)
- [Community](#community)

## Code of Conduct

This project and everyone participating in it is governed by the [Solana Validator Switch Code of Conduct](CODE_OF_CONDUCT.md). By participating, you are expected to uphold this code.

## Getting Started

1. Fork the repository on GitHub
2. Clone your fork locally
3. Create a new branch for your feature or bug fix
4. Make your changes
5. Push to your fork and submit a pull request

## How Can I Contribute?

### Reporting Bugs

Before creating bug reports, please check existing issues as you might find out that you don't need to create one. When you are creating a bug report, please include as many details as possible:

- **Use a clear and descriptive title**
- **Describe the exact steps to reproduce the problem**
- **Provide specific examples to demonstrate the steps**
- **Include your configuration** (redact sensitive information)
- **Include logs and error messages**
- **Describe the behavior you observed and expected**
- **Include your environment details** (OS, Rust version, etc.)

### Suggesting Enhancements

Enhancement suggestions are tracked as GitHub issues. When creating an enhancement suggestion, please include:

- **Use a clear and descriptive title**
- **Provide a detailed description of the suggested enhancement**
- **Explain why this enhancement would be useful**
- **List any alternative solutions you've considered**

### Your First Code Contribution

Unsure where to begin? You can start by looking through these issues:

- [Good first issues](https://github.com/huiskylabs/solana-validator-switch/labels/good%20first%20issue) - issues which should only require a few lines of code
- [Help wanted issues](https://github.com/huiskylabs/solana-validator-switch/labels/help%20wanted) - issues which need extra attention

## Development Setup

### Prerequisites

- Rust 1.70+ (install via [rustup](https://rustup.rs/))
- Git
- A Solana validator setup (for testing)

### Local Development

1. **Clone your fork**
   ```bash
   git clone https://github.com/YOUR_USERNAME/solana-validator-switch.git
   cd solana-validator-switch
   ```

2. **Install dependencies**
   ```bash
   cargo build
   ```

3. **Set up pre-commit hooks**
   ```bash
   ./setup-hooks.sh
   ```

4. **Run tests**
   ```bash
   cargo test
   ```

5. **Run the application**
   ```bash
   cargo run --release
   ```

### Testing

- Write tests for any new functionality
- Ensure all tests pass: `cargo test`
- Run clippy: `cargo clippy -- -D warnings`
- Format code: `cargo fmt`

## Pull Request Process

1. **Update Documentation**
   - Update the README.md with details of changes if applicable
   - Update any relevant documentation in the `docs/` directory
   - Add or update tests as needed

2. **Code Quality**
   - Ensure your code follows the [Rust Style Guidelines](#style-guidelines)
   - Run `cargo fmt` before committing
   - Run `cargo clippy -- -D warnings` and fix any issues
   - Ensure all tests pass with `cargo test`

3. **Commit Messages**
   - Use the present tense ("Add feature" not "Added feature")
   - Use the imperative mood ("Move cursor to..." not "Moves cursor to...")
   - Limit the first line to 72 characters or less
   - Reference issues and pull requests liberally after the first line

   Example:
   ```
   feat: add support for multiple SSH key types

   - Add Ed25519 key support
   - Improve key detection logic
   - Update configuration example

   Fixes #123
   ```

4. **Pull Request Description**
   - Clearly describe what your PR does
   - Link any related issues
   - Include screenshots for UI changes
   - List any breaking changes

## Style Guidelines

### Rust Style

We follow the standard Rust style guidelines:

- Use `cargo fmt` to format your code
- Use `cargo clippy` to catch common mistakes
- Prefer `&str` over `String` for function parameters
- Use `Result<T, E>` for operations that can fail
- Write descriptive variable names
- Add comments for complex logic
- Use `TODO` or `FIXME` markers for future work

### Documentation

- Use `///` for public API documentation
- Include examples in documentation when helpful
- Keep documentation up to date with code changes
- Write clear commit messages

### Error Handling

- Use `anyhow` for application errors
- Provide helpful error messages
- Log errors appropriately
- Handle edge cases gracefully

## Feature Development Guidelines

### Adding New Features

1. **Discuss First**
   - Open an issue to discuss major features
   - Get feedback from maintainers
   - Consider the impact on existing users

2. **Design Considerations**
   - Keep the UI/UX consistent
   - Maintain backward compatibility
   - Consider performance implications
   - Think about error cases

3. **Implementation**
   - Write clean, maintainable code
   - Add comprehensive tests
   - Update documentation
   - Consider configuration options

### Performance Considerations

SVS is performance-critical software. When contributing:

- Profile before optimizing
- Avoid premature optimization
- Consider memory usage
- Test with realistic workloads
- Document performance characteristics

## Community

### Getting Help

- GitHub Issues for bug reports and features
- GitHub Discussions for questions and ideas
- Twitter: [@huiskylabs](https://twitter.com/huiskylabs)

### Recognition

Contributors are recognized in:
- The project README
- Release notes
- Our website

## License

By contributing to Solana Validator Switch, you agree that your contributions will be licensed under its MIT license.

## Questions?

Feel free to open an issue or reach out to the maintainers if you have any questions about contributing.

Thank you for helping make Solana Validator Switch better! 🚀