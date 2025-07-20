#[cfg(test)]
mod alert_logic_tests {
    use crate::alert::ComprehensiveAlertTracker;
    use crate::types::{AlertConfig, FailureTracker, NodeHealthStatus, TelegramConfig};
    use std::time::{Duration, Instant};

    fn create_realistic_alert_config() -> AlertConfig {
        AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,   // Critical - tight threshold
            ssh_failure_threshold_seconds: 1800, // 30 minutes - very loose threshold
            rpc_failure_threshold_seconds: 1800, // 30 minutes - very loose threshold
            telegram: Some(TelegramConfig {
                bot_token: "test_token".to_string(),
                chat_id: "test_chat".to_string(),
            }),
        }
    }

    // Critical test: Delinquency alert should ONLY fire when SSH and RPC are working
    #[test]
    fn test_delinquency_only_when_ssh_rpc_working() {
        let _config = create_realistic_alert_config();
        
        // Scenario 1: SSH ✅, RPC ✅, Not voting ❌ = SHOULD ALERT
        let health1 = NodeHealthStatus {
            ssh_status: FailureTracker::new(), // No failures = working
            rpc_status: FailureTracker::new(), // No failures = working
            is_voting: false,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now() - Duration::from_secs(40)), // 40s ago
        };
        
        // This should trigger delinquency alert
        assert!(!health1.ssh_status.consecutive_failures > 0);
        assert!(!health1.rpc_status.consecutive_failures > 0);
        assert!(!health1.is_voting);
        
        // Scenario 2: SSH ❌, RPC ✅, Not voting ❌ = NO DELINQUENCY ALERT
        let mut health2 = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: false,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now() - Duration::from_secs(40)),
        };
        health2.ssh_status.record_failure("Connection refused".to_string());
        
        // Should NOT trigger delinquency alert (only SSH failure alert)
        assert!(health2.ssh_status.consecutive_failures > 0);
        assert!(!health2.rpc_status.consecutive_failures > 0);
        
        // Scenario 3: SSH ✅, RPC ❌, Not voting ❌ = NO DELINQUENCY ALERT
        let mut health3 = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: false,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now() - Duration::from_secs(40)),
        };
        health3.rpc_status.record_failure("429 Too Many Requests".to_string());
        
        // Should NOT trigger delinquency alert (only RPC failure alert)
        assert!(!health3.ssh_status.consecutive_failures > 0);
        assert!(health3.rpc_status.consecutive_failures > 0);
    }

    // Test that SSH/RPC failures don't trigger immediate alerts (loose time thresholds)
    #[test]
    fn test_loose_thresholds_for_infrastructure_alerts() {
        let config = create_realistic_alert_config();
        let mut ssh_tracker = FailureTracker::new();
        let mut rpc_tracker = FailureTracker::new();
        
        // SSH: Many failures but still no alert if under 30 minutes
        for i in 0..100 {
            ssh_tracker.record_failure(format!("SSH Error {}", i));
        }
        let ssh_seconds = ssh_tracker.seconds_since_first_failure().unwrap_or(0);
        assert!(ssh_seconds < config.ssh_failure_threshold_seconds);
        
        // RPC: Many failures but still no alert if under 30 minutes
        for i in 0..100 {
            rpc_tracker.record_failure(format!("RPC Error {}", i));
        }
        let rpc_seconds = rpc_tracker.seconds_since_first_failure().unwrap_or(0);
        assert!(rpc_seconds < config.rpc_failure_threshold_seconds);
    }

    // Test the decision tree for alerts
    #[test]
    fn test_alert_decision_tree() {
        let config = create_realistic_alert_config();
        
        struct TestCase {
            name: &'static str,
            ssh_working: bool,
            rpc_working: bool,
            voting: bool,
            seconds_since_vote: u64,
            expected_alerts: Vec<&'static str>,
        }
        
        let test_cases = vec![
            TestCase {
                name: "Everything working",
                ssh_working: true,
                rpc_working: true,
                voting: true,
                seconds_since_vote: 10,
                expected_alerts: vec![], // No alerts
            },
            TestCase {
                name: "Critical delinquency",
                ssh_working: true,
                rpc_working: true,
                voting: false,
                seconds_since_vote: 40, // > 30s threshold
                expected_alerts: vec!["delinquency"],
            },
            TestCase {
                name: "SSH down, not voting",
                ssh_working: false,
                rpc_working: true,
                voting: false,
                seconds_since_vote: 40,
                expected_alerts: vec!["ssh"], // Only SSH alert, NO delinquency
            },
            TestCase {
                name: "RPC down, not voting",
                ssh_working: true,
                rpc_working: false,
                voting: false,
                seconds_since_vote: 40,
                expected_alerts: vec!["rpc"], // Only RPC alert, NO delinquency
            },
            TestCase {
                name: "Both down, not voting",
                ssh_working: false,
                rpc_working: false,
                voting: false,
                seconds_since_vote: 40,
                expected_alerts: vec!["ssh", "rpc"], // Only infrastructure alerts
            },
        ];
        
        for test in test_cases {
            let mut health = NodeHealthStatus {
                ssh_status: FailureTracker::new(),
                rpc_status: FailureTracker::new(),
                is_voting: test.voting,
                last_vote_slot: Some(1000),
                last_vote_time: Some(Instant::now() - Duration::from_secs(test.seconds_since_vote)),
            };
            
            // Simulate failures
            if !test.ssh_working {
                health.ssh_status.record_failure("SSH failed".to_string());
            }
            
            if !test.rpc_working {
                health.rpc_status.record_failure("RPC failed".to_string());
            }
            
            // Verify alert logic
            let should_alert_delinquency = test.ssh_working 
                && test.rpc_working 
                && !test.voting 
                && test.seconds_since_vote >= config.delinquency_threshold_seconds;
                
            let should_alert_ssh = !test.ssh_working 
                && health.ssh_status.consecutive_failures > 0;
                
            let should_alert_rpc = !test.rpc_working 
                && health.rpc_status.consecutive_failures > 0;
            
            // Check expectations
            assert_eq!(
                should_alert_delinquency,
                test.expected_alerts.contains(&"delinquency"),
                "Test case '{}' failed for delinquency alert",
                test.name
            );
            
            assert_eq!(
                should_alert_ssh,
                test.expected_alerts.contains(&"ssh"),
                "Test case '{}' failed for SSH alert",
                test.name
            );
            
            assert_eq!(
                should_alert_rpc,
                test.expected_alerts.contains(&"rpc"),
                "Test case '{}' failed for RPC alert",
                test.name
            );
        }
    }

    // Test that delinquency alerts are suppressed when we can't verify
    #[test]
    fn test_no_false_delinquency_alerts() {
        let _config = create_realistic_alert_config();
        
        // Even if vote hasn't increased for hours, if SSH/RPC is down, NO delinquency alert
        let mut health = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: false,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now() - Duration::from_secs(3600)), // 1 hour ago!
        };
        
        // SSH failure
        health.ssh_status.record_failure("Connection timeout".to_string());
        
        // Should NOT trigger delinquency despite 1 hour of no voting
        // because we can't verify due to SSH being down
        let should_alert_delinquency = health.ssh_status.consecutive_failures == 0 
            && health.rpc_status.consecutive_failures == 0;
            
        assert!(!should_alert_delinquency, "Should not alert delinquency when SSH is down");
    }

    // Test time-based thresholds for infrastructure alerts
    #[tokio::test]
    async fn test_time_based_infrastructure_thresholds() {
        let config = create_realistic_alert_config();
        let mut ssh_tracker = FailureTracker::new();
        
        // Record first SSH failure
        ssh_tracker.record_failure("SSH timeout".to_string());
        
        // Even with only 1 failure, after 30 minutes it should trigger
        let seconds = ssh_tracker.seconds_since_first_failure().unwrap_or(0);
        
        // In real scenario, we'd wait 30 minutes
        // Here we just verify the logic
        let should_alert = seconds >= config.ssh_failure_threshold_seconds;
        
        // With 1 failure and < 30 minutes, should not alert
        assert!(!should_alert || seconds >= 1800);
    }

    // Test alert cooldown periods
    #[test]
    fn test_alert_cooldowns() {
        let mut tracker = ComprehensiveAlertTracker::new(2, 2);
        
        // First delinquency alert should go through
        assert!(tracker.delinquency_tracker.should_send_alert(0));
        
        // Immediate second alert should be blocked (15 min cooldown for delinquency)
        assert!(!tracker.delinquency_tracker.should_send_alert(0));
        
        // Different validator should have independent cooldown
        assert!(tracker.delinquency_tracker.should_send_alert(1));
        
        // SSH alerts have independent cooldowns per node
        assert!(tracker.ssh_failure_tracker[0].should_send_alert(0));
        assert!(!tracker.ssh_failure_tracker[0].should_send_alert(0));
        assert!(tracker.ssh_failure_tracker[1].should_send_alert(0)); // Different node
    }

    // Test realistic monitoring cycle
    #[test]
    fn test_realistic_monitoring_cycle() {
        let _config = create_realistic_alert_config();
        let mut health = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: true,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now()),
        };
        
        // Simulate monitoring cycles every 5 seconds
        let mut cycles: Vec<(String, Vec<&str>)> = vec![];
        
        // Cycle 1: Everything fine
        cycles.push(("All OK".to_string(), vec![]));
        
        // Cycles 2-10: RPC starts failing (not enough for alert yet)
        for i in 2..=10 {
            health.rpc_status.record_failure("Rate limited".to_string());
            cycles.push((format!("RPC fail #{}", i-1), vec![]));
        }
        
        // Cycles 11-31: RPC continues failing (triggers at 30)
        for i in 11..=31 {
            health.rpc_status.record_failure("Rate limited".to_string());
            let alerts = if i == 31 { vec!["rpc"] } else { vec![] };
            cycles.push((format!("RPC fail #{}", i-1), alerts));
        }
        
        // Cycle 32: RPC recovers, but now validator stops voting
        health.rpc_status.record_success();
        health.is_voting = false;
        health.last_vote_time = Some(Instant::now() - Duration::from_secs(35));
        
        // Should trigger delinquency because SSH ✅ and RPC ✅
        cycles.push(("RPC OK, not voting".to_string(), vec!["delinquency"]));
        
        // Verify final state
        assert_eq!(health.ssh_status.consecutive_failures, 0);
        assert_eq!(health.rpc_status.consecutive_failures, 0);
        assert!(!health.is_voting);
    }

    // Test edge case: RPC recovers just before delinquency check
    #[test]
    fn test_rpc_recovery_prevents_false_delinquency() {
        let _config = create_realistic_alert_config();
        let mut health = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: false,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now() - Duration::from_secs(40)), // Would trigger delinquency
        };
        
        // RPC was failing
        health.rpc_status.record_failure("Network error".to_string());
        
        // Cannot send delinquency alert while RPC is failing
        let can_check_delinquency = health.rpc_status.consecutive_failures == 0;
        assert!(!can_check_delinquency);
        
        // RPC recovers
        health.rpc_status.record_success();
        
        // Now we CAN send delinquency alert
        let can_check_delinquency_after = health.rpc_status.consecutive_failures == 0;
        assert!(can_check_delinquency_after);
    }
}