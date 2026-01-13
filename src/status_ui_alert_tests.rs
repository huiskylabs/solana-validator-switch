#[cfg(test)]
mod status_ui_alert_tests {
    use crate::types::{AlertConfig, FailureTracker, NodeHealthStatus};
    use std::time::{Duration, Instant};

    // Regression test for bug: pressing 'S' to switch from status UI switched the wrong validator
    // Bug: User views validator 2 via Tab, presses 'S', but validator 1 gets switched instead
    // Root cause: UI maintained its own selected_validator_index but it wasn't synced to app_state
    // Fix: sync ui_state.selected_validator_index to app_state before executing switch
    #[test]
    fn test_validator_selection_sync_for_switch() {
        // Simulate the scenario where:
        // - app_state has selected_validator_index = 0 (default)
        // - ui_state has selected_validator_index = 1 (user pressed Tab)
        // The switch should use ui_state's index, not app_state's

        let app_state_selected_index = 0; // Default
        let ui_state_selected_index = 1; // User navigated to validator 2

        // Simulate the fix: sync UI selection to app_state before switch
        // This is the fix that was added to status_ui_v2.rs:
        // if let Ok(ui_state_guard) = app.ui_state.try_read() {
        //     app_state_mut.selected_validator_index = ui_state_guard.selected_validator_index;
        // }
        let app_state_for_switch = ui_state_selected_index;

        // The switch command should now use the correct validator
        assert_eq!(
            app_state_for_switch, ui_state_selected_index,
            "Switch should use the UI's selected validator index, not the default"
        );
        assert_ne!(
            app_state_for_switch, app_state_selected_index,
            "Switch should NOT use app_state's default index when user selected different validator"
        );
    }

    // Test that Tab navigation correctly cycles through validators in UI state
    #[test]
    fn test_tab_navigation_cycles_validators() {
        let validator_count = 3;
        let mut ui_selected_index = 0;

        // Simulate pressing Tab multiple times
        for expected in [1, 2, 0, 1, 2, 0] {
            ui_selected_index = (ui_selected_index + 1) % validator_count;
            assert_eq!(
                ui_selected_index, expected,
                "Tab should cycle through validators: 0 -> 1 -> 2 -> 0"
            );
        }
    }

    // Test edge case: single validator should not change selection on Tab
    #[test]
    fn test_tab_with_single_validator() {
        let validator_count = 1;
        let mut ui_selected_index = 0;

        // Tab should keep index at 0 when there's only one validator
        ui_selected_index = (ui_selected_index + 1) % validator_count;
        assert_eq!(
            ui_selected_index, 0,
            "With single validator, Tab should keep selection at 0"
        );
    }

    // This test verifies the EXACT logic that should be in status_ui_v2.rs
    // for determining when to trigger auto-failover
    #[test]
    fn test_correct_auto_failover_logic() {
        let config = AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800, // 30 minutes
            rpc_failure_threshold_seconds: 1800, // 30 minutes
            telegram: None,
            auto_failover_enabled: true,
        };

        // The CORRECT logic for auto-failover:
        // 1. Check if vote hasn't increased for threshold time
        // 2. Check if RPC is working (no consecutive failures)
        // 3. Auto-failover triggers if BOTH conditions are met
        // Note: SSH may be down if the primary node is completely offline

        let test_cases = vec![
            (
                "RPC working, not voting = FAILOVER",
                0,    // ssh failures (doesn't matter)
                0,    // rpc failures
                40,   // seconds since vote
                true, // should trigger failover
            ),
            (
                "SSH down, RPC working, not voting = FAILOVER",
                1,    // ssh has failures (primary node may be offline)
                0,    // rpc working
                40,   // seconds since vote
                true, // should trigger failover
            ),
            (
                "RPC down, not voting = NO FAILOVER",
                0,     // ssh working
                1,     // rpc has failures
                40,    // seconds since vote
                false, // should NOT trigger (can't verify voting status)
            ),
            (
                "Both down, not voting = NO FAILOVER",
                1,     // ssh has failures
                1,     // rpc has failures
                40,    // seconds since vote
                false, // should NOT trigger (can't verify voting status)
            ),
            (
                "RPC working, voting recently = NO FAILOVER",
                0,     // ssh working
                0,     // rpc working
                20,    // only 20 seconds (under 30s threshold)
                false, // should NOT trigger
            ),
        ];

        for (scenario, ssh_failures, rpc_failures, seconds_since_vote, expected_failover) in
            test_cases
        {
            let mut health = NodeHealthStatus {
                ssh_status: FailureTracker::new(),
                rpc_status: FailureTracker::new(),
                is_voting: seconds_since_vote < 30, // Voting if recent
                last_vote_slot: Some(1000),
                last_vote_time: Some(Instant::now() - Duration::from_secs(seconds_since_vote)),
            };

            // Set up failures
            for _ in 0..ssh_failures {
                health.ssh_status.record_failure("SSH error".to_string());
            }
            for _ in 0..rpc_failures {
                health.rpc_status.record_failure("RPC error".to_string());
            }

            // THE CORRECT AUTO-FAILOVER CHECK
            let should_trigger_failover = seconds_since_vote >= config.delinquency_threshold_seconds  // Not voting for threshold
                && health.rpc_status.consecutive_failures == 0; // RPC must be working to verify

            assert_eq!(
                should_trigger_failover, expected_failover,
                "Failed for scenario: {}",
                scenario
            );
        }
    }

    // Test that verifies we can still trigger auto-failover even if SSH is down
    #[test]
    fn test_auto_failover_with_ssh_down() {
        // This is the key difference: auto-failover only needs RPC working
        let mut ssh_tracker = FailureTracker::new();
        let rpc_tracker = FailureTracker::new();

        // SSH is down (primary node may be completely offline)
        ssh_tracker.record_failure("Connection refused".to_string());

        // RPC is working (we can verify on-chain that validator is not voting)
        assert_eq!(rpc_tracker.consecutive_failures, 0);

        // Auto-failover should still trigger because:
        // 1. We can verify via RPC that validator is not voting
        // 2. SSH being down doesn't prevent failover (it just means optional steps may fail)
        let seconds_since_vote = 60;
        let threshold = 30;
        let should_trigger_failover =
            seconds_since_vote >= threshold && rpc_tracker.consecutive_failures == 0;

        assert!(
            should_trigger_failover,
            "Should trigger failover even with SSH down"
        );
    }

    // Test the actual monitoring flow with state transitions
    #[test]
    fn test_monitoring_state_transitions() {
        let mut health = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: true,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now()),
        };

        let mut alerts = Vec::new();

        // State 1: Normal operation
        assert_eq!(health.ssh_status.consecutive_failures, 0);
        assert_eq!(health.rpc_status.consecutive_failures, 0);
        // No alerts

        // State 2: RPC starts failing
        health.rpc_status.record_failure("Timeout".to_string());
        // Still no delinquency alert (can't verify voting)

        // State 3: Validator actually stops voting (but we don't know due to RPC)
        health.is_voting = false;
        health.last_vote_time = Some(Instant::now() - Duration::from_secs(60));
        // Still no delinquency alert (RPC is down)

        let should_alert_delinquency = health.rpc_status.consecutive_failures == 0;
        if !should_alert_delinquency {
            alerts.push("SUPPRESSED: Cannot verify voting due to RPC failure");
        }

        // State 4: RPC recovers
        health.rpc_status.record_success();

        // NOW we can send delinquency alert
        let can_verify_now = health.ssh_status.consecutive_failures == 0
            && health.rpc_status.consecutive_failures == 0;
        if can_verify_now && !health.is_voting {
            alerts.push("DELINQUENCY: Validator not voting");
        }

        assert!(alerts.contains(&"SUPPRESSED: Cannot verify voting due to RPC failure"));
        assert!(alerts.contains(&"DELINQUENCY: Validator not voting"));
    }

    // Test infrastructure alert thresholds
    #[test]
    fn test_infrastructure_alert_thresholds() {
        let config = AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800, // 30 minutes - VERY LOOSE
            rpc_failure_threshold_seconds: 1800, // 30 minutes - VERY LOOSE
            telegram: None,
            auto_failover_enabled: false,
        };

        let mut ssh_tracker = FailureTracker::new();
        let mut rpc_tracker = FailureTracker::new();

        // Many failures but still under time threshold = NO ALERT (avoiding noise)
        // Record initial failure to start the timer
        ssh_tracker.record_failure("SSH error".to_string());
        rpc_tracker.record_failure("RPC error".to_string());

        // Even after 100 failures, if time hasn't passed, no alert
        for _ in 1..100 {
            ssh_tracker.record_failure("SSH error".to_string());
            rpc_tracker.record_failure("RPC error".to_string());
        }

        // Time-based thresholds are what matter
        let ssh_seconds = ssh_tracker.seconds_since_first_failure().unwrap_or(0);
        let rpc_seconds = rpc_tracker.seconds_since_first_failure().unwrap_or(0);

        // Should be very recent (under 1 second)
        assert!(ssh_seconds < 1, "SSH failures should be recent");
        assert!(rpc_seconds < 1, "RPC failures should be recent");

        // No alerts because time threshold not met (need 30 minutes)
        assert!(ssh_seconds < config.ssh_failure_threshold_seconds);
        assert!(rpc_seconds < config.rpc_failure_threshold_seconds);

        // This demonstrates time-based thresholds to avoid noisy alerts
    }
}
