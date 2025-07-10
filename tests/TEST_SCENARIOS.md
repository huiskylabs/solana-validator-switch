# Validator Switch Test Scenarios

This document outlines all test scenarios for the validator switch functionality to ensure robust error handling and excellent UX.

## 1. Happy Path Scenarios

### 1.1 Agave to Agave Switch
- Active Agave validator switches to unfunded identity
- Tower file transfers successfully
- Standby Agave validator switches to funded identity
- Catchup verification succeeds
- Timing displayed correctly

### 1.2 Firedancer to Firedancer Switch
- Proper fdctl path detection
- Config file auto-detection from process
- Identity switch using fdctl commands
- Tower file compatibility

### 1.3 Mixed Validator Types
- Agave (active) to Firedancer (standby)
- Firedancer (active) to Agave (standby)

## 2. Edge Cases

### 2.1 Missing Components
- [ ] Missing tower file on active node
- [ ] Missing executable paths (fdctl not found)
- [ ] Missing config files (firedancer config)
- [ ] Missing identity keypair files
- [ ] Incorrect file permissions

### 2.2 Network Issues
- [ ] SSH connection timeout during switch
- [ ] Network interruption during tower transfer
- [ ] Slow network (transfer takes > 10 seconds)
- [ ] Connection lost after active switch but before standby

### 2.3 Validator State Issues
- [ ] Validators significantly out of sync (>1000 slots)
- [ ] Active validator not running
- [ ] Standby validator not running
- [ ] Validator using unexpected identity

### 2.4 Configuration Issues
- [ ] Mismatched paths between nodes
- [ ] Invalid SSH keys
- [ ] Wrong user permissions
- [ ] Firewall blocking connections

## 3. Failure Scenarios

### 3.1 Command Failures
- [ ] Identity switch command fails on active
- [ ] Identity switch command fails on standby
- [ ] Tower transfer fails (disk full, permission denied)
- [ ] Catchup command fails or times out

### 3.2 Partial Failures
- [ ] Active switches but standby fails
- [ ] Tower transfers but standby switch fails
- [ ] Everything succeeds but catchup fails

### 3.3 Concurrent Operations
- [ ] Multiple switch attempts simultaneously
- [ ] Switch during validator restart
- [ ] Switch during ledger snapshot

## 4. UX Test Scenarios

### 4.1 User Interactions
- [ ] User cancels at confirmation prompt
- [ ] User enters invalid input
- [ ] Terminal resize during operation
- [ ] Ctrl+C during switch

### 4.2 Message Clarity
- [ ] Error messages are actionable
- [ ] Progress indicators accurate
- [ ] Timing displays are meaningful
- [ ] Recovery instructions clear

### 4.3 Edge Case UX
- [ ] Very long validator names
- [ ] Non-ASCII characters in paths
- [ ] Color display in non-color terminals

## 5. Performance Scenarios

### 5.1 Timing Benchmarks
- [ ] Identity switch < 5 seconds
- [ ] Tower transfer < 1 second (typical)
- [ ] Total operation < 15 seconds
- [ ] Large tower file (>100MB) handling

### 5.2 Resource Usage
- [ ] Memory usage remains reasonable
- [ ] No file descriptor leaks
- [ ] SSH connections properly cleaned up

## 6. Recovery Scenarios

### 6.1 Post-Failure Recovery
- [ ] Clear state after partial switch
- [ ] Manual recovery instructions work
- [ ] Idempotent operations (can retry safely)

### 6.2 Rollback Scenarios
- [ ] Can switch back immediately
- [ ] No data corruption after failed switch
- [ ] Tower file integrity maintained

## Test Implementation Status

- ✅ Basic unit tests (switch_test.rs)
- ✅ UX-focused tests (switch_ux_test.rs)
- ✅ Mock SSH implementation
- ✅ Command generation tests
- ✅ Error handling tests
- 🔄 Integration tests (integration_test.rs)
- 📝 Manual testing checklist

## Manual Testing Checklist

Before release, manually verify:

1. **Real SSH Connections**
   - [ ] Test with actual validator nodes
   - [ ] Test with high-latency connections
   - [ ] Test with packet loss

2. **Different Validator Versions**
   - [ ] Latest Agave version
   - [ ] Latest Firedancer version
   - [ ] Mixed versions between nodes

3. **Production-like Scenarios**
   - [ ] 50GB+ ledger directories
   - [ ] Validators with 1M+ slots
   - [ ] During high network activity

4. **Terminal Compatibility**
   - [ ] Different terminal emulators
   - [ ] SSH sessions
   - [ ] Screen/tmux sessions