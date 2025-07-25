# Solana Validator Switch CLI - UX Specification

## Core Value Proposition (Refined)

**"Professional-grade validator switching from your terminal with zero stored credentials"**

## Installation & First Run

### NPM Installation

```bash
# Global installation
npm install -g solana-validator-switch

# Launch interactive CLI (both commands work)
solana-validator-switch
svs

# Or use specific commands
solana-validator-switch setup
svs setup

solana-validator-switch monitor
svs monitor

solana-validator-switch switch
svs switch
```

### Package Discovery Experience

```bash
$ svs

┌─────────────────────────────────────────────────────────────┐
│  🚀 Welcome to Solana Validator Switch CLI v1.0.0          │
│  Professional-grade validator switching from your terminal  │
└─────────────────────────────────────────────────────────────┘

📋 No configuration found. Let's get you set up!

This interactive CLI tool provides:
✅ Ultra-fast validator switching (~300ms)
✅ Real-time monitoring dashboard
✅ SSH key-based authentication only
✅ Zero credential storage
✅ Browser-less operation
✅ Professional operator focused

Press ENTER to start setup or Ctrl+C to exit...
```

## Command Structure

### Core Commands

```bash
# Setup and configuration
svs setup                      # Initial setup wizard
svs config                     # Edit configuration
svs config --list              # Show current config
svs config --test              # Test all connections

# Monitoring
svs monitor                    # Full dashboard (default)
svs status                     # Quick status check
svs health                     # Detailed health report
svs watch                      # Continuous status updates

# Switch operations
svs switch                     # Interactive switch with prompts
svs switch --auto              # Auto-switch if conditions met

# Information
svs nodes                      # List configured nodes
svs logs                       # View recent logs
svs history                    # Switch history
svs version                    # Version information
```

## Security-First Setup Flow (3-Step Process)

### Step 1: Security Overview & SSH Key Setup

```bash
$ svs setup

┌─────────────────────────────────────────────────────────────┐
│  🔒 Security Setup - Step 1 of 3                           │
└─────────────────────────────────────────────────────────────┘

Welcome to Solana Validator Switch CLI!

This application uses SSH key-based authentication only:

✅ Your SSH private keys stay on your machine
✅ No passwords or credentials stored anywhere
✅ No server-side credential storage
✅ Configuration stored locally only
✅ Works with existing SSH key infrastructure

SECURITY MODEL:
• CLI connects directly to your validator nodes
• Uses your existing SSH keys for authentication
• All operations happen through secure SSH tunnels
• Zero trust: no intermediate servers or services

📍 Detected SSH keys on your system:
  ~/.ssh/id_rsa         (RSA, 4096 bits) ✅
  ~/.ssh/id_ed25519     (ED25519) ✅
  ~/.ssh/validator_key  (RSA, 2048 bits) ✅

? Which SSH key would you like to use?
  > ~/.ssh/id_rsa
    ~/.ssh/id_ed25519
    ~/.ssh/validator_key
    Browse for different key...
    Generate new key pair...

Press ENTER to continue or Ctrl+C to exit...
```

### Step 2: Node Configuration & Auto-Detection

```bash
┌─────────────────────────────────────────────────────────────┐
│  ⚙️ Node Configuration - Step 2 of 3                       │
└─────────────────────────────────────────────────────────────┘

🔧 Let's configure your validator nodes:

PRIMARY NODE:
? Host/IP: 192.168.1.10
? SSH Port: 22
? SSH User: solana
? Label: prod-main

🔍 Testing connection... ✅ Connected successfully!
🔍 Detecting validator client... 🔥 Agave v2.2.0 detected
🔍 Auto-detecting file paths...

✅ Found validator configuration:
  Funded identity: /home/solana/funded-validator-keypair.json
  Unfunded identity: /home/solana/unfunded-keypair.json
  Ledger path: /home/solana/ledger
  Tower file: /home/solana/tower.bin
  Solana CLI: /home/solana/.local/share/solana/install/active_release/bin/solana

BACKUP NODE:
? Host/IP: 192.168.1.11
? SSH Port: 22
? SSH User: solana
? Label: prod-backup

🔍 Testing connection... ✅ Connected successfully!
🔍 Detecting validator client... ⚡ Firedancer v0.103 detected
🔍 Auto-detecting file paths...

✅ Found validator configuration:
  Funded identity: /home/solana/funded-validator-keypair.json
  Unfunded identity: /home/solana/unfunded-keypair.json
  Ledger path: /home/solana/ledger
  Tower file: /home/solana/tower.bin
  Solana CLI: /home/solana/.local/share/solana/install/active_release/bin/solana

🌐 RPC ENDPOINT CONFIGURATION:
? RPC Endpoint:
  > Use Solana Mainnet Beta (https://api.mainnet-beta.solana.com)
    Use custom RPC endpoint

? Custom RPC endpoint: [Skip - using default]

⚠️  NOTICE: Different validator clients detected (Agave vs Firedancer)
   This is supported but may have different performance characteristics
   during switch operations.

? All configurations look correct? (Y/n)
? Save configuration? (Y/n)

💾 Configuration saved to ~/.solana-validator-switch/config.json
```

### Step 3: Connection Verification & Final Setup

```bash
┌─────────────────────────────────────────────────────────────┐
│  🔒 Connection Verification - Step 3 of 3                  │
└─────────────────────────────────────────────────────────────┘

🔍 Performing comprehensive connection verification...

SSH CONNECTION TESTS:
✅ Primary node SSH connection successful
✅ Backup node SSH connection successful
✅ SSH key authentication working
✅ Command execution permissions verified

RPC ENDPOINT TESTS:
✅ RPC endpoint responding (https://api.mainnet-beta.solana.com)
✅ Validator status queries working
✅ Network connectivity confirmed

FILE SYSTEM ACCESS TESTS:
✅ Funded identity keypairs accessible on both nodes
✅ Unfunded identity keypairs accessible on both nodes
✅ Ledger directories readable on both nodes
✅ Tower files accessible on both nodes
✅ Solana CLI available on both nodes

VALIDATOR CLIENT VERIFICATION:
✅ Primary validator client: 🔥 Agave v2.2.0 (confirmed)
✅ Backup validator client: ⚡ Firedancer v0.103 (confirmed)
✅ Both clients responding to status queries
✅ Identity verification successful

SECURITY VERIFICATION:
✅ SSH connections encrypted and authenticated
✅ No credentials stored by this application
✅ All operations use secure SSH tunnels
✅ Configuration contains no sensitive data

🎉 All systems verified and ready!

Setup complete! Launching interactive CLI...

Press ENTER to continue...
```

## Interactive CLI Context (After Setup)

### Main Dashboard - Always Active

```bash
$ svs monitor

┌─────────────────────────────────────────────────────────────┐
│  🟢 Solana Validator Switch CLI v1.0.0                     │
│  🔒 SSH Key Auth Active                                     │
│  🔄 Auto-refresh: ON (every 10s) | Last: 2024-07-06 15:42:33│
└─────────────────────────────────────────────────────────────┘

╭─ PRIMARY NODE ──────────────────╮ ╭─ BACKUP NODE ───────────────────╮
│  🟢 ACTIVE   prod-main          │ │  🟡 STANDBY  prod-backup        │
│  📡 192.168.1.10:22             │ │  📡 192.168.1.11:22             │
│  🔥 Agave v2.2.0                │ │  ⚡ Firedancer v0.103           │
│                                 │ │                                 │
│  🎯 Slot: 245,123,890           │ │  🎯 Slot: 245,123,885           │
│  📊 Vote Dist: 1                │ │  📊 Vote Dist: 6                │
│  💚 Node Health: 🟢 Healthy     │ │  💚 Node Health: 🟢 Healthy     │
│  ⏱️  Last Vote: 2s ago          │ │  ⏱️  Last Vote: 8s ago          │
│  📈 Uptime: 99.8%               │ │  📈 Uptime: 99.9%               │
│                                 │ │                                 │
│  🔑 Identity: B7Kx...9Mz4       │ │  🔑 Identity: C8Ly...1Az5       │
│  💰 Status: FUNDED   ✅         │ │  💰 Status: UNFUNDED ❌         │
│  🗳️  Voting: ACTIVE  ✅         │ │  🗳️  Voting: STANDBY 🟡         │
│  🔗 SSH: ✅ Connected           │ │  🔗 SSH: ✅ Connected           │
│                                 │ │                                 │
│  💾 Disk: 89% (⚠️ Warning)      │ │  💾 Disk: 45% ✅               │
│  🧠 RAM:  67% ✅                │ │  🧠 RAM:  72% ✅               │
│  🔄 CPU:  45% ✅                │ │  🔄 CPU:  38% ✅               │
╰─────────────────────────────────╯ ╰─────────────────────────────────╯

🌐 RPC: https://api.mainnet-beta.solana.com ✅

┌─ SWITCH READINESS ──────────────────────────────────────────┐
│  🚦 Status: 🟢 READY TO SWITCH                             │
│  📊 Backup is 5 slots behind primary (within safe range)   │
│  ⏱️  Estimated switch time: 30-45 seconds                  │
│  ⚠️  Warning: Primary disk usage high (89%)                │
└─────────────────────────────────────────────────────────────┘

┌─ RECENT ACTIVITY ───────────────────────────────────────────┐
│  15:42:33 ✅ Health check passed for both nodes            │
│  15:42:30 📊 Backup synchronized (5 slots behind)          │
│  15:42:25 ⚠️  Primary disk usage: 89%                      │
│  15:40:15 🔄 Last switch: prod-main → prod-backup (42s)    │
│  15:35:22 📈 Network conditions: Stable                    │
└─────────────────────────────────────────────────────────────┘

┌─ COMMANDS ──────────────────────────────────────────────────┐
│  (S)witch validator                  (Q)uit                     │
│  (R)efresh now (in 8s)              (C)onfiguration             │
│  (H)ealth details                   (L)ogs                      │
│  (T)oggle auto-refresh              (W)atch mode               │
│  (?)  Help                          (X)  Exit CLI               │
└─────────────────────────────────────────────────────────────┘

Command: _
```

### Interactive Command Processing

```bash
Command: s

┌─────────────────────────────────────────────────────────────┐
│  🔄 Switch Validator                                        │
└─────────────────────────────────────────────────────────────┘

? Switch from prod-main to prod-backup? (y/N) y

Starting switch operation...
```

```bash
Command: c

┌─────────────────────────────────────────────────────────────┐
│  ⚙️ Configuration Menu                                      │
└─────────────────────────────────────────────────────────────┘

? What would you like to configure?
  > Node settings
    RPC endpoint
    Monitoring preferences
    Display options
    Test connections
    Back to dashboard

Select option [1-6]:
```

```bash
Command: t

🔄 Auto-refresh toggled OFF
⏱️  Refresh interval: 10 seconds
📊 Last refresh: 15:42:33

Press any key to continue...
```

```bash
Command: h

┌─────────────────────────────────────────────────────────────┐
│  🏥 Health Details                                          │
└─────────────────────────────────────────────────────────────┘

[Health information displayed...]

Commands: [r]efresh | [b]ack to dashboard | [q]uit

Command: _
```

### Health Scoring System (Simplified)

```bash
$ svs health

┌─────────────────────────────────────────────────────────────┐
│  🏥 Node Health Status                                      │
└─────────────────────────────────────────────────────────────┘

🔍 PRIMARY NODE (prod-main):
┌─────────────────────────────────────────────────────────────┐
│  🗳️  VOTING STATUS                                          │
│    Vote Distance: 1 slot         ✅ Excellent (0-3)        │
│    Last Vote: 2 seconds ago      ✅ Recent                 │
│    Voting Status: Active         ✅ Healthy                │
│                                                             │
│  🖥️  SYSTEM RESOURCES                                       │
│    CPU Usage: 45%                ✅ Normal                  │
│    RAM Usage: 67%                ✅ Normal                  │
│    Disk Usage: 89%               ⚠️  Warning (>85%)        │
│                                                             │
│  🔒 CONNECTION STATUS                                       │
│    SSH Connection: Active        ✅ Connected               │
│    Ledger Path: Verified         ✅ Accessible             │
│    Tower File: Verified          ✅ Accessible             │
│    Identity Files: Verified      ✅ Accessible             │
└─────────────────────────────────────────────────────────────┘

🔍 BACKUP NODE (prod-backup):
┌─────────────────────────────────────────────────────────────┐
│  🗳️  VOTING STATUS                                          │
│    Vote Distance: 6 slots        🟡 Good (4-10)           │
│    Catchup Status: Synced        ✅ Ready                 │
│    Voting Status: Standby        🟡 Ready                 │
│                                                             │
│  🖥️  SYSTEM RESOURCES                                       │
│    CPU Usage: 38%                ✅ Normal                  │
│    RAM Usage: 72%                ✅ Normal                  │
│    Disk Usage: 45%               ✅ Good                   │
│                                                             │
│  🔒 CONNECTION STATUS                                       │
│    SSH Connection: Active        ✅ Connected               │
│    Ledger Path: Verified         ✅ Accessible             │
│    Tower File: Verified          ✅ Accessible             │
│    Identity Files: Verified      ✅ Accessible             │
└─────────────────────────────────────────────────────────────┘

🚦 SWITCH READINESS ANALYSIS:
┌─────────────────────────────────────────────────────────────┐
│  Status: 🟢 READY TO SWITCH                                │
│                                                             │
│  ✅ Primary node is voting normally                        │
│  ✅ Backup node is synchronized (6 slots behind)           │
│  ✅ Both nodes have adequate system resources               │
│  ✅ SSH connections are active                             │
│  ✅ All file paths are accessible                          │
│  ⚠️  Primary disk usage is high (89%) - monitor closely    │
│                                                             │
│  Estimated switch time: 30-45 seconds                      │
│  Risk level: 🟢 LOW                                        │
└─────────────────────────────────────────────────────────────┘

Commands: [r]efresh | [b]ack to dashboard | [q]uit

Command: _
```

## The Switch Experience (Simplified & Fast)

### Pre-Switch Check (Basic Version)

```bash
$ svs switch

┌─────────────────────────────────────────────────────────────┐
│  🔄 Switch Validator - Pre-flight Check                     │
└─────────────────────────────────────────────────────────────┘

🔍 Checking current state...

📊 CURRENT STATUS:
✅ Primary node voting normally (vote distance: 1)
✅ Backup node synchronized (6 slots behind)
✅ SSH connections active
✅ All file paths accessible
⚠️  Primary disk usage high (89%)

🔄 SWITCH PLAN:
┌─────────────────────────────────────────────────────────────┐
│  Will switch FROM: prod-main (primary)                     │
│  Will switch TO:   prod-backup (backup)                    │
│                                                             │
│  1. Stop primary validator                                  │
│  2. Transfer tower file                                     │
│  3. Start backup validator                                  │
│  4. Verify backup voting                                    │
│                                                             │
│  Estimated time: 30-45 seconds                             │
│  Risk level: 🟢 LOW                                        │
└─────────────────────────────────────────────────────────────┘

? Proceed with switch? (y/N)
```

### Switch Execution (Simplified Progress)

```bash
✅ Switch confirmed. Starting execution...

┌─────────────────────────────────────────────────────────────┐
│  🔄 Switch in Progress - Step 2 of 4                       │
│  ⏱️  Elapsed: 00:23                                        │
└─────────────────────────────────────────────────────────────┘

✅ 1. Primary validator stopped                       [00:08]
🔵 2. Transferring tower file...                      [00:23]
    • Tower file: 2.3KB
    • Transfer: In progress
⏳ 3. Starting backup validator...
⏳ 4. Verifying backup voting...

🔍 STATUS:
Primary: 🔴 Stopped
Backup:  🟡 Preparing
SSH:     🟢 Connected

Press Ctrl+C for emergency stop
```

### Switch Completion (Simplified)

```bash
$ svs status

✅ Switch completed successfully!

┌─────────────────────────────────────────────────────────────┐
│  🎉 Switch Complete                                         │
└─────────────────────────────────────────────────────────────┘

📊 SUMMARY:
Total time: 42 seconds
Voting gap: 18 seconds
Success: ✅ No errors

🔄 NEW STATUS:
🟢 ACTIVE: prod-backup (192.168.1.11)
    • Voting: ✅ Active
    • Vote distance: 1
    • Status: Healthy

🟡 STANDBY: prod-main (192.168.1.10)
    • Voting: ❌ Stopped
    • Status: Ready for next switch

? What's next?
  > Return to monitoring
    View logs
    Quit

Select [1-3]:
```

## Command Structure

# Switch operations

svs switch # Interactive switch with prompts
svs switch --auto # Auto-switch if conditions met

# Information

svs nodes # List configured nodes
svs logs # View recent logs
svs history # Switch history
svs version # Version information

````

### Configuration Management
```bash
$ svs config

┌─────────────────────────────────────────────────────────────┐
│  ⚙️  Configuration Management                               │
└─────────────────────────────────────────────────────────────┘

Configuration file: ~/.solana-validator-switch/config.json

? What would you like to do?
  > Edit node settings
    Update SSH keys
    Modify monitoring settings
    Security settings
    View current config
    Test all connections
    Reset to defaults
    Back to main menu

# Or direct editing
svs config --edit       # Opens config in $EDITOR
svs config --set monitoring.interval=5
svs config --get nodes.primary.host
````

### Sample Configuration File (Updated)

```json
{
  "version": "1.0.0",
  "nodes": {
    "primary": {
      "label": "prod-main",
      "host": "192.168.1.10",
      "port": 22,
      "user": "solana",
      "keyPath": "~/.ssh/id_rsa",
      "paths": {
        "fundedIdentity": "/home/solana/funded-validator-keypair.json",
        "unfundedIdentity": "/home/solana/unfunded-keypair.json",
        "ledger": "/home/solana/ledger",
        "tower": "/home/solana/tower.bin"
      }
    },
    "backup": {
      "label": "prod-backup",
      "host": "192.168.1.11",
      "port": 22,
      "user": "solana",
      "keyPath": "~/.ssh/id_rsa",
      "paths": {
        "fundedIdentity": "/home/solana/funded-validator-keypair.json",
        "unfundedIdentity": "/home/solana/unfunded-keypair.json",
        "ledger": "/home/solana/ledger",
        "tower": "/home/solana/tower.bin"
      }
    }
  },
  "rpc": {
    "endpoint": "https://api.mainnet-beta.solana.com"
  },
  "monitoring": {
    "interval": 10,
    "healthThreshold": 5,
    "readinessThreshold": 10
  },
  "security": {
    "confirmSwitches": true,
    "sessionTimeout": 900,
    "maxRetries": 3
  },
  "display": {
    "theme": "dark",
    "compact": false,
    "showTechnicalDetails": true
  }
}
```

## Error Handling & Recovery (Professional Grade)

### Connection Errors with Intelligent Diagnostics

```bash
$ svs switch

❌ Connection failed to prod-main (192.168.1.10)

┌─────────────────────────────────────────────────────────────┐
│  🚨 Connection Error - Diagnostic Analysis                 │
└─────────────────────────────────────────────────────────────┘

🔍 ERROR DETAILS:
┌─────────────────────────────────────────────────────────────┐
│  Error: SSH connection timeout after 30 seconds            │
│  Node: prod-main (192.168.1.10:22)                        │
│  User: solana                                              │
│  Key: ~/.ssh/id_rsa                                        │
│  Timestamp: 2024-07-06 15:45:12 UTC                       │
│  Attempt: 1 of 3                                           │
└─────────────────────────────────────────────────────────────┘

🔍 AUTOMATIC DIAGNOSTICS:
┌─────────────────────────────────────────────────────────────┐
│  🔍 Network connectivity test...                           │
│      • Ping test: ❌ FAILED (Request timeout)              │
│      • Traceroute: 🔄 Running...                           │
│      • DNS resolution: ✅ 192.168.1.10 resolved           │
│                                                             │
│  🔍 SSH service availability...                            │
│      • Port 22 scan: ❌ CLOSED/FILTERED                    │
│      • Telnet test: ❌ Connection refused                  │
│      • SSH banner: ❌ No response                          │
│                                                             │
│  🔍 Authentication test...                                 │
│      • SSH key permissions: ✅ 600 (correct)              │
│      • SSH key format: ✅ Valid RSA key                   │
│      • SSH agent: ✅ Key loaded                           │
│                                                             │
│  🔍 Configuration validation...                            │
│      • Config file: ✅ Valid JSON                         │
│      • Host entry: ✅ Found                               │
│      • Port setting: ✅ 22 (standard)                     │
└─────────────────────────────────────────────────────────────┘

🛠️ TROUBLESHOOTING SUGGESTIONS:
┌─────────────────────────────────────────────────────────────┐
│  1. 🔌 NETWORK CONNECTIVITY:                                │
│     • Check if node is powered on and connected            │
│     • Verify network cable/WiFi connection                 │
│     • Test: ping 192.168.1.10                             │
│                                                             │
│  2. 🔥 FIREWALL/SECURITY:                                   │
│     • Verify SSH service is running                        │
│     • Check firewall rules on validator node               │
│     • Test: nmap -p 22 192.168.1.10                       │
│                                                             │
│  3. 🔑 SSH CONFIGURATION:                                   │
│     • Verify SSH daemon is running                         │
│     • Check SSH config allows key authentication           │
│     • Test: ssh -v solana@192.168.1.10                    │
│                                                             │
│  4. 🛡️ AUTHENTICATION:                                      │
│     • Verify SSH key is authorized on target node          │
│     • Check ~/.ssh/authorized_keys file                    │
│     • Test: ssh -i ~/.ssh/id_rsa solana@192.168.1.10      │
└─────────────────────────────────────────────────────────────┘

? What would you like to do?
  > 🔄 Retry connection (attempt 2 of 3)
    🔧 Run automated diagnostics
    🔑 Test with different SSH key
    ⚙️ Edit node configuration
    📋 View SSH connection logs
    🔍 Advanced troubleshooting
    ⏭️ Skip this node and continue
    🚪 Quit

⚠️  IMPACT: Cannot monitor primary node until connection restored
🔄 Backup monitoring continues normally
```

### Switch Failure Recovery with Smart Options

```bash
$ svs switch

❌ Switch failed at step 4: Backup validator failed to start

┌─────────────────────────────────────────────────────────────┐
│  🚨 Switch Failure - Recovery Center                       │
└─────────────────────────────────────────────────────────────┘

🔍 FAILURE ANALYSIS:
┌─────────────────────────────────────────────────────────────┐
│  Failed Operation: Backup validator startup                │
│  Error Message: "Insufficient disk space on backup node"   │
│  Error Code: ENOSPC                                        │
│  Failure Time: 2024-07-06 15:43:47 UTC                    │
│  Switch Progress: 75% complete                             │
│                                                             │
│  🔍 Root Cause Analysis:                                   │
│  • Backup node disk usage: 95% (ledger partition)         │
│  • Required space: 2.1GB (validator startup)              │
│  • Available space: 847MB (insufficient)                  │
│  • Temp files: 1.3GB (can be cleaned)                     │
└─────────────────────────────────────────────────────────────┘

🔍 CURRENT SYSTEM STATE:
┌─────────────────────────────────────────────────────────────┐
│  🟡 PRIMARY NODE: STOPPED - Safe State                     │
│      • Validator: ❌ Stopped (unfunded identity)           │
│      • Identity: C8Ly...1Az5 (unfunded keypair)           │
│      • Status: Ready for recovery                          │
│      • Risk: 🟢 LOW - No impact on stake                   │
│                                                             │
│  🔴 BACKUP NODE: FAILED TO START                           │
│      • Validator: ❌ Startup failed                        │
│      • Identity: B7Kx...9Mz4 (funded keypair loaded)      │
│      • Disk: 95% full (2.1GB needed)                      │
│      • Status: Requires intervention                       │
│                                                             │
│  🔒 STAKE SECURITY: ✅ PROTECTED                           │
│      • No risk to staked SOL                              │
│      • Validator keys secure                               │
│      • Can recover to any configuration                    │
└─────────────────────────────────────────────────────────────┘

? What would you like to do?
  > View detailed logs
    Contact support
    Exit

⚠️  VALIDATOR OFFLINE: No votes being cast
⏱️  Downtime: 4 minutes 23 seconds
```

## Configuration Management (Interactive Context)

### Configuration Main Menu

```bash
$ svs config

┌─────────────────────────────────────────────────────────────┐
│  ⚙️ Configuration Management                                │
└─────────────────────────────────────────────────────────────┘

📁 Configuration file: ~/.solana-validator-switch/config.json
📊 Last modified: 2024-07-06 14:32:15 UTC
💾 File size: 2.1KB
🔒 Permissions: 600 (owner read/write only)

Current Configuration:
┌─────────────────────────────────────────────────────────────┐
│  📋 NODES:                                                  │
│    • Primary: prod-main (192.168.1.10)                     │
│    • Backup: prod-backup (192.168.1.11)                    │
│                                                             │
│  🌐 RPC ENDPOINT:                                           │
│    • https://api.mainnet-beta.solana.com                   │
│                                                             │
│  📊 MONITORING:                                             │
│    • Refresh interval: 2 seconds                           │
│    • Health threshold: 5 slots                             │
│    • Readiness threshold: 10 slots                         │
│                                                             │
│  🎨 DISPLAY:                                                │
│    • Theme: Dark mode                                      │
│    • Compact mode: ❌ Disabled                             │
│    • Technical details: ✅ Enabled                         │
└─────────────────────────────────────────────────────────────┘

? What would you like to do?
  > Edit node settings
    Configure RPC endpoint
    Update security settings
    Modify monitoring preferences
    Change display options
    Test all connections
    Reset to defaults
    Back to dashboard

Select option [1-8]:
```

### Node Configuration (Interactive)

```bash
$ svs config --edit

┌─────────────────────────────────────────────────────────────┐
│  🔧 Node Settings                                           │
└─────────────────────────────────────────────────────────────┘

📝 PRIMARY NODE (prod-main):
┌─────────────────────────────────────────────────────────────┐
│  Host: [192.168.1.10          ] Port: [22]                 │
│  User: [solana               ] SSH Key: [~/.ssh/id_rsa]    │
│  Label: [prod-main           ]                             │
│                                                             │
│  🔍 Advanced Settings:                                      │
│    SSH Timeout: [30] seconds                               │
│    SSH Keep-alive: [5] seconds                             │
│    Max Retries: [3] attempts                               │
│    Connection Pool: [✅] Enabled                           │
└─────────────────────────────────────────────────────────────┘

📝 BACKUP NODE (prod-backup):
┌─────────────────────────────────────────────────────────────┐
│  Host: [192.168.1.11          ] Port: [22]                 │
│  User: [solana               ] SSH Key: [~/.ssh/id_rsa]    │
│  Label: [prod-backup         ]                             │
│                                                             │
│  🔍 Advanced Settings:                                      │
│    SSH Timeout: [30] seconds                               │
│    SSH Keep-alive: [5] seconds                             │
│    Max Retries: [3] attempts                               │
│    Connection Pool: [✅] Enabled                           │
└─────────────────────────────────────────────────────────────┘

Commands: [s]ave changes | [t]est connections | [r]eset | [b]ack | [q]uit

Command: _
```

### RPC Endpoint Configuration

```bash
$ svs config --rpc

┌─────────────────────────────────────────────────────────────┐
│  🌐 RPC Endpoint Configuration                              │
└─────────────────────────────────────────────────────────────┘

Current RPC Endpoint:
📡 https://api.mainnet-beta.solana.com ✅

? RPC Endpoint Options:
  > Keep current (Solana Mainnet Beta)
    Use custom RPC endpoint
    Test current endpoint
    Back to configuration menu

? Custom RPC endpoint: [Enter URL]
  Examples:
  • https://api.mainnet-beta.solana.com (default)
  • https://solana-api.projectserum.com
  • https://rpc.ankr.com/solana
  • http://your-private-rpc:8899

🔍 Testing RPC endpoint...
✅ RPC endpoint is responding (78ms)
✅ getHealth: ok
✅ getSlot: 245,123,890
✅ getVersion: 1.18.15

Commands: [s]ave | [t]est again | [b]ack | [q]uit

Command: _
```

## Session Management & Security

### Session Status and Control

```bash
$ svs session

┌─────────────────────────────────────────────────────────────┐
│  🔒 Session Management                                      │
└─────────────────────────────────────────────────────────────┘

📊 CURRENT SESSION:
┌─────────────────────────────────────────────────────────────┐
│  Started: 2024-07-06 14:32:15 UTC                          │
│  Duration: 42 minutes 18 seconds                           │
│  Timeout: 15 minutes (auto-logout)                         │
│  Activity: 🟢 Active (last command: 23s ago)               │
│  SSH Connections: 2 active                                 │
│  Commands executed: 47                                     │
│  Switches performed: 1                                     │
└─────────────────────────────────────────────────────────────┘

🔐 SECURITY STATUS:
┌─────────────────────────────────────────────────────────────┐
│  Authentication: ✅ SSH key verified                       │
│  Encryption: ✅ AES-256-GCM active                         │
│  Integrity: ✅ All connections secure                      │
│  Audit trail: ✅ Logging enabled                           │
│  Session lock: ❌ Not locked                               │
└─────────────────────────────────────────────────────────────┘

🌐 CONNECTION STATUS:
┌─────────────────────────────────────────────────────────────┐
│  prod-main (192.168.1.10):                                 │
│    • Status: 🟢 Connected (38s ago)                        │
│    • Latency: 42ms (excellent)                             │
│    • Commands: 23 executed                                 │
│    • Errors: 0                                             │
│                                                             │
│  prod-backup (192.168.1.11):                               │
│    • Status: 🟢 Connected (41s ago)                        │
│    • Latency: 35ms (excellent)                             │
│    • Commands: 24 executed                                 │
│    • Errors: 0                                             │
└─────────────────────────────────────────────────────────────┘

? Session actions:
  > Extend session (add 15 minutes)
    Lock session (require password)
    Refresh all connections
    View session logs
    Export session data
    Logout and cleanup
    Change timeout settings
    Security audit

Session expires in 12 minutes 42 seconds...
```

## Command Line Options

### Global Options

```bash
  -c, --config PATH     Use custom config file
  -v, --verbose         Verbose output
  -q, --quiet           Quiet mode (errors only)
  -j, --json            JSON output format
  -h, --help            Show help
  --version             Show version
  --no-color            Disable colored output
  --timeout SECONDS     SSH timeout (default: 30)
```

### Command Examples

```bash
# Setup
svs setup

# Configuration
svs config --list
svs config --edit

# Monitoring
svs monitor
svs status
svs watch

# Switching
svs switch

# Information
svs health
svs logs
```

### Environment Variables

```bash
export SVS_CONFIG_PATH=~/.solana-validator-switch/config.json
export SVS_SSH_TIMEOUT=30
export SVS_LOG_LEVEL=info
export SVS_NO_COLOR=true
```

This CLI design maintains the core powerful features while providing a streamlined, keyboard-driven experience perfect for professional validator operators who prefer terminal interfaces.
