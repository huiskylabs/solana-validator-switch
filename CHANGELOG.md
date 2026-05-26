# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.1.0] - 2026-05-25

### Added
- **Low-priority alert channel**: New `telegram_low_priority` config field routes backup-node
  warnings (SSH failures, delinquency, `getHealth` issues) and successful planned switches to a
  separate Telegram channel, keeping the primary channel reserved for actionable failures
- **RPC failure suppression**: High-priority delinquency alerts are suppressed while the
  vote-account RPC is returning consecutive failures; stale cached vote data can no longer produce
  false delinquency pages
- **Stale vote-account data handling**: Vote data that hasn't been refreshed from the cluster is
  now treated as a low-priority condition rather than a delinquency signal
- **`verbose_logging` flag**: Optional runtime diagnostics gated behind a new config field
  (default `false`); log output routed to `~/.solana-validator-switch/logs/latest.log`
- **Configurable poll intervals**: New `vote_account_poll_interval_seconds` and
  `node_status_poll_interval_seconds` config fields (defaults: 10 s and 20 s) allow tuning RPC
  and SSH cadence independently
- **VoteStateV4 compatibility**: Graceful fallback for the new vote-state format introduced in
  Agave 2.x / Firedancer 0.5+; delinquency detection is preserved while richer UI columns degrade
  cleanly instead of panicking

### Fixed
- **Tower verification race (Firedancer)**: SHA-256 checksum is now computed from the exact bytes
  transferred rather than re-fetching the source file after the copy, eliminating a TOCTOU race
  that caused tower-transfer failures under Firedancer
- **Firedancer startup identity detection**: Tightened `ps`-based config-path grep to avoid
  false matches that prevented Firedancer from being detected at startup
- **SSH session stuck for 60+ minutes**: Dead SSH control-socket handles are now evicted from the
  session cache on command failure instead of being reused across every subsequent poll, limiting
  recovery to one failed tick (~10 s) rather than an hour or more
- **Firedancer config path cached**: The `fdctl --config` path is resolved once at startup and
  cached in `NodeWithStatus`, eliminating repeated `ps` lookups during failover

### Changed
- **Primary node load reduced**: `getHealth` calls, SSH keep-alive pings, and the catchup-stream
  monitor are no longer issued against the active primary between slow-check intervals (10 min);
  a voting primary's health is proved via cluster vote-account data instead
- **SSH connection pre-warmed before failover**: The primary SSH session is established at the
  start of the failover procedure to compensate for removing the periodic ping that previously
  kept the connection warm
- **Successful planned switches** now route to the low-priority Telegram channel
- `delinquency_threshold_seconds` default lowered to 30 s (was 1800 s) to match real-world
  operator expectations

## [2.0.6] - 2026-03-13

### Fixed
- Startup process validation now strips ANSI escape codes from command output for accurate parsing
- Improved command output handling to prevent false negatives in validator process detection

## [1.4.0] - 2025-01-27

### Fixed
- **CRITICAL**: Fixed auto-failover not triggering on validator delinquency
  - Auto-failover was incorrectly checking validator's internal RPC health instead of vote data RPC health
  - Now correctly checks if vote data can be fetched from Solana RPC to verify on-chain data availability
  - This prevented failover even when delinquency was successfully detected
- Removed duplicate unthrottled delinquency alerts that were bypassing the 15-minute cooldown
  - Delinquency alerts now properly respect the configured throttling period
  - Eliminated alert spam when validator becomes delinquent
- Removed `--require-tower` flag from standby validator identity switch for better reliability
- Improved debug logging - auto-failover conditions only log when actually triggering

### Changed
- Consolidated delinquency checking to single location with proper alert throttling
- Cleaned up redundant code in refresh_vote_data_for_alerts function


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