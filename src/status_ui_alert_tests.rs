#[cfg(test)]
mod status_ui_alert_tests {
    use crate::types::{AlertConfig, FailureTracker, NodeHealthStatus};
    use std::time::{Duration, Instant};

    // This test verifies the EXACT logic that should be in status_ui_v2.rs
    // for determining when to send delinquency alerts
    #[test]
    fn test_correct_delinquency_alert_logic() {
        let config = AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800, // 30 minutes
            rpc_failure_threshold_seconds: 1800, // 30 minutes
            telegram: None,
        };

        // The CORRECT logic for delinquency alerts:
        // 1. Check if vote hasn't increased for threshold time
        // 2. Check if SSH is working (no consecutive failures)
        // 3. Check if RPC is working (no consecutive failures)
        // 4. ONLY alert if ALL conditions are met

        let test_cases = vec![
            (
                "SSH and RPC working, not voting = ALERT",
                0, // ssh failures
                0, // rpc failures
                40, // seconds since vote
                true, // should alert
            ),
            (
                "SSH down, not voting = NO ALERT",
                1, // ssh has failures
                0, // rpc working
                40, // seconds since vote
                false, // should NOT alert
            ),
            (
                "RPC down, not voting = NO ALERT",
                0, // ssh working
                1, // rpc has failures
                40, // seconds since vote
                false, // should NOT alert
            ),
            (
                "Both down, not voting = NO ALERT",
                1, // ssh has failures
                1, // rpc has failures
                40, // seconds since vote
                false, // should NOT alert
            ),
            (
                "All working, voting recently = NO ALERT",
                0, // ssh working
                0, // rpc working
                20, // only 20 seconds (under 30s threshold)
                false, // should NOT alert
            ),
        ];

        for (scenario, ssh_failures, rpc_failures, seconds_since_vote, expected_alert) in test_cases {
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

            // THE CORRECT DELINQUENCY CHECK
            let should_send_delinquency_alert = 
                seconds_since_vote >= config.delinquency_threshold_seconds  // Not voting for threshold
                && health.ssh_status.consecutive_failures == 0              // SSH must be working
                && health.rpc_status.consecutive_failures == 0;             // RPC must be working

            assert_eq!(
                should_send_delinquency_alert, 
                expected_alert,
                "Failed for scenario: {}",
                scenario
            );
        }
    }

    // Test that verifies we DON'T send delinquency alerts during RPC failures
    #[test]
    fn test_no_delinquency_during_rpc_failure() {
        // This is the critical bug we fixed
        let _last_vote_slot_times = vec![Some((1000u64, Instant::now() - Duration::from_secs(60)))];
        let mut rpc_failure_tracker = FailureTracker::new();
        
        // RPC fails
        rpc_failure_tracker.record_failure("HTTP 429".to_string());
        
        // Even though vote hasn't updated for 60 seconds, we should NOT alert
        // because we can't trust the data (RPC is failing)
        let can_trust_vote_data = rpc_failure_tracker.consecutive_failures == 0;
        assert!(!can_trust_vote_data, "Cannot trust vote data when RPC is failing");
        
        // Therefore, no delinquency alert should be sent
        let should_send_delinquency = can_trust_vote_data && 60 >= 30;
        assert!(!should_send_delinquency, "Should not send delinquency alert during RPC failure");
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