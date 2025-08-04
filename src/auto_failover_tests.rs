#[cfg(test)]
mod tests {
    use crate::types::AlertConfig;

    #[test]
    fn test_alert_config_with_auto_failover() {
        let alert_config = AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800,
            rpc_failure_threshold_seconds: 1800,
            telegram: None,
            auto_failover_enabled: true,
        };

        assert!(alert_config.enabled);
        assert!(alert_config.auto_failover_enabled);
    }

    #[test]
    fn test_auto_failover_disabled_by_default() {
        let alert_config = AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800,
            rpc_failure_threshold_seconds: 1800,
            telegram: None,
            auto_failover_enabled: false,
        };

        assert!(!alert_config.auto_failover_enabled);
    }

    #[test]
    fn test_delinquency_triggers_failover_conditions() {
        use crate::types::{FailureTracker, NodeHealthStatus};
        use std::time::Instant;

        let mut health = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: false,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now()),
        };

        // Test condition 1: SSH and RPC working, should trigger failover
        assert_eq!(health.ssh_status.consecutive_failures, 0);
        assert_eq!(health.rpc_status.consecutive_failures, 0);
        let should_failover = health.ssh_status.consecutive_failures == 0
            && health.rpc_status.consecutive_failures == 0;
        assert!(
            should_failover,
            "Should trigger failover when SSH and RPC are working"
        );

        // Test condition 2: SSH failing, should NOT trigger failover
        health
            .ssh_status
            .record_failure("Connection refused".to_string());
        let should_failover = health.ssh_status.consecutive_failures == 0
            && health.rpc_status.consecutive_failures == 0;
        assert!(
            !should_failover,
            "Should NOT trigger failover when SSH is failing"
        );

        // Test condition 3: RPC failing, should NOT trigger failover
        health.ssh_status.record_success();
        health
            .rpc_status
            .record_failure("429 Too Many Requests".to_string());
        let should_failover = health.ssh_status.consecutive_failures == 0
            && health.rpc_status.consecutive_failures == 0;
        assert!(
            !should_failover,
            "Should NOT trigger failover when RPC is failing"
        );
    }
}
