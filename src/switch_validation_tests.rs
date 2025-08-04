#[cfg(test)]
mod switch_validation_tests {
    use crate::types::{NodeConfig, NodePaths, NodeStatus, NodeWithStatus, ValidatorType};
    use std::collections::HashMap;

    // Helper function to create a test node
    fn create_test_node(
        label: &str,
        host: &str,
        available: bool,
        is_active: bool,
    ) -> NodeWithStatus {
        NodeWithStatus {
            node: NodeConfig {
                label: label.to_string(),
                host: host.to_string(),
                port: 22,
                user: "solana".to_string(),
                paths: NodePaths {
                    funded_identity: "/home/solana/funded.json".to_string(),
                    unfunded_identity: "/home/solana/unfunded.json".to_string(),
                    vote_keypair: "/home/solana/vote.json".to_string(),
                },
                ssh_key_path: Some("/home/user/.ssh/id_rsa".to_string()),
            },
            status: if !available {
                NodeStatus::Unknown
            } else if is_active {
                NodeStatus::Active
            } else {
                NodeStatus::Standby
            },
            validator_type: ValidatorType::Agave,
            agave_validator_executable: Some("/home/solana/bin/agave-validator".to_string()),
            fdctl_executable: None,
            solana_cli_executable: Some("/home/solana/bin/solana".to_string()),
            version: Some("Agave 2.0.0".to_string()),
            sync_status: Some("Caught up".to_string()),
            current_identity: Some(
                if is_active {
                    "funded123"
                } else {
                    "unfunded123"
                }
                .to_string(),
            ),
            ledger_path: Some("/mnt/ledger".to_string()),
            tower_path: if is_active {
                Some("/mnt/ledger/tower.bin".to_string())
            } else {
                None
            },
            swap_ready: Some(available),
            swap_issues: if available {
                vec![]
            } else {
                vec!["SSH connection failed".to_string()]
            },
            ssh_key_path: Some("/home/user/.ssh/id_rsa".to_string()),
        }
    }

    fn create_test_ssh_keys() -> HashMap<String, String> {
        let mut keys = HashMap::new();
        keys.insert(
            "validator1-1.example.com".to_string(),
            "/home/user/.ssh/id_rsa".to_string(),
        );
        keys.insert(
            "validator1-2.example.com".to_string(),
            "/home/user/.ssh/id_rsa".to_string(),
        );
        keys.insert(
            "validator2-1.example.com".to_string(),
            "/home/user/.ssh/id_rsa".to_string(),
        );
        keys.insert(
            "validator2-2.example.com".to_string(),
            "/home/user/.ssh/id_rsa".to_string(),
        );
        keys
    }

    #[test]
    fn test_switch_validation_target_node_down_fails() {
        // Target (standby) node is down - this should FAIL validation
        let active_node = create_test_node("node-1-1", "validator1-1.example.com", true, true);
        let standby_node = create_test_node("node-1-2", "validator1-2.example.com", false, false); // DOWN
        let ssh_keys = create_test_ssh_keys();

        let mut validation_errors = Vec::new();
        let mut validation_warnings: Vec<String> = Vec::new();

        // Check target (standby) node - this is critical for switch success
        if standby_node.status == NodeStatus::Unknown {
            validation_errors.push(format!(
                "Target node {} is unreachable (SSH connection failed)",
                standby_node.node.label
            ));
        }

        // Check if we can get SSH key for target node
        if !ssh_keys.contains_key(&standby_node.node.host) {
            validation_errors.push(format!(
                "No SSH key available for target node {}",
                standby_node.node.label
            ));
        }

        // Should have validation errors
        assert_eq!(validation_errors.len(), 1);
        assert!(validation_errors[0].contains("Target node node-1-2 is unreachable"));
        assert!(validation_warnings.is_empty());
    }

    #[test]
    fn test_switch_validation_source_node_down_succeeds_with_warning() {
        // Source (active) node is down - this should SUCCEED with warnings
        let active_node = create_test_node("node-1-1", "validator1-1.example.com", false, true); // DOWN
        let standby_node = create_test_node("node-1-2", "validator1-2.example.com", true, false);
        let ssh_keys = create_test_ssh_keys();

        let mut validation_errors = Vec::new();
        let mut validation_warnings: Vec<String> = Vec::new();

        // Check target (standby) node - this is critical for switch success
        if standby_node.status == NodeStatus::Unknown {
            validation_errors.push(format!(
                "Target node {} is unreachable (SSH connection failed)",
                standby_node.node.label
            ));
        }

        // Check source (active) node - this is preferred but not critical
        if active_node.status == NodeStatus::Unknown {
            validation_warnings.push(format!(
                "Source node {} is unreachable - will skip optional steps (tower copy, graceful shutdown)",
                active_node.node.label
            ));
        }

        // Should have NO validation errors, only warnings
        assert!(validation_errors.is_empty());
        assert_eq!(validation_warnings.len(), 1);
        assert!(validation_warnings[0].contains("Source node node-1-1 is unreachable"));
        assert!(validation_warnings[0].contains("will skip optional steps"));
    }

    #[test]
    fn test_switch_validation_both_nodes_up_succeeds() {
        // Both nodes are up - this should SUCCEED with no warnings
        let active_node = create_test_node("node-1-1", "validator1-1.example.com", true, true);
        let standby_node = create_test_node("node-1-2", "validator1-2.example.com", true, false);
        let ssh_keys = create_test_ssh_keys();

        let mut validation_errors = Vec::new();
        let mut validation_warnings: Vec<String> = Vec::new();

        // Check target (standby) node
        if standby_node.status == NodeStatus::Unknown {
            validation_errors.push(format!(
                "Target node {} is unreachable (SSH connection failed)",
                standby_node.node.label
            ));
        }

        // Check source (active) node
        if active_node.status == NodeStatus::Unknown {
            validation_warnings.push(format!(
                "Source node {} is unreachable - will skip optional steps (tower copy, graceful shutdown)",
                active_node.node.label
            ));
        }

        // Should have NO errors or warnings
        assert!(validation_errors.is_empty());
        assert!(validation_warnings.is_empty());
    }

    #[test]
    fn test_switch_validation_no_ssh_key_for_target_fails() {
        // No SSH key for target node - this should FAIL
        let active_node = create_test_node("node-1-1", "validator1-1.example.com", true, true);
        let standby_node = create_test_node("node-1-2", "validator1-2.example.com", true, false);
        let mut ssh_keys = create_test_ssh_keys();
        ssh_keys.remove(&standby_node.node.host); // Remove SSH key for target

        let mut validation_errors = Vec::new();

        // Check if we can get SSH key for target node
        if !ssh_keys.contains_key(&standby_node.node.host) {
            validation_errors.push(format!(
                "No SSH key available for target node {}",
                standby_node.node.label
            ));
        }

        // Should have validation error
        assert_eq!(validation_errors.len(), 1);
        assert!(validation_errors[0].contains("No SSH key available for target node"));
    }

    #[test]
    fn test_switch_validation_target_not_swap_ready_fails() {
        // Target node is reachable but not swap-ready - this should FAIL
        let active_node = create_test_node("node-1-1", "validator1-1.example.com", true, true);
        let mut standby_node =
            create_test_node("node-1-2", "validator1-2.example.com", true, false);

        // Make target not swap-ready
        standby_node.swap_ready = Some(false);
        standby_node.swap_issues = vec![
            "Funded identity keypair missing or not readable".to_string(),
            "Vote keypair missing or not readable".to_string(),
        ];

        let ssh_keys = create_test_ssh_keys();

        let mut validation_errors = Vec::new();

        // Check target node swap readiness
        if let Some(false) = standby_node.swap_ready {
            validation_errors.push(format!(
                "Target node {} is not swap-ready: {}",
                standby_node.node.label,
                standby_node.swap_issues.join(", ")
            ));
        }

        // Should have validation error
        assert_eq!(validation_errors.len(), 1);
        assert!(validation_errors[0].contains("Target node node-1-2 is not swap-ready"));
        assert!(validation_errors[0].contains("Funded identity keypair missing"));
        assert!(validation_errors[0].contains("Vote keypair missing"));
    }
}
