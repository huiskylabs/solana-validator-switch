# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [2.2.0] - 2026-09-14

### Added
- **Promotion verification**: Emergency failover now confirms the target node actually switched to the funded identity before reporting success. `set-identity` returning success only proves SSH accepted the command; the new verification step prevents false "takeover completed" reports when the target never actually switched
- **Failover modes**: Introduced `FailoverMode::Graceful` and `FailoverMode::DegradedSourceUnavailable` to handle scenarios where the source node is unreachable. Degraded mode skips source demotion and tower transfer, allowing promotion of a healthy standby even when the primary is completely down
- **Type-safe role provenance**: New `AssertedNodeRoles` wrapper forces callers to state whether roles came from live UI state or current app state, preventing stale role reads at compile time. Eliminates the catastrophic backwards-failover scenario
- **SSH session recovery**: Per-connection mutex locks prevent SSH connection stampedes when nodes are unhealthy. Session liveness now probes through the actual shell path (bash/PowerShell) rather than bare `true`, catching wedged multiplex masters that fail real commands
- **Role resolution system**: `resolve_roles()` function derives switch direction from observed runtime state rather than config order or startup snapshots, handling ActiveAndStandby, DegradedStandbyPromotion, and TowerRecovery scenarios
- **Runtime logging**: Switch attempts and outcomes are now logged to `~/.solana-validator-switch/logs/latest.log` with full context (dry-run vs live, source/target nodes, mode, reason)
- **SSH pool tests**: New test suite for SSH connection management and recovery logic

### Fixed
- **Stale role bug**: Status UI and manual switches now derive node roles from live state refreshed on every poll tick instead of using the startup snapshot frozen in `AppState`. After an automatic failover, subsequent operations no longer use inverted roles
- **SSH shell detection**: Prefer bash over PowerShell on all platforms. PowerShell misdetection was causing "remote process has terminated" errors on Linux validators with pwsh installed but misconfigured
- **Alert cooldown blocking failover**: The 15-minute alert cooldown no longer prevents the auto-failover gate from firing. Alert sending and failover evaluation are now separate concerns
- **Wedged SSH sessions**: Sessions that stay half-alive (trivial commands succeed, real commands fail with "remote process terminated") are now detected and reconnected instead of wedging for hours
- **Tower unavailable handling**: Failover can now continue without tower transfer when the source was successfully demoted but tower retrieval fails, instead of rolling back a working demotion

### Changed
- **RPC timeout increased**: Cluster RPC calls now use 10s timeout (was 3s) to accommodate tail latency. Two calls still fit within the 60s vote poll interval
- **Enrichment timeout**: Optional vote-account enrichment (VoteState decoding for UI decoration) uses a tight 2s timeout to prevent delaying delinquency detection
- **Blocking RPC client**: Moved to `tokio::task::spawn_blocking()` to prevent starving Tokio worker threads during vote polls
- **SSH pre-warming**: Manual switches now only pre-warm the standby connection in degraded mode, skipping the source when it's known to be unreachable
- **Log attribution**: Validator-level log lines now use role-corrected statuses, attributing lines to the correct node after failovers instead of naming the demoted one
- **Enrichment logging**: Only state transitions (degraded/recovered) are logged instead of every failure, reducing log noise from transient RPC hiccups

## [2.1.1] - 2026-06-11

### Fixed
- **Auto-failover trigger restored**: the trigger inadvertently removed during the
  2.1.0 cleanup pass is reinstated, so HIGH-PRIORITY delinquency once again spawns
  `execute_emergency_failover` when `auto_failover_enabled && vote_rpc_failures == 0`
- **Telegram entity-parse failure**: dropped `parse_mode: "Markdown"` from the
  `sendMessage` payload; validator labels containing unescaped Markdown
  metacharacters (`_`, `*`, `` ` ``, `[`) no longer cause silent alert drops,
  including HIGH-PRIORITY delinquency alerts that gate auto-failover operator
  visibility
- **Concurrent failover spawn**: the auto-failover spawn site now checks
  `emergency_takeover_in_progress` before spawning `execute_emergency_failover`
  and emits an Info-level `"Auto-failover spawn skipped: previous emergency
  takeover still in progress"` log line when a prior takeover is still running,
  preventing a second `tokio::spawn` from racing the in-flight one

### Changed
- **Background-task storage**: `Arc<RwLock<Vec<JoinHandle>>>` migrated to
  `Arc<std::sync::Mutex<JoinSet>>` (`JoinSet` auto-aborts on drop, structurally
  preventing the dup-cycle bug where re-invocations of `spawn_background_tasks`
  could leave previous tasks running alongside the new ones). Boolean signaling
  flags (`should_quit`, `emergency_takeover_in_progress`, `switch_confirmed`)
  migrated from `Arc<RwLock<bool>>` to `Arc<AtomicBool>` (wait-free, eliminates
  silent-drop under lock contention)

### Added
- **`SVS_SIMULATE_FAILOVER=<idx>` env var**: env-var-gated simulation that emits
  a `🚨 SIMULATION DRY-RUN: ...` log marker on every poll tick without spawning
  the real failover, allowing non-destructive end-to-end verification of the
  auto-failover gate logic

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