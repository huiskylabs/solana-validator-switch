# Alert Threshold Update Summary

## Changes Made

Updated the default alert thresholds to prevent noisy infrastructure alerts:

### Before (Too Noisy)
- SSH failures: Alert after 5 failures or 60 seconds
- RPC failures: Alert after 10 failures or 30 seconds

### After (Much Looser)
- SSH failures: Alert after 30 minutes (1800 seconds) - TIME-BASED ONLY
- RPC failures: Alert after 30 minutes (1800 seconds) - TIME-BASED ONLY
- Delinquency: Still 30 seconds (CRITICAL - unchanged)

## Why This Change?

1. **SSH/RPC are LOW PRIORITY**: These are informational alerts, not critical
2. **Avoid Alert Fatigue**: Infrastructure can have temporary hiccups
3. **Focus on What Matters**: Delinquency alerts (validator not voting) are the only critical alerts

## Updated Files

1. `src/types.rs` - Changed default threshold functions
2. `config.example.yaml` - Updated example with new values and comments
3. `README.md` - Updated documentation to reflect new thresholds
4. All test files - Updated to use new realistic thresholds

## Alert Priority Guide

```
🚨 CRITICAL (30 seconds):
   - Validator Delinquency (only when SSH ✅ and RPC ✅)

📊 LOW PRIORITY (30 minutes):
   - SSH Connection Failures
   - RPC Connection Failures
```

## Configuration

Users can still customize thresholds in their config:

```yaml
alert_config:
  enabled: true
  delinquency_threshold_seconds: 30   # CRITICAL
  ssh_failure_threshold_seconds: 1800 # 30 minutes
  rpc_failure_threshold_seconds: 1800 # 30 minutes
```

## Benefits

1. **No More False Alarms**: Infrastructure alerts won't wake you up at night
2. **Clear Signal**: When you get a delinquency alert, it's real
3. **Peace of Mind**: Temporary network issues won't trigger alerts
4. **Still Informed**: You'll know about persistent infrastructure issues (after 30 mins)