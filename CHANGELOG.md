# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- GitHub Actions workflow for automated releases
- Cross-platform binary builds (Linux, macOS, Windows)
- Release creation script
- Installation instructions for pre-built binaries

### Changed
- Simplified tower file transfer output for better readability
- Updated README with clearer switch time messaging

## [1.0.0] - 2024-XX-XX

### Added
- Initial release
- Interactive CLI menu system
- Automatic validator type detection (Solana/Agave/Firedancer)
- Ultra-fast validator switching (~1 second average)
- Real-time status monitoring
- Comprehensive error handling with recovery suggestions
- Dry-run mode for testing
- Progress indicators and timing information
- Support for multiple validator pairs
- SSH connection pooling for performance
- Tower file transfer with speed calculation
- Swap readiness verification
- Post-switch catchup verification

### Security
- Secure SSH key handling
- No hardcoded credentials
- Safe tower file transfer

[Unreleased]: https://github.com/huiskylabs/solana-validator-switch/compare/v1.0.0...HEAD
[1.0.0]: https://github.com/huiskylabs/solana-validator-switch/releases/tag/v1.0.0