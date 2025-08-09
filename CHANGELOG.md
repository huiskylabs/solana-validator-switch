# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [1.3.3] - 2025-01-27

### Fixed
- **CRITICAL**: Fixed auto-failover not triggering on validator delinquency
  - Auto-failover was incorrectly checking validator's internal RPC health instead of vote data RPC health
  - Now correctly checks if vote data can be fetched from Solana RPC to verify on-chain data availability
  - This prevented failover even when delinquency was successfully detected
- Removed `--require-tower` flag from standby validator identity switch for better reliability
- Added debug logging to help diagnose auto-failover trigger conditions

## [1.2.4] - 2025-01-23

### Changed
- Optimized swap readiness checks to eliminate redundancy - reduced SSH calls from 3 to 1-2 per node
- Tower file check is now only performed once for active nodes instead of re-running all checks

### Performance
- Faster startup time due to reduced SSH operations
- More efficient node status detection process

## [1.2.3] - 2025-01-23

### Fixed
- Fixed SSH key usage in node status detection - now correctly uses configured/detected SSH keys instead of hardcoded default
- Version checks and swap readiness checks now work properly with custom SSH keys (thanks @stefiix92)

## [1.2.2] - 2025-01-23

### Fixed
- UI refresh behavior now only triggers after successful switch completion, not on initial load
- Added TODO comments for future TOML parser refactoring in Firedancer config parsing

### Changed
- Removed unnecessary UI refresh when canceling switch view
- Improved post-switch UI restart with background refresh for updated validator status

## [1.2.1] - 2025-01-23

### Fixed
- UI event handling now correctly filters key press events only, fixing the double 'y' press issue in switch confirmation
- Startup checks now properly skip tower file requirement for standby nodes during initial validation
- RPC port detection improved to read actual configured ports from validator command lines

### Changed
- Enhanced UI rendering during emergency takeover to prevent display corruption
- Improved catchup status streaming with real-time updates for both Agave/Jito and Firedancer validators

## [1.2.0] - 2025-01-19

### Added
- **Telegram Alerts**: Complete Telegram notification system for validator monitoring
  - Validator delinquency alerts when voting stops
  - Catchup failure alerts for standby nodes (3 consecutive failures)
  - Switch success/failure notifications with timing details
  - Comprehensive test alert command showing all alert types
- **Enhanced Status UI**: Improved validator status display
  - Alert configuration status shown in validator tables
  - Catchup status with 30-second refresh and countdown timer
  - Merged "Last Vote" info into "Vote Status" row for cleaner display
  - Better visual padding for improved readability
  - Spinner indicator (🔄) during catchup checks

### Fixed
- Validator status now correctly updates after successful switch
- UI no longer shows stale Active/Standby assignments post-switch
- Catchup countdown timer moved to status text for better visibility
- Removed UI corruption issues from Telegram bot integration

### Changed
- Removed redundant standard UI, keeping only the enhanced UI
- Simplified Telegram integration (removed bot polling)
- Catchup checks now run every 30 seconds instead of 5 seconds
- Pre-commit hook now only checks build (removed test timeout issues)

### Removed
- Telegram bot view for remote CLI control (caused UI issues)
- Windows support from CI/CD pipeline

## [1.1.0] - 2024-12-18

### Added
- GitHub Actions workflow for automated releases
- Cross-platform binary builds (Linux, macOS, Windows)
- Release creation script
- Installation instructions for pre-built binaries
- Optimized tower transfer with streaming base64 decode + dd
- Enhanced SSH connection pooling with Arc<Session> efficiency
- Modern async architecture with Tokio runtime optimizations
- Interactive dashboard with real-time monitoring
- Comprehensive documentation updates

### Changed
- Simplified tower file transfer output for better readability
- Updated README with clearer switch time messaging
- Improved tower transfer latency from 200-500ms to 100-300ms
- Enhanced SSH command execution with execute_command_with_args optimization
- Updated technical documentation to reflect current implementation
- Optimized SSH connection management with multiplexing

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

[Unreleased]: https://github.com/huiskylabs/solana-validator-switch/compare/v1.2.0...HEAD
[1.2.0]: https://github.com/huiskylabs/solana-validator-switch/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/huiskylabs/solana-validator-switch/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/huiskylabs/solana-validator-switch/releases/tag/v1.0.0