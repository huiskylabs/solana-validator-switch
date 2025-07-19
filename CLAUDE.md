# Solana Validator Switch CLI Project

## Overview

Professional-grade CLI tool for ultra-fast Solana validator switching with runtime node status detection, built in Rust. Used by validator operators to maintain uptime by quickly switching between primary and backup nodes.

## Key Features

- **Ultra-fast switching**: Sub-second identity switches with optimized streaming operations
- **SSH connection pooling**: Persistent connections with multiplexing for instant commands
- **Real-time monitoring**: Interactive dashboard with Ratatui-based terminal UI
- **Telegram alerts**: Notifications for validator health, delinquency, and switch events
- **Universal support**: Works with Firedancer, Agave, Solana, and Jito validators

## Architecture

- **Language**: Rust
- **Async Runtime**: Tokio
- **CLI Framework**: Clap
- **Terminal UI**: Ratatui
- **SSH Library**: openssh-rs v0.10 (with native multiplexing)
- **Configuration**: YAML-based (~/.solana-validator-switch/config.yaml)

## Project Structure

- `src/` - Main source code
  - `main.rs` - Entry point and CLI setup
  - `commands/` - Command implementations (status, switch, test-alert)
  - `ssh.rs` - SSH connection pooling and management
  - `alert.rs` - Telegram alert functionality
  - `config.rs` - Configuration management
  - `solana_rpc.rs` - Solana RPC interactions
- `docs/` - Technical documentation
- `tests/` - Integration and unit tests
- `wiki/` - GitHub wiki pages

## Development Workflow

1. **Testing**: Run `cargo test` before commits
2. **Linting**: Run `cargo clippy -- -D warnings`
3. **Formatting**: Run `cargo fmt`
4. **Building**: Run `cargo build --release`
5. **Pre-commit hooks**: Set up with `./setup-hooks.sh`

## Recent Updates (v1.2.0)

- Side-by-side table layout with SSH health monitoring
- Catchup failure alerts for standby nodes
- Enhanced test alerts showing all alert types
- Improved status UI with countdown timers
- Better error handling and recovery strategies

## Key Commands

- `svs` - Interactive mode (recommended)
- `svs status` - Check validator status
- `svs switch` - Perform validator switch
- `svs switch --dry-run` - Preview switch without executing
- `svs test-alert` - Test Telegram alerts

## Performance Characteristics

- **Switch time**: 25-40 seconds total
- **Voting gap**: 15-25 seconds
- **Memory usage**: ~30MB base + 5MB per SSH session
- **CPU usage**: <3% monitoring, 8-15% during switch

## Security Considerations

- No credential storage (SSH keys referenced by path only)
- No passwords or private keys in config
- Session data in memory only
- Secure file permissions (0600) for config

## Testing

- Run all tests: `cargo test`
- Integration tests available in `tests/`
- Test configurations in `tests/test_config.yaml`

## Important Notes

- This is a defensive security tool for validator operators
- Helps maintain validator uptime and prevent slashing
- Used in production by Huisky Labs validators
- Open source project with active development
- Never include CLAUDE inside commit message
