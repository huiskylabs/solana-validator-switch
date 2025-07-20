# Alert Behavior Summary

## Alert Thresholds
- **Delinquency**: 30 seconds (CRITICAL - tight threshold)
- **SSH Failures**: 30 minutes (LOW PRIORITY - loose threshold)  
- **RPC Failures**: 30 minutes (LOW PRIORITY - loose threshold)

## Key Behaviors

### 1. Time-Based Thresholds Only
- Removed count-based thresholds for simplicity
- Alerts fire based on continuous failure duration only
- SSH/RPC must fail continuously for 30 minutes to trigger alert

### 2. Timer Reset on Success
- When SSH/RPC succeeds, the failure timer is **completely reset**
- Example: Fail for 20 minutes → Success → Timer resets to 0
- Next failure starts a fresh 30-minute countdown
- This prevents false alarms from accumulated old failures

### 3. Alert Cooldown Periods
- **High Severity (Delinquency)**: 15-minute cooldown
  - Critical alerts need faster re-notification
- **Low Severity (SSH/RPC)**: 30-minute cooldown  
  - Infrastructure alerts are less urgent
- Prevents alert spam during extended outages
- Each validator and node has independent cooldowns

### 4. Delinquency Alerts Only When Infrastructure Works
- Delinquency alerts ONLY fire when:
  - Validator hasn't voted for 30+ seconds AND
  - SSH is working (no consecutive failures) AND  
  - RPC is working (no consecutive failures)
- This prevents false delinquency alerts during infrastructure issues

## Example Scenarios

1. **SSH fails for 25 minutes, then recovers**
   - No alert sent (didn't reach 30 minutes)
   - Timer resets on recovery
   
2. **RPC fails for 35 minutes continuously**
   - Alert sent at 30 minutes
   - No more RPC alerts for 30 minutes (low severity cooldown)
   
3. **Validator not voting + RPC is down**
   - No delinquency alert (can't verify voting status)
   - Only RPC failure alert after 30 minutes

4. **Multiple short failures**
   - Fail 10 min → Success → Fail 10 min → Success
   - No alerts (each failure period < 30 minutes)
   - Timer resets after each success