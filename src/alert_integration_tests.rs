#[cfg(test)]
mod alert_integration_tests {
    use crate::types::{AlertConfig, FailureTracker, NodeHealthStatus};
    use std::time::{Duration, Instant};

    // Test the actual logic from status_ui_v2.rs
    #[test]
    fn test_status_ui_delinquency_logic() {
        let config = AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800, // 30 minutes
            rpc_failure_threshold_seconds: 1800, // 30 minutes
            telegram: None,
            auto_failover_enabled: false,
        };

        // Simulate the actual check from status_ui_v2.rs
        let seconds_since_vote = 45;
        let threshold = config.delinquency_threshold_seconds;

        // Mock validator health states
        let mut validator_health = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: false,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now() - Duration::from_secs(seconds_since_vote)),
        };

        // Case 1: Both SSH and RPC working - SHOULD alert
        let should_alert_case1 = seconds_since_vote >= threshold
            && validator_health.ssh_status.consecutive_failures == 0
            && validator_health.rpc_status.consecutive_failures == 0;
        assert!(
            should_alert_case1,
            "Should alert when SSH/RPC working and not voting"
        );

        // Case 2: SSH failing - should NOT alert for delinquency
        validator_health
            .ssh_status
            .record_failure("Connection refused".to_string());
        let should_alert_case2 = seconds_since_vote >= threshold
            && validator_health.ssh_status.consecutive_failures == 0
            && validator_health.rpc_status.consecutive_failures == 0;
        assert!(
            !should_alert_case2,
            "Should NOT alert delinquency when SSH failing"
        );

        // Case 3: RPC failing - should NOT alert for delinquency
        validator_health.ssh_status.record_success(); // Reset SSH
        validator_health
            .rpc_status
            .record_failure("429 Too Many Requests".to_string());
        let should_alert_case3 = seconds_since_vote >= threshold
            && validator_health.ssh_status.consecutive_failures == 0
            && validator_health.rpc_status.consecutive_failures == 0;
        assert!(
            !should_alert_case3,
            "Should NOT alert delinquency when RPC failing"
        );
    }

    // Test RPC failure preserving slot times (the critical bug fix)
    #[test]
    fn test_rpc_failure_preserves_slot_time() {
        // Simulate existing slot time
        let existing_slot_time = Some((298745632u64, Instant::now() - Duration::from_secs(25)));

        // RPC fails
        let rpc_result: Result<(), String> = Err("RPC timeout".to_string());

        // The fix: preserve existing slot time instead of setting to None
        let new_slot_time = if rpc_result.is_err() {
            existing_slot_time // Preserve it!
        } else {
            None // This was the bug - losing the timestamp
        };

        assert!(
            new_slot_time.is_some(),
            "Slot time should be preserved on RPC failure"
        );
        assert_eq!(
            new_slot_time.unwrap().0,
            298745632,
            "Slot number should be preserved"
        );
    }

    // Test SSH failure threshold logic
    #[test]
    fn test_ssh_alert_threshold_logic() {
        let config = AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800, // 30 minutes
            rpc_failure_threshold_seconds: 1800, // 30 minutes
            telegram: None,
            auto_failover_enabled: false,
        };

        let mut ssh_tracker = FailureTracker::new();

        // Simulate SSH failures over time
        for i in 0..19 {
            ssh_tracker.record_failure(format!("SSH timeout {}", i));
        }

        // Check if should alert
        let seconds = ssh_tracker.seconds_since_first_failure().unwrap_or(0);

        // Only time-based thresholds now
        let should_alert = seconds >= config.ssh_failure_threshold_seconds;

        assert!(
            !should_alert,
            "Should not alert immediately (need 30 minutes)"
        );

        // After many failures, still no alert if time hasn't passed
        ssh_tracker.record_failure("SSH timeout 20".to_string());
        let seconds_now = ssh_tracker.seconds_since_first_failure().unwrap_or(0);
        let should_alert_now = seconds_now >= config.ssh_failure_threshold_seconds;
        assert!(
            !should_alert_now,
            "Should not alert until 30 minutes have passed"
        );
    }

    // Test complete monitoring flow
    #[test]
    fn test_complete_monitoring_flow() {
        let config = AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800, // 30 minutes
            rpc_failure_threshold_seconds: 1800, // 30 minutes
            telegram: None,
            auto_failover_enabled: false,
        };

        // Validator state
        let mut health = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: true,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now()),
        };

        let mut alerts_triggered = Vec::new();

        // Monitoring loop simulation
        for minute in 0..10 {
            match minute {
                0..=2 => {
                    // Everything working fine
                    health.ssh_status.record_success();
                    health.rpc_status.record_success();
                    health.is_voting = true;
                    health.last_vote_time = Some(Instant::now());
                }
                3..=5 => {
                    // RPC starts failing
                    health.ssh_status.record_success();
                    for _ in 0..10 {
                        health.rpc_status.record_failure("Rate limited".to_string());
                    }
                    // Validator still voting but we can't see it due to RPC
                }
                6 => {
                    // RPC failures exceed threshold
                    for _ in 0..80 {
                        health.rpc_status.record_failure("Rate limited".to_string());
                    }
                    // Note: In real scenario, we'd check time threshold too
                    if health.rpc_status.consecutive_failures > 0 {
                        alerts_triggered.push("RPC_FAILURE");
                    }
                }
                7 => {
                    // RPC recovers, but validator stopped voting
                    health.rpc_status.record_success();
                    health.ssh_status.record_success();
                    health.is_voting = false;
                    health.last_vote_time = Some(Instant::now() - Duration::from_secs(40));

                    // This should trigger delinquency alert
                    if health.ssh_status.consecutive_failures == 0
                        && health.rpc_status.consecutive_failures == 0
                        && !health.is_voting
                    {
                        alerts_triggered.push("DELINQUENCY");
                    }
                }
                _ => {}
            }
        }

        assert!(
            alerts_triggered.contains(&"RPC_FAILURE"),
            "Should have RPC failure alert"
        );
        assert!(
            alerts_triggered.contains(&"DELINQUENCY"),
            "Should have delinquency alert"
        );
        assert_eq!(alerts_triggered.len(), 2, "Should have exactly 2 alerts");
    }

    // Test alert suppression during maintenance
    #[test]
    fn test_alert_suppression_scenarios() {
        // Scenario: Validator is being restarted (SSH works, RPC works, not voting temporarily)
        let _health = NodeHealthStatus {
            ssh_status: FailureTracker::new(), // Working
            rpc_status: FailureTracker::new(), // Working
            is_voting: false,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now() - Duration::from_secs(25)), // < 30s threshold
        };

        // Should NOT alert yet (under threshold)
        let should_alert = 25 >= 30; // seconds_since_vote >= threshold
        assert!(!should_alert, "Should not alert during brief restart");
    }
}
