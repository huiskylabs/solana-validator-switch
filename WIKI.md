# Solana Validator Switch Wiki

Ultra-fast validator switching tool for Solana validators.

## Features

- ⚡ Sub-second validator switching
- 🔍 Auto-detects validator type (Firedancer/Agave/Jito)
- 📱 Telegram notifications
- 🛡️ Secure SSH-based operations

---

## Installation

### Download Binary

```bash
# Linux
curl -L https://github.com/huiskylabs/solana-validator-switch/releases/latest/download/svs-linux-x86_64.tar.gz | tar -xz
sudo mv svs /usr/local/bin/

# macOS Intel
curl -L https://github.com/huiskylabs/solana-validator-switch/releases/latest/download/svs-macos-x86_64.tar.gz | tar -xz
sudo mv svs /usr/local/bin/

# macOS Apple Silicon
curl -L https://github.com/huiskylabs/solana-validator-switch/releases/latest/download/svs-macos-aarch64.tar.gz | tar -xz
sudo mv svs /usr/local/bin/
```

### Setup

```bash
# Create config directory
mkdir -p ~/.solana-validator-switch

# Download config template
curl -L https://raw.githubusercontent.com/huiskylabs/solana-validator-switch/main/config.example.yaml \
  -o ~/.solana-validator-switch/config.yaml

# Edit config
nano ~/.solana-validator-switch/config.yaml
```

### Verify

```bash
svs --version
```

---

## Configuration

### Basic Configuration

Edit `~/.solana-validator-switch/config.yaml`:

```yaml
validators:
  - votePubkey: YOUR_VOTE_ACCOUNT_PUBKEY
    identityPubkey: YOUR_VALIDATOR_IDENTITY_PUBKEY
    rpc: https://api.mainnet-beta.solana.com
    nodes:
      - label: validator-1
        host: 10.0.0.1
        user: solana
        paths:
          fundedIdentity: /home/solana/funded-validator-keypair.json
          unfundedIdentity: /home/solana/unfunded-validator-keypair.json
          voteKeypair: /home/solana/vote-account-keypair.json
      
      - label: validator-2
        host: 10.0.0.2
        user: solana
        paths:
          fundedIdentity: /home/solana/funded-validator-keypair.json
          unfundedIdentity: /home/solana/unfunded-validator-keypair.json
          voteKeypair: /home/solana/vote-account-keypair.json
```

### Telegram Alerts (Optional)

```yaml
alert_config:
  enabled: true
  delinquency_threshold_seconds: 30
  telegram:
    bot_token: "YOUR_BOT_TOKEN"
    chat_id: "YOUR_CHAT_ID"
```

### SSH Requirements

- Key-based authentication required
- Common key locations auto-detected: `~/.ssh/id_rsa`, `~/.ssh/id_ed25519`
- Test SSH access: `ssh user@host`

---

## Usage

### Interactive Mode (Recommended)

```bash
svs
```

Navigate the menu to:
- Check status
- Perform switch
- Test alerts

### Command Line Mode

```bash
svs status              # Check validator status
svs switch              # Perform validator switch
svs switch --dry-run    # Preview switch without executing
svs test-alert          # Test Telegram alerts
```

### Status Display

The status command shows:
- Validator type and version
- Active/Standby status
- Vote status with slot info
- Catchup status with countdown
- Alert configuration
- Swap readiness

### Switch Operation

1. **Pre-flight checks** - Verifies both nodes are ready
2. **Active → Unfunded** - Switches active node to unfunded identity
3. **Tower transfer** - Copies tower file to standby
4. **Standby → Funded** - Switches standby to funded identity
5. **Verification** - Confirms new active is voting

Total time: ~1 second average

### Keyboard Shortcuts

- `q` or `Esc` - Quit
- `Enter` - Select menu item
- Arrow keys - Navigate

---

## Telegram Alerts

### Setup

#### 1. Create Bot

- Message [@BotFather](https://t.me/botfather)
- Send `/newbot`
- Save the token

#### 2. Get Chat ID

- Add bot to group or start chat
- Send a test message
- Visit: `https://api.telegram.org/bot<TOKEN>/getUpdates`
- Find `"chat":{"id":-123456789}`

#### 3. Configure

```yaml
alert_config:
  enabled: true
  delinquency_threshold_seconds: 30
  telegram:
    bot_token: "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11"
    chat_id: "-123456789"  # Negative for groups
```

#### 4. Test

```bash
svs test-alert
```

### Alert Types

- **Delinquency Alert** - Validator stops voting > 30s
- **Catchup Failure** - Standby fails 3 consecutive checks
- **Switch Result** - Success/failure notifications

### Cooldowns

- 5-minute cooldown between same alerts
- Prevents notification spam
- Resets on recovery

---

## FAQ

### General

**Q: What validators does SVS support?**  
A: Firedancer, Agave, Jito, and Solana validators. Auto-detected at runtime.

**Q: How fast is the switch?**  
A: Average ~1 second for the identity switch. Full operation including verification: 30-45 seconds.

**Q: Does it work with multiple validators?**  
A: Yes, configure multiple validator pairs in config.yaml.

### Troubleshooting

**Q: SSH connection failed**  
A: Ensure key-based SSH works: `ssh user@host`. Check firewall rules.

**Q: Swap not ready**  
A: Verify all keypair files exist and are readable. Check tower file exists in ledger.

**Q: Telegram alerts not working**  
A: Run `svs test-alert`. Check bot token and chat ID are correct.

**Q: Status not updating after switch**  
A: Fixed in v1.2.0. Update to latest version.

### Security

**Q: Are my keys safe?**  
A: Yes. SVS only stores paths to keys, never the keys themselves.

**Q: What ports are needed?**  
A: Only SSH (port 22 by default) to your validator nodes.

**Q: Can I use password authentication?**  
A: No, only SSH key authentication is supported for security.

---

## Support

- [GitHub Issues](https://github.com/huiskylabs/solana-validator-switch/issues)
- Twitter: [@huiskylabs](https://twitter.com/huiskylabs)