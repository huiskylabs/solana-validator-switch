# Validator Switch Testing Guide

## Overview

The validator switch functionality has comprehensive test coverage to ensure robust error handling and excellent user experience across all scenarios.

## Test Structure

### 1. Unit Tests (`src/commands/switch_test.rs`)
- **Mock SSH Pool**: Simulates SSH connections without real network calls
- **Command Generation**: Tests correct command formation for different validator types
- **Error Handling**: Verifies proper error propagation and messages
- **Timing Measurements**: Ensures accurate timing tracking

### 2. UX Tests (`src/commands/switch_ux_test.rs`)
- **User Interactions**: Confirmation dialogs, cancellations
- **Progress Messages**: Validates clear progress indicators
- **Error Messages**: Ensures actionable error messages
- **Edge Cases**: Handles unusual inputs and states

### 3. Integration Tests (`tests/integration_test.rs`)
- **Full Workflow**: End-to-end switch scenarios
- **Mixed Validators**: Agave to Firedancer switches
- **Failure Recovery**: Partial switch handling
- **Performance**: Timing benchmarks

## Running Tests

```bash
# Run all tests
cargo test

# Run specific test module
cargo test switch_test

# Run with output for debugging
cargo test -- --nocapture

# Run only UX tests
cargo test ux_tests
```

## Key Test Scenarios

### Happy Path
✅ Agave to Agave switch
✅ Firedancer to Firedancer switch
✅ Mixed validator types
✅ Tower file transfer
✅ Identity switch timing

### Edge Cases
✅ Missing tower files
✅ Missing executables
✅ Network timeouts
✅ Permission errors
✅ Config auto-detection

### Failure Scenarios
✅ SSH connection failures
✅ Partial switch states
✅ Command timeouts
✅ Invalid configurations

## Manual Testing Checklist

Before release, manually test:

1. **Real Validators**
   - [ ] Production validator pair
   - [ ] Different geographic locations
   - [ ] High-latency connections

2. **Validator Versions**
   - [ ] Latest Agave release
   - [ ] Latest Firedancer release
   - [ ] Mixed version scenarios

3. **Load Conditions**
   - [ ] Large ledger directories (50GB+)
   - [ ] During high network activity
   - [ ] Concurrent operations

4. **Terminal Compatibility**
   - [ ] Different terminal emulators
   - [ ] SSH sessions
   - [ ] Screen/tmux environments

## Test Coverage Goals

- **Unit Test Coverage**: >80%
- **Integration Coverage**: All critical paths
- **UX Coverage**: All user-facing messages
- **Error Coverage**: All known failure modes

## Adding New Tests

When adding new features:

1. Add unit tests for core logic
2. Add UX tests for user interactions
3. Update integration tests for workflows
4. Document manual test scenarios
5. Update this README

## Mock Infrastructure

### MockSshPool
Simulates SSH connections with:
- Configurable responses
- Command history tracking
- Failure injection
- Timing simulation

### Test Fixtures
- Example node configurations
- Sample tower files
- Mock validator processes
- Error scenarios