# Automatic Failover Implementation

## Overview
This implementation adds automatic failover capability to the Solana Validator Switch tool. When enabled, it will automatically switch to the standby validator if the active validator stops voting for the configured threshold (default 30 seconds).

## Key Safety Features

### 1. **RPC Connectivity Verification**
- Auto-failover ONLY triggers when RPC is confirmed working
- We need RPC to verify on-chain that the validator is not voting
- SSH may be down if the primary node is completely offline
- This ensures the delinquency detection is accurate and not due to RPC monitoring failures

### 2. **Startup Identity Safety Check**
- Validators MUST NOT start with their authorized voter identity
- On startup, svs checks the validator configuration:
  - **Firedancer**: Verifies `identity_path` differs from `authorized_voter_paths[0]`
  - **Agave/Jito**: Verifies `--identity` differs from `--authorized-voter`
- This prevents accidental double-signing during normal switch operations
- The startup identity will be switched to the authorized voter during failover

### 3. **Unfunded Identity Requirement**
- Validators MUST start with unfunded identity when auto-failover is enabled
- On startup, svs checks that validators are NOT running with funded identity
- This prevents double-signing if a validator unexpectedly restarts

### 4. **Optional Steps with Fallback**
- Primary switch to unfunded: Optional (continues if it fails)
- Tower file copy: Optional (continues if it fails)
- Standby activation: Required (must succeed)
- This ensures failover can succeed even if the primary node is unresponsive

## Configuration

### config.yaml
```yaml
alert_config:
  enabled: true
  delinquency_threshold_seconds: 30
  
  # Enable automatic failover
  auto_failover_enabled: true
  
  auto_failover_config:
    # Enforce safety check on startup
    require_unfunded_on_startup: true
    
    # Optional step configuration
    max_optional_step_retries: 2
    optional_step_timeout_seconds: 10
```

### Validator Startup Configuration
**For Agave/Jito:**
```bash
solana-validator --identity /path/to/unfunded-validator-keypair.json ...
```

**For Firedancer:**
```toml
identity_path = "/path/to/unfunded-validator-keypair.json"
```

## How It Works

1. **Monitoring**: Status UI continuously monitors validator voting via RPC
2. **Detection**: When validator hasn't voted for 30+ seconds AND RPC is working
3. **Alert**: Sends delinquency alert via Telegram  
4. **Failover**: If auto-failover is enabled, initiates emergency takeover:
   - Try to switch primary to unfunded (optional - may fail if SSH is down)
   - Try to copy tower file (optional - may fail if SSH is down)
   - Switch standby to funded identity (required - must succeed)
5. **Notification**: Sends detailed Telegram alert with results

## Emergency Takeover Alert Format

### Success:
```
🚨 EMERGENCY TAKEOVER INITIATED

Validator: <identity>
Reason: Not voting for 30+ seconds with confirmed connectivity

Previous Active: node-1 ❌
New Active: node-2 ✅

Optional Steps:
• Primary → Unfunded: ✅ Success
• Tower Copy: ❌ Failed (continued)

Required Step:
• Standby → Funded: ✅ Success

Takeover completed in: 3500ms

⚠️ VERIFY VALIDATOR STATUS IMMEDIATELY
```

### Failure:
```
❌ EMERGENCY TAKEOVER FAILED

Validator: <identity>
Reason: Not voting with confirmed connectivity

Previous Active: node-1 ❌
Attempted New Active: node-2 ❌

Optional Steps:
• Primary → Unfunded: ❌
• Tower Copy: ❌
• Standby → Funded: ❌

Error: Failed to activate standby: SSH connection failed
Duration: 15000ms

⚠️ MANUAL INTERVENTION REQUIRED
```

## Testing

Run the auto-failover tests:
```bash
cargo test auto_failover_tests
```

## Important Notes

1. **Double-Sign Prevention**: The unfunded identity requirement is critical for safety
2. **Manual Verification**: Always verify validator status after automatic failover
3. **Alert Cooldowns**: Delinquency alerts have 15-minute cooldown after failover
4. **Network Conditions**: Ensure stable network connectivity for reliable failover
5. **Testing**: Thoroughly test in devnet/testnet before enabling on mainnet

## Troubleshooting

### "SAFETY CHECK FAILED: has identity_path same as authorized_voter_paths" (Firedancer)
- The `identity_path` in config matches `authorized_voter_paths[0]`
- This is unsafe as it could lead to double-signing
- Edit the Firedancer config file and set `identity_path` to a different keypair

### "SAFETY CHECK FAILED: has --identity same as --authorized-voter" (Agave/Jito)
- The `--identity` command line argument matches `--authorized-voter`
- This is unsafe as it could lead to double-signing
- Update the startup command to use different keypairs for these arguments

### "SAFETY CHECK FAILED: running with FUNDED identity"
- Validator is running with funded identity (when auto-failover is enabled)
- Stop validator, change to unfunded identity, restart

### Failover Not Triggering
- Check SSH/RPC connectivity
- Verify auto_failover_enabled: true
- Check delinquency threshold settings

### False Failovers
- Increase delinquency_threshold_seconds
- Check network stability
- Verify RPC endpoint reliability