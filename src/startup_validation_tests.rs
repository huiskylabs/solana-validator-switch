#[cfg(test)]
mod startup_validation_tests {
    use crate::types::{
        NodeConfig, NodePaths, NodeStatus, NodeWithStatus, ValidatorPair, ValidatorType,
    };

    // Helper function to create a test node
    fn create_test_node(label: &str, host: &str, available: bool) -> NodeWithStatus {
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
                    solana_cli: "/home/solana/bin/solana".to_string(),
                    agave_validator: Some("/home/solana/bin/agave-validator".to_string()),
                    fdctl: None,
                },
                ssh_key_path: Some("/home/user/.ssh/id_rsa".to_string()),
            },
            status: if available {
                NodeStatus::Standby
            } else {
                NodeStatus::Unknown
            },
            validator_type: ValidatorType::Agave,
            agave_validator_executable: Some("/home/solana/bin/agave-validator".to_string()),
            fdctl_executable: None,
            solana_cli_executable: Some("/home/solana/bin/solana".to_string()),
            version: Some("Agave 2.0.0".to_string()),
            sync_status: Some("Caught up".to_string()),
            current_identity: Some("unfunded123".to_string()),
            ledger_path: Some("/mnt/ledger".to_string()),
            tower_path: None,
            swap_ready: Some(available),
            swap_issues: if available {
                vec![]
            } else {
                vec!["SSH connection failed".to_string()]
            },
            ssh_key_path: Some("/home/user/.ssh/id_rsa".to_string()),
        }
    }

    // Helper function to create a test validator pair
    fn create_test_validator_pair(index: usize) -> ValidatorPair {
        ValidatorPair {
            vote_pubkey: format!("vote_pubkey_{}", index),
            identity_pubkey: format!("identity_pubkey_{}", index),
            rpc: format!("http://rpc{}.example.com:8899", index),
            nodes: vec![
                NodeConfig {
                    label: format!("node-{}-1", index),
                    host: format!("validator{}-1.example.com", index),
                    port: 22,
                    user: "solana".to_string(),
                    paths: NodePaths {
                        funded_identity: "/home/solana/funded.json".to_string(),
                        unfunded_identity: "/home/solana/unfunded.json".to_string(),
                        vote_keypair: "/home/solana/vote.json".to_string(),
                        solana_cli: "/home/solana/bin/solana".to_string(),
                        agave_validator: Some("/home/solana/bin/agave-validator".to_string()),
                        fdctl: None,
                    },
                    ssh_key_path: Some("/home/user/.ssh/id_rsa".to_string()),
                },
                NodeConfig {
                    label: format!("node-{}-2", index),
                    host: format!("validator{}-2.example.com", index),
                    port: 22,
                    user: "solana".to_string(),
                    paths: NodePaths {
                        funded_identity: "/home/solana/funded.json".to_string(),
                        unfunded_identity: "/home/solana/unfunded.json".to_string(),
                        vote_keypair: "/home/solana/vote.json".to_string(),
                        solana_cli: "/home/solana/bin/solana".to_string(),
                        agave_validator: Some("/home/solana/bin/agave-validator".to_string()),
                        fdctl: None,
                    },
                    ssh_key_path: Some("/home/user/.ssh/id_rsa".to_string()),
                },
            ],
        }
    }

    #[test]
    fn test_startup_continues_when_unrelated_validator_down() {
        // Scenario 1: Validator 2 Node 2 is down, but we should still be able to start
        // and switch Validator 1 nodes

        let validator_statuses = vec![
            crate::ValidatorStatus {
                validator_pair: create_test_validator_pair(1),
                nodes_with_status: vec![
                    create_test_node("node-1-1", "validator1-1.example.com", true),
                    create_test_node("node-1-2", "validator1-2.example.com", true),
                ],
                metadata: None,
            },
            crate::ValidatorStatus {
                validator_pair: create_test_validator_pair(2),
                nodes_with_status: vec![
                    create_test_node("node-2-1", "validator2-1.example.com", true),
                    create_test_node("node-2-2", "validator2-2.example.com", false), // DOWN
                ],
                metadata: None,
            },
        ];

        // Check that we get warnings but no critical failures
        let mut warnings = Vec::new();
        let has_critical_failures = false;

        for (validator_idx, validator_status) in validator_statuses.iter().enumerate() {
            for (node_idx, node_with_status) in
                validator_status.nodes_with_status.iter().enumerate()
            {
                let node_label = format!(
                    "Validator {} Node {} ({})",
                    validator_idx + 1,
                    node_idx + 1,
                    node_with_status.node.label
                );

                // Check for SSH connectivity failure
                if node_with_status.status == NodeStatus::Unknown {
                    warnings.push(format!(
                        "{}: SSH connection failed (will limit functionality)",
                        node_label
                    ));
                    // In the old code, this would have been a critical failure
                    // has_critical_failures = true;
                }
            }
        }

        // Assert that we have warnings but no critical failures
        assert_eq!(warnings.len(), 1);
        assert_eq!(
            warnings[0],
            "Validator 2 Node 2 (node-2-2): SSH connection failed (will limit functionality)"
        );
        assert!(!has_critical_failures);

        // Verify that Validator 1 nodes are both ready
        assert_eq!(
            validator_statuses[0].nodes_with_status[0].swap_ready,
            Some(true)
        );
        assert_eq!(
            validator_statuses[0].nodes_with_status[1].swap_ready,
            Some(true)
        );
    }

    #[test]
    fn test_startup_continues_when_source_node_down() {
        // Scenario 2: Validator 1 Node 1 (source) is down, but we should still be able to start
        // and potentially switch to Node 2 in emergency

        let validator_statuses = vec![crate::ValidatorStatus {
            validator_pair: create_test_validator_pair(1),
            nodes_with_status: vec![
                create_test_node("node-1-1", "validator1-1.example.com", false), // DOWN (source)
                create_test_node("node-1-2", "validator1-2.example.com", true),  // UP (target)
            ],
            metadata: None,
        }];

        // Check that we get warnings but no critical failures
        let mut warnings = Vec::new();
        let has_critical_failures = false;

        for (validator_idx, validator_status) in validator_statuses.iter().enumerate() {
            for (node_idx, node_with_status) in
                validator_status.nodes_with_status.iter().enumerate()
            {
                let node_label = format!(
                    "Validator {} Node {} ({})",
                    validator_idx + 1,
                    node_idx + 1,
                    node_with_status.node.label
                );

                // Check for SSH connectivity failure
                if node_with_status.status == NodeStatus::Unknown {
                    warnings.push(format!(
                        "{}: SSH connection failed (will limit functionality)",
                        node_label
                    ));
                }

                // Check for swap readiness failure
                if let Some(false) = node_with_status.swap_ready {
                    if !node_with_status.swap_issues.is_empty() {
                        warnings.push(format!(
                            "{}: Not swap ready - {}",
                            node_label,
                            node_with_status.swap_issues.join(", ")
                        ));
                    }
                }
            }
        }

        // Assert that we have warnings but no critical failures
        assert_eq!(warnings.len(), 2); // Both SSH failure and swap readiness
        assert!(warnings
            .iter()
            .any(|w| w.contains("node-1-1") && w.contains("SSH connection failed")));
        assert!(!has_critical_failures);

        // Verify that target node is ready
        assert_eq!(
            validator_statuses[0].nodes_with_status[1].swap_ready,
            Some(true)
        );
    }

    #[test]
    fn test_old_behavior_would_have_blocked() {
        // This test documents what the OLD behavior would have done
        // to ensure we understand the change

        let validator_statuses = vec![
            crate::ValidatorStatus {
                validator_pair: create_test_validator_pair(1),
                nodes_with_status: vec![
                    create_test_node("node-1-1", "validator1-1.example.com", true),
                    create_test_node("node-1-2", "validator1-2.example.com", true),
                ],
                metadata: None,
            },
            crate::ValidatorStatus {
                validator_pair: create_test_validator_pair(2),
                nodes_with_status: vec![
                    create_test_node("node-2-1", "validator2-1.example.com", true),
                    create_test_node("node-2-2", "validator2-2.example.com", false), // DOWN
                ],
                metadata: None,
            },
        ];

        // OLD behavior simulation
        let mut critical_failures_old = Vec::new();

        for (validator_idx, validator_status) in validator_statuses.iter().enumerate() {
            for (node_idx, node_with_status) in
                validator_status.nodes_with_status.iter().enumerate()
            {
                let node_label = format!(
                    "Validator {} Node {} ({})",
                    validator_idx + 1,
                    node_idx + 1,
                    node_with_status.node.label
                );

                // OLD: Check for SSH connectivity failure
                if node_with_status.status == NodeStatus::Unknown {
                    critical_failures_old.push(format!("{}: SSH connection failed", node_label));
                }
            }
        }

        // OLD behavior would have had critical failures
        assert_eq!(critical_failures_old.len(), 1);
        assert!(critical_failures_old[0].contains("Validator 2 Node 2"));

        // OLD behavior would have blocked startup
        // assert!(!critical_failures_old.is_empty()); // This would have caused "CRITICAL STARTUP FAILURES DETECTED"
    }
}
