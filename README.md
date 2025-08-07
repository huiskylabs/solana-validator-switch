# Solana Validator Switch (SVS) - Ultra-Fast Hot Swap & Failover Tool

**Solana Validator Switch (SVS)** is a professional CLI tool for instant hot swap and failover operations, enabling validator switching in just 1-3 seconds. Built in Rust for maximum performance, SVS provides automated failover capabilities, seamless hot swapping between active/standby nodes, and works with Firedancer, Agave, Solana, and Jito validators. Trusted by production validators for zero-downtime hot swaps and emergency failover scenarios to maintain 99.9%+ uptime.

<div align="center">

![Build Status](https://github.com/huiskylabs/solana-validator-switch/workflows/CI/badge.svg)
![Version](https://img.shields.io/github/v/release/huiskylabs/solana-validator-switch)
![License](https://img.shields.io/github/license/huiskylabs/solana-validator-switch)
![Rust](https://img.shields.io/badge/rust-%23000000.svg?style=flat&logo=rust&logoColor=white)
![Solana](https://img.shields.io/badge/Solana-00FFF0?style=flat&logo=solana&logoColor=black)
![PRs Welcome](https://img.shields.io/badge/PRs-welcome-brightgreen.svg)
![Downloads](https://img.shields.io/github/downloads/huiskylabs/solana-validator-switch/total)
![Last Commit](https://img.shields.io/github/last-commit/huiskylabs/solana-validator-switch)

[Features](#key-features) • [Installation](#installation) • [Usage](#usage) • [Configuration](#configuration) • [Performance](#-performance--benchmarks) • [FAQ](#-frequently-asked-questions)

</div>

> **Built by validators, for validators** - Stop losing sleep over manual switches. Get the fastest switch possible.

## 🎥 Demo

![Solana Validator Switch Demo - Ultra-fast validator switching in action](assets/demo.gif)

## 🚀 Why Solana Validator Switch?

**SVS** is the industry-leading Solana validator hot swap and failover solution, trusted by professional validators to maintain 99.9%+ uptime. Whether you're running Firedancer, Agave, Solana, or Jito validators, SVS provides the fastest, most reliable hot swapping and failover capabilities:

- **⚡ Lightning-fast hot swap**: 1-3 seconds total switch time
- **🔄 Automated failover**: Zero-downtime automatic failover on validator failure
- **🔥 Instant hot swapping**: Seamless hot swap between active/standby validators
- **📊 Real-time monitoring**: Live dashboard tracks both nodes for failover readiness
- **🔔 Failover alerts**: Telegram notifications for automatic failover events
- **🛡️ Production-ready**: Battle-tested hot swap operations by Huisky Labs
- **🔧 Universal compatibility**: Hot swap support for all major Solana clients

## Installation

### Quick Install (Recommended)

```bash
# Auto-detects your platform and installs the latest version
curl -sSL https://raw.githubusercontent.com/huiskylabs/solana-validator-switch/main/install.sh | bash

# After installation, 'svs' is available immediately
svs
```

<details>
<summary>Alternative installation methods (requires Rust and Cargo)</summary>

#### Clone and Run

```bash
git clone https://github.com/huiskylabs/solana-validator-switch
cd solana-validator-switch
cargo run --release
```

#### Install with Cargo

```bash
cargo install --git https://github.com/huiskylabs/solana-validator-switch

# Add to PATH if not already there
export PATH="$HOME/.cargo/bin:$PATH"
svs
```

</details>

## Usage

### Interactive Mode (Recommended)

```bash
svs           # Opens interactive menu
svs -c /path/to/custom/config.yaml  # Use custom config file
```

### Command Line Mode

```bash
svs status                    # Check validator status
svs status --validator 0      # Check specific validator by index
svs switch                    # Perform validator switch
svs switch --dry-run          # Preview switch without executing
svs switch --validator 1      # Switch specific validator by index
svs test-alert                # Test Telegram alert configuration
svs --config /path/to/config  # Use custom config file for any command
svs --version                 # Show version
svs --help                    # Show help
```

## Configuration

### Default Configuration
```bash
mkdir -p ~/.solana-validator-switch
cp config.example.yaml ~/.solana-validator-switch/config.yaml
nano ~/.solana-validator-switch/config.yaml
```

### Multiple Configurations
You can manage multiple validator setups using custom config files:

```bash
# Create different configs for different validator pairs
cp config.example.yaml ~/configs/mainnet-validators.yaml
cp config.example.yaml ~/configs/testnet-validators.yaml

# Use specific config
svs --config ~/configs/mainnet-validators.yaml status
svs -c ~/configs/testnet-validators.yaml switch
```

This is useful for:
- Managing multiple validator pairs
- Separating mainnet/testnet configurations
- Testing configurations before deployment

See [config.example.yaml](config.example.yaml) for the full configuration template.

### Telegram Alerts Setup (Optional)

To enable Telegram notifications:

1. **Create a Telegram Bot**:

   - Message [@BotFather](https://t.me/botfather) on Telegram
   - Send `/newbot` and follow the prompts
   - Save the bot token (looks like `123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11`)

2. **Get Your Chat ID**:

   - Add the bot to a group or start a private chat with it
   - Send a message to the bot
   - Visit `https://api.telegram.org/bot<YOUR_BOT_TOKEN>/getUpdates`
   - Find your chat ID in the response (negative for groups, positive for private chats)

3. **Configure in config.yaml**:

   ```yaml
   alert_config:
     enabled: true
     delinquency_threshold_seconds: 30 # Alert after 30 seconds without voting
     telegram:
       bot_token: "YOUR_BOT_TOKEN"
       chat_id: "YOUR_CHAT_ID"
   ```

4. **Test Your Configuration**:
   ```bash
   svs test-alert
   ```

You'll receive notifications for:

- **Validator Delinquency** (CRITICAL): When your validator stops voting for more than 30 seconds
  - Only triggers when SSH and RPC are both working (no false alarms)
  - Includes SSH and RPC connection status in the alert
- **SSH Connection Failures** (LOW PRIORITY): When SSH connections fail repeatedly
  - Triggers after 100 consecutive failures or 30 minutes of failures
  - Very loose thresholds to avoid noise
- **RPC Connection Failures** (LOW PRIORITY): When RPC calls fail due to throttling or network issues
  - Triggers after 100 consecutive failures or 30 minutes of failures
  - Very loose thresholds to avoid noise
- **Switch Results**: Success/failure notifications with timing details

## Key Features

- **Ultra-Fast Hot Swap**: Instant 1-3 second hot swap operations with optimized streaming
- **Automated Failover**: Automatic failover when primary validator goes down
- **Runtime Status Detection**: Continuous monitoring for failover readiness
- **SSH Connection Pooling**: Persistent connections enable instant hot swap execution
- **Optimized Tower Transfer**: Lightning-fast tower hot swap via streaming operations
- **Universal Hot Swap Support**: Works with Firedancer, Agave, Solana, and Jito validators
- **Interactive Dashboard**: Real-time monitoring for hot swap and failover operations
  - **Hot swap controls** - (S)witch for manual hot swap, auto-failover on detection
  - **Multi-validator support** - Tab key to monitor multiple hot swap pairs
  - Failover readiness indicators for both nodes
  - Manual hot swap trigger with (S) key
  - SSH connectivity monitoring for failover reliability
  - RPC health checks ensure safe hot swapping
  - Real-time hot swap status and countdown timers
- **Failover Alerts**: Telegram notifications for critical events
  - Automatic failover trigger notifications
  - Hot swap completion confirmations
  - Validator delinquency alerts (triggers failover)
  - Failover failure alerts for manual intervention
- **Hot Swap Status Display**: Live monitoring during operations
  - Active/standby validator status for hot swap readiness
  - Current validator identity for hot swap verification
  - Version compatibility checks for safe hot swapping
  - SSH and RPC health for reliable failover

## Security

- **No credential storage**: SSH private keys never leave your `~/.ssh/` directory
- **Path-only configuration**: Only file paths and hostnames stored in config files
- **No network exposure**: Tool operates through SSH connections only
- **Local execution**: All operations run locally, no external services

## Why SVS?

Built by [Huisky Labs](https://huisky.xyz/) validator team who needed reliable switching tools for our own operations. After countless manual switches and near-misses, we built what we wished existed.

- **Battle-tested**: Used in production by Huisky Labs validators
- **Community-driven**: We actively use and improve this tool daily
- **Open source**: Transparency and security through open development

### Support Development

If SVS saves you time and SOL, consider:

- ⭐ Starring this repo to help other validators find it
- 🗳️ Delegating to [Huisky Labs validators](https://huisky.xyz/)
- 🐛 Reporting issues or contributing improvements

## Roadmap

### ✅ Completed

- [x] **Ultra-fast switching** - Sub-second identity switches with optimized streaming operations
- [x] **Universal validator support** - Works with Firedancer, Agave, Solana, and Jito
- [x] **Interactive CLI** - User-friendly menu system with guided workflows
- [x] **Dry-run mode** - Test switches without executing for safety
- [x] **SSH connection pooling** - Persistent connections with multiplexing for instant commands
- [x] **Auto-detect active/standby** - Runtime detection of validator states
- [x] **Optimized tower transfer** - Streaming base64 decode + dd for minimal latency
- [x] **Interactive dashboard** - Real-time monitoring with Ratatui-based terminal UI
- [x] **Modern async architecture** - Tokio-based async runtime with Arc<Session> efficiency
- [x] **Telegram notifications** - Real-time alerts for validator health and switch events
- [x] **Continuous monitoring** - Real-time validator health monitoring with delinquency alerts
- [x] **Multi-validator support** - Manage multiple validator pairs with Tab key switching
- [x] **Ultra-responsive UI** - Dedicated keyboard thread prevents blocking, action-based processing

- [x] **Auto-switch on failure** - Automatic failover when primary validator goes down

Have ideas? [Open an issue](https://github.com/huiskylabs/solana-validator-switch/issues) or contribute!

## License

MIT License

---

<div align="center">
Built with ❤️ by <a href="https://huisky.xyz/">Huisky Labs</a> • <a href="https://github.com/huiskylabs">GitHub</a> • <a href="https://twitter.com/huiskylabs">Twitter</a>
</div>
