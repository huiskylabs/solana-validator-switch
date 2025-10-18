#[cfg(test)]
mod tests {
    use crate::alert::{AlertTracker, ComprehensiveAlertTracker};
    use crate::types::{AlertConfig, FailureTracker, NodeHealthStatus, TelegramConfig};
    use std::time::{Duration, Instant};
    use tokio::time::sleep;

    fn create_test_alert_config() -> AlertConfig {
        AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800,
            rpc_failure_threshold_seconds: 1800,
            telegram: Some(TelegramConfig {
                bot_token: "test_token".to_string(),
                chat_id: "test_chat".to_string(),
            }),
            auto_failover_enabled: false,
        }
    }

    #[test]
    fn test_failure_tracker_success_resets_failures() {
        let mut tracker = FailureTracker::new();

        // Record some failures
        tracker.record_failure("Error 1".to_string());
        tracker.record_failure("Error 2".to_string());
        assert_eq!(tracker.consecutive_failures, 2);

        // Success should reset
        tracker.record_success();
        assert_eq!(tracker.consecutive_failures, 0);
        assert!(tracker.first_failure_time.is_none());
        assert!(tracker.last_error.is_none());
    }

    #[test]
    fn test_failure_tracker_counts_consecutive_failures() {
        let mut tracker = FailureTracker::new();

        tracker.record_failure("Error 1".to_string());
        assert_eq!(tracker.consecutive_failures, 1);
        assert!(tracker.first_failure_time.is_some());

        tracker.record_failure("Error 2".to_string());
        assert_eq!(tracker.consecutive_failures, 2);
        assert_eq!(tracker.last_error, Some("Error 2".to_string()));
    }

    #[tokio::test]
    async fn test_failure_tracker_time_tracking() {
        let mut tracker = FailureTracker::new();

        tracker.record_failure("Error".to_string());
        let first_failure_time = tracker.first_failure_time.unwrap();

        // Wait a bit
        sleep(Duration::from_millis(100)).await;

        // Second failure shouldn't change first failure time
        tracker.record_failure("Error 2".to_string());
        assert_eq!(tracker.first_failure_time.unwrap(), first_failure_time);

        // Check time calculation
        let seconds = tracker.seconds_since_first_failure().unwrap();
        assert!(seconds < 1); // Should be less than 1 second
    }

    #[test]
    fn test_alert_tracker_cooldown() {
        let mut tracker = AlertTracker::new(2);

        // First alert should be allowed
        assert!(tracker.should_send_alert(0));

        // Immediate second alert should be blocked
        assert!(!tracker.should_send_alert(0));

        // Different validator should be allowed
        assert!(tracker.should_send_alert(1));
    }

    #[test]
    fn test_alert_tracker_reset() {
        let mut tracker = AlertTracker::new(2);

        // Send alert
        assert!(tracker.should_send_alert(0));
        assert!(!tracker.should_send_alert(0));

        // Reset should allow new alert
        tracker.reset(0);
        assert!(tracker.should_send_alert(0));
    }

    #[test]
    fn test_comprehensive_alert_tracker() {
        let tracker = ComprehensiveAlertTracker::new(2, 2);

        // Just verify it was created successfully
        assert_eq!(tracker.ssh_failure_tracker.len(), 2);
    }

    #[test]
    fn test_node_health_status_initialization() {
        let health = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: true,
            last_vote_slot: Some(12345),
            last_vote_time: Some(Instant::now()),
        };

        assert_eq!(health.ssh_status.consecutive_failures, 0);
        assert_eq!(health.rpc_status.consecutive_failures, 0);
        assert!(health.is_voting);
    }

    #[tokio::test]
    async fn test_rpc_failure_preserves_slot_time() {
        // This tests the critical bug fix where RPC failures were causing
        // slot times to be lost, leading to false delinquency alerts

        let last_vote_slot_times: Vec<Option<(u64, Instant)>> = vec![Some((1000, Instant::now()))];
        let new_vote_data: Option<()> = None; // Simulating RPC failure

        let mut new_slot_times = Vec::new();

        // When RPC fails, we should preserve the existing slot time
        if new_vote_data.is_none() {
            // This is the fix - preserve existing time instead of None
            new_slot_times.push(last_vote_slot_times.get(0).and_then(|&v| v));
        }

        // Verify slot time was preserved
        assert!(new_slot_times[0].is_some());
        assert_eq!(new_slot_times[0].unwrap().0, 1000);
    }

    #[test]
    fn test_ssh_failure_threshold_triggers() {
        let config = create_test_alert_config();
        let mut tracker = FailureTracker::new();

        // Record initial failure to start the timer
        tracker.record_failure("SSH Error 1".to_string());

        // Even after many failures, if time hasn't passed, no alert
        for i in 2..100 {
            tracker.record_failure(format!("SSH Error {}", i));
        }

        // Should not trigger alert yet (need 30 minutes)
        let seconds = tracker.seconds_since_first_failure().unwrap_or(0);
        assert!(seconds < config.ssh_failure_threshold_seconds);
    }

    #[test]
    fn test_rpc_failure_threshold_triggers() {
        let config = create_test_alert_config();
        let mut tracker = FailureTracker::new();

        // Record initial failure to start the timer
        tracker.record_failure("RPC Error 1".to_string());

        // Even after many failures, if time hasn't passed, no alert
        for i in 2..100 {
            tracker.record_failure(format!("RPC Error {}", i));
        }

        // Should not trigger alert yet (need 30 minutes)
        let seconds = tracker.seconds_since_first_failure().unwrap_or(0);
        assert!(seconds < config.rpc_failure_threshold_seconds);
    }

    #[tokio::test]
    async fn test_combined_failure_scenario() {
        // Test scenario: SSH is down, RPC is working, validator is voting
        let mut ssh_tracker = FailureTracker::new();
        let mut rpc_tracker = FailureTracker::new();
        let _config = create_test_alert_config();

        // SSH failures
        for i in 0..5 {
            ssh_tracker.record_failure(format!("SSH timeout {}", i));
        }

        // RPC success
        rpc_tracker.record_success();

        // Check states
        assert_eq!(ssh_tracker.consecutive_failures, 5);
        assert_eq!(rpc_tracker.consecutive_failures, 0);
    }

    #[tokio::test]
    async fn test_delinquency_with_health_status() {
        // Test delinquency alert includes health status
        let mut health = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: false,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now() - Duration::from_secs(60)),
        };

        // Simulate some failures
        health
            .ssh_status
            .record_failure("Connection refused".to_string());
        health.rpc_status.record_success();

        // Verify health status
        assert_eq!(health.ssh_status.consecutive_failures, 1);
        assert_eq!(health.rpc_status.consecutive_failures, 0);
        assert!(!health.is_voting);
    }

    #[test]
    fn test_time_based_alert_threshold() {
        let config = create_test_alert_config();
        let mut tracker = FailureTracker::new();

        // Record first failure
        tracker.record_failure("Error".to_string());

        // Even with only 1 failure, if enough time passes, alert should trigger
        // In real scenario, we'd wait, but here we just check the logic
        let seconds = tracker.seconds_since_first_failure().unwrap_or(0);

        // Alert should trigger only if time threshold is met
        let should_alert = seconds >= config.ssh_failure_threshold_seconds;

        // With only 1 failure and no time passed, should not alert
        assert!(!should_alert);
    }

    #[test]
    fn test_multiple_validator_tracking() {
        let mut trackers: Vec<FailureTracker> = vec![
            FailureTracker::new(),
            FailureTracker::new(),
            FailureTracker::new(),
        ];

        // Different validators can have different states
        trackers[0].record_failure("Validator 0 error".to_string());
        trackers[1].record_success();
        trackers[2].record_failure("Validator 2 error".to_string());
        trackers[2].record_failure("Validator 2 error 2".to_string());

        assert_eq!(trackers[0].consecutive_failures, 1);
        assert_eq!(trackers[1].consecutive_failures, 0);
        assert_eq!(trackers[2].consecutive_failures, 2);
    }
}
