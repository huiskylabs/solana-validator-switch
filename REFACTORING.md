# Solana Validator Switch - Refactoring with Modern Stack

This document describes the refactoring of the Solana Validator Switch tool to use a modern async architecture with improved performance and user experience.

## Architecture Overview

### Technology Stack

| Component | Old | New | Benefits |
|-----------|-----|-----|----------|
| SSH Exec | ssh2 (sync) | openssh-rs (async) | Reuses sessions, async support, better performance |
| UI | ratatui (basic) | ratatui + channels | Scrollable logs, real-time updates, better state management |
| Async Runtime | tokio (partial) | tokio (full) | Clean concurrency, proper task management |
| Messaging | Direct calls | tokio::sync::mpsc | Non-blocking UI updates, streaming SSH output |
| State Mgmt | Mutex only | Arc<RwLock<>> | Better concurrent access, cleaner architecture |

## Key Improvements

### 1. SSH Session Management (`ssh_async.rs`)

The new SSH module uses `openssh-rs` which provides:
- **Connection pooling** with automatic reuse
- **Async/await** support throughout
- **Native multiplexing** for better performance
- **Streaming output** via channels

```rust
// Old approach (blocking)
let output = ssh_manager.execute_command("long_command")?;

// New approach (streaming)
ssh_pool.execute_command_streaming(
    &node,
    &ssh_key,
    "long_command",
    log_sender, // Channel for real-time output
).await?;
```

### 2. Enhanced UI (`status_ui_v2.rs`)

The refactored UI provides:
- **Split pane design** - Summary view + scrollable logs
- **Real-time SSH output** - See command output as it happens
- **Per-host log tracking** - Switch between different hosts
- **Keyboard navigation** - Tab between panes, scroll logs
- **Non-blocking updates** - UI remains responsive during data fetches

### 3. State Management

```rust
pub struct UiState {
    // Vote data with increment tracking
    pub vote_data: Vec<Option<ValidatorVoteData>>,
    pub increment_times: Vec<Option<Instant>>,
    
    // SSH logs organized by host
    pub host_logs: HashMap<String, Vec<String>>,
    pub log_scroll_offset: usize,
    
    // UI state
    pub focused_pane: FocusedPane,
}
```

### 4. Background Task Architecture

The new architecture separates concerns:
- **UI thread** - Handles rendering at 10 FPS
- **Vote data task** - Fetches RPC data every 5 seconds
- **Catchup task** - Runs SSH catchup commands every 30 seconds
- **Log processor** - Receives SSH output via channels

## Usage

### Standard UI (Current)
```bash
svs status
```

### Enhanced UI (New)
```bash
USE_ENHANCED_UI=1 svs status
```

## Migration Path

The refactoring is designed to be incremental:

1. **Phase 1** ✅ - New SSH module with openssh-rs implemented and integrated
2. **Phase 2** ✅ - Build enhanced UI with channel-based updates
3. **Phase 3** 🚧 - Migrate existing commands to use new SSH pool
4. **Phase 4** 🚧 - Make enhanced UI the default

## Benefits

1. **Performance**
   - SSH connections are reused (not recreated each time)
   - Commands can be cancelled early (e.g., catchup when caught up)
   - Async operations don't block the UI

2. **User Experience**
   - See real-time output from SSH commands
   - Navigate between different hosts' logs
   - UI remains responsive during long operations

3. **Developer Experience**
   - Cleaner async/await code
   - Better error handling with Result types
   - Modular architecture easier to test

## Next Steps

1. Complete migration of all SSH operations to the new pool
2. Add configuration options for UI preferences
3. Implement log persistence and search
4. Add more real-time monitoring capabilities