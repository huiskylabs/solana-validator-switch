#[cfg(test)]
mod tests {
    use crate::commands::switch::SwitchManager;
    use crate::types::*;
    use std::sync::{Arc, Mutex};
    use anyhow::{Result, anyhow};
    
    // Mock SSH connection pool for testing
    struct MockSshPool {
        responses: std::collections::HashMap<String, String>,
        should_fail: bool,
        fail_message: String,
        command_history: Arc<Mutex<Vec<(String, String)>>>, // (node_host, command)
    }
    
    impl MockSshPool {
        fn new() -> Self {
            Self {
                responses: std::collections::HashMap::new(),
                should_fail: false,
                fail_message: String::from("Mock SSH failure"),
                command_history: Arc::new(Mutex::new(Vec::new())),
            }
        }
        
        fn with_response(mut self, pattern: &str, response: &str) -> Self {
            self.responses.insert(pattern.to_string(), response.to_string());
            self
        }
        
        fn with_failure(mut self, message: &str) -> Self {
            self.should_fail = true;
            self.fail_message = message.to_string();
            self
        }
        
        async fn execute_command(&mut self, node: &NodeConfig, _key_path: &str, command: &str) -> Result<String> {
            // Record command history
            self.command_history.lock().unwrap().push((node.host.clone(), command.to_string()));
            
            if self.should_fail {
                return Err(anyhow!(self.fail_message.clone()));
            }
            
            // Match command patterns
            for (pattern, response) in &self.responses {
                if command.contains(pattern) {
                    return Ok(response.clone());
                }
            }
            
            // Default responses
            if command.contains("ps aux") {
                Ok("solana    1234  0.0  0.0   1234  5678 ?        Sl   Jan01   0:00 agave-validator".to_string())
            } else if command.contains("ls -t") && command.contains("tower") {
                Ok("/mnt/solana_ledger/tower-1_9-12345.bin".to_string())
            } else if command.contains("base64") && !command.contains("-d") {
                Ok("SGVsbG8gV29ybGQK".to_string()) // "Hello World\n" in base64
            } else {
                Ok(String::new())
            }
        }
        
        async fn execute_command_with_input(&mut self, node: &NodeConfig, command: &str, _input: &str) -> Result<String> {
            self.command_history.lock().unwrap().push((node.host.clone(), command.to_string()));
            
            if self.should_fail {
                return Err(anyhow!(self.fail_message.clone()));
            }
            
            Ok(String::new())
        }
    }
    
    fn create_test_node(label: &str, host: &str, funded: &str, unfunded: &str) -> NodeConfig {
        NodeConfig {
            label: label.to_string(),
            host: host.to_string(),
            port: 22,
            user: "solana".to_string(),
            paths: NodePaths {
                funded_identity: funded.to_string(),
                unfunded_identity: unfunded.to_string(),
                vote_keypair: "/home/solana/vote.json".to_string(),
                ledger: "/mnt/solana_ledger".to_string(),
                tower: "/mnt/solana_ledger/tower-1_9-*.bin".to_string(),
                solana_cli_path: "/home/solana/.local/share/solana/install/active_release/bin/solana".to_string(),
                firedancer_config: None,
                fdctl_path: None,
            },
        }
    }
    
    fn create_test_node_with_status(node: NodeConfig, status: NodeStatus, validator_type: ValidatorType) -> NodeWithStatus {
        NodeWithStatus {
            node,
            status,
            validator_type,
            agave_validator_executable: Some("/usr/bin/agave-validator".to_string()),
            fdctl_executable: None,
            version: Some("1.18.0".to_string()),
            sync_status: Some("Caught up".to_string()),
            current_identity: None,
            swap_ready: Some(true),
            swap_issues: vec![],
        }
    }
    
    #[tokio::test]
    async fn test_switch_manager_creation() {
        let active_node = create_test_node("Node1", "1.2.3.4", "/funded1.json", "/unfunded1.json");
        let standby_node = create_test_node("Node2", "5.6.7.8", "/funded2.json", "/unfunded2.json");
        
        let active_with_status = create_test_node_with_status(active_node, NodeStatus::Active, ValidatorType::Agave);
        let standby_with_status = create_test_node_with_status(standby_node, NodeStatus::Standby, ValidatorType::Agave);
        
        let validator_pair = ValidatorPair {
            vote_pubkey: "Vote123".to_string(),
            identity_pubkey: "Identity123".to_string(),
            rpc: "https://api.mainnet-beta.solana.com".to_string(),
            local_ssh_key_path: "~/.ssh/id_rsa".to_string(),
            nodes: vec![],
        };
        
        let ssh_pool = Arc::new(Mutex::new(MockSshPool::new()));
        
        let manager = SwitchManager::new(
            active_with_status.clone(),
            standby_with_status.clone(),
            validator_pair.clone(),
            ssh_pool.clone()
        );
        
        assert_eq!(manager.active_node_with_status.node.host, "1.2.3.4");
        assert_eq!(manager.standby_node_with_status.node.host, "5.6.7.8");
    }
    
    #[tokio::test]
    async fn test_agave_identity_switch_command_generation() {
        let active_node = create_test_node("Node1", "1.2.3.4", "/funded1.json", "/unfunded1.json");
        let active_with_status = create_test_node_with_status(active_node, NodeStatus::Active, ValidatorType::Agave);
        
        let validator_pair = ValidatorPair {
            vote_pubkey: "Vote123".to_string(),
            identity_pubkey: "Identity123".to_string(),
            rpc: "https://api.mainnet-beta.solana.com".to_string(),
            local_ssh_key_path: "~/.ssh/id_rsa".to_string(),
            nodes: vec![],
        };
        
        let mut ssh_pool = MockSshPool::new()
            .with_response("ps aux", "solana 1234 agave-validator --identity");
        
        let ssh_pool_arc = Arc::new(Mutex::new(ssh_pool));
        let mut manager = SwitchManager::new(
            active_with_status.clone(),
            active_with_status.clone(), // Using same node for simplicity
            validator_pair,
            ssh_pool_arc.clone()
        );
        
        // Test switch_primary_to_unfunded
        let result = manager.switch_primary_to_unfunded(true).await;
        assert!(result.is_ok());
        
        // Check command history
        let history = ssh_pool_arc.lock().unwrap().command_history.lock().unwrap().clone();
        assert!(history.iter().any(|(_, cmd)| cmd.contains("agave-validator") && cmd.contains("set-identity")));
        assert!(history.iter().any(|(_, cmd)| cmd.contains("--require-tower")));
        assert!(history.iter().any(|(_, cmd)| cmd.contains("/unfunded1.json")));
    }
    
    #[tokio::test]
    async fn test_firedancer_identity_switch_command_generation() {
        let mut active_node = create_test_node("Node1", "1.2.3.4", "/funded1.json", "/unfunded1.json");
        active_node.paths.firedancer_config = Some("/home/solana/firedancer-config.toml".to_string());
        
        let mut active_with_status = create_test_node_with_status(active_node, NodeStatus::Active, ValidatorType::Firedancer);
        active_with_status.fdctl_executable = Some("/home/solana/fdctl".to_string());
        
        let validator_pair = ValidatorPair {
            vote_pubkey: "Vote123".to_string(),
            identity_pubkey: "Identity123".to_string(),
            rpc: "https://api.mainnet-beta.solana.com".to_string(),
            local_ssh_key_path: "~/.ssh/id_rsa".to_string(),
            nodes: vec![],
        };
        
        let ssh_pool = MockSshPool::new()
            .with_response("ps aux", "solana 1234 /home/solana/fdctl run --config /home/solana/firedancer-config.toml");
        
        let ssh_pool_arc = Arc::new(Mutex::new(ssh_pool));
        let mut manager = SwitchManager::new(
            active_with_status.clone(),
            active_with_status.clone(),
            validator_pair,
            ssh_pool_arc.clone()
        );
        
        let result = manager.switch_primary_to_unfunded(true).await;
        assert!(result.is_ok());
        
        let history = ssh_pool_arc.lock().unwrap().command_history.lock().unwrap().clone();
        assert!(history.iter().any(|(_, cmd)| cmd.contains("/home/solana/fdctl set-identity")));
        assert!(history.iter().any(|(_, cmd)| cmd.contains("--config /home/solana/firedancer-config.toml")));
        assert!(history.iter().any(|(_, cmd)| cmd.contains("/unfunded1.json")));
    }
    
    #[tokio::test]
    async fn test_firedancer_config_auto_detection() {
        let mut active_node = create_test_node("Node1", "1.2.3.4", "/funded1.json", "/unfunded1.json");
        // Don't set firedancer_config to test auto-detection
        
        let mut active_with_status = create_test_node_with_status(active_node, NodeStatus::Active, ValidatorType::Firedancer);
        active_with_status.fdctl_executable = Some("/home/solana/fdctl".to_string());
        
        let validator_pair = ValidatorPair {
            vote_pubkey: "Vote123".to_string(),
            identity_pubkey: "Identity123".to_string(),
            rpc: "https://api.mainnet-beta.solana.com".to_string(),
            local_ssh_key_path: "~/.ssh/id_rsa".to_string(),
            nodes: vec![],
        };
        
        // Simulate fdctl process with config in command line
        let ssh_pool = MockSshPool::new()
            .with_response("ps aux", "90384 39424 ? SL Jul09 0:01 /home/solana/firedancer/build/native/gcc/bin/fdctl run --config /home/solana/auto-detected-config.toml");
        
        let ssh_pool_arc = Arc::new(Mutex::new(ssh_pool));
        let mut manager = SwitchManager::new(
            active_with_status.clone(),
            active_with_status.clone(),
            validator_pair,
            ssh_pool_arc.clone()
        );
        
        let result = manager.switch_primary_to_unfunded(true).await;
        assert!(result.is_ok());
        
        let history = ssh_pool_arc.lock().unwrap().command_history.lock().unwrap().clone();
        assert!(history.iter().any(|(_, cmd)| cmd.contains("--config /home/solana/auto-detected-config.toml")));
    }
    
    #[tokio::test]
    async fn test_tower_file_transfer() {
        let active_node = create_test_node("Node1", "1.2.3.4", "/funded1.json", "/unfunded1.json");
        let standby_node = create_test_node("Node2", "5.6.7.8", "/funded2.json", "/unfunded2.json");
        
        let active_with_status = create_test_node_with_status(active_node, NodeStatus::Active, ValidatorType::Agave);
        let standby_with_status = create_test_node_with_status(standby_node, NodeStatus::Standby, ValidatorType::Agave);
        
        let validator_pair = ValidatorPair {
            vote_pubkey: "Vote123".to_string(),
            identity_pubkey: "Identity123".to_string(),
            rpc: "https://api.mainnet-beta.solana.com".to_string(),
            local_ssh_key_path: "~/.ssh/id_rsa".to_string(),
            nodes: vec![],
        };
        
        let ssh_pool = MockSshPool::new()
            .with_response("ls -t", "/mnt/solana_ledger/tower-1_9-12345.bin")
            .with_response("base64", "SGVsbG8gV29ybGQK"); // Mock tower file content
        
        let ssh_pool_arc = Arc::new(Mutex::new(ssh_pool));
        let mut manager = SwitchManager::new(
            active_with_status,
            standby_with_status,
            validator_pair,
            ssh_pool_arc.clone()
        );
        
        let result = manager.transfer_tower_file(false).await;
        assert!(result.is_ok());
        assert_eq!(manager.tower_file_name, Some("tower-1_9-12345.bin".to_string()));
        assert!(manager.tower_transfer_time.is_some());
        
        let history = ssh_pool_arc.lock().unwrap().command_history.lock().unwrap().clone();
        assert!(history.iter().any(|(_, cmd)| cmd.contains("base64 /mnt/solana_ledger/tower-1_9-12345.bin")));
        assert!(history.iter().any(|(_, cmd)| cmd.contains("base64 -d > /mnt/solana_ledger/tower-1_9-12345.bin")));
    }
    
    #[tokio::test]
    async fn test_tower_file_not_found() {
        let active_node = create_test_node("Node1", "1.2.3.4", "/funded1.json", "/unfunded1.json");
        let standby_node = create_test_node("Node2", "5.6.7.8", "/funded2.json", "/unfunded2.json");
        
        let active_with_status = create_test_node_with_status(active_node, NodeStatus::Active, ValidatorType::Agave);
        let standby_with_status = create_test_node_with_status(standby_node, NodeStatus::Standby, ValidatorType::Agave);
        
        let validator_pair = ValidatorPair {
            vote_pubkey: "Vote123".to_string(),
            identity_pubkey: "Identity123".to_string(),
            rpc: "https://api.mainnet-beta.solana.com".to_string(),
            local_ssh_key_path: "~/.ssh/id_rsa".to_string(),
            nodes: vec![],
        };
        
        let ssh_pool = MockSshPool::new()
            .with_response("ls -t", ""); // No tower file found
        
        let ssh_pool_arc = Arc::new(Mutex::new(ssh_pool));
        let mut manager = SwitchManager::new(
            active_with_status,
            standby_with_status,
            validator_pair,
            ssh_pool_arc
        );
        
        let result = manager.transfer_tower_file(false).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No tower file found"));
    }
    
    #[tokio::test]
    async fn test_ssh_connection_failure() {
        let active_node = create_test_node("Node1", "1.2.3.4", "/funded1.json", "/unfunded1.json");
        let active_with_status = create_test_node_with_status(active_node, NodeStatus::Active, ValidatorType::Agave);
        
        let validator_pair = ValidatorPair {
            vote_pubkey: "Vote123".to_string(),
            identity_pubkey: "Identity123".to_string(),
            rpc: "https://api.mainnet-beta.solana.com".to_string(),
            local_ssh_key_path: "~/.ssh/id_rsa".to_string(),
            nodes: vec![],
        };
        
        let ssh_pool = MockSshPool::new()
            .with_failure("SSH connection timeout");
        
        let ssh_pool_arc = Arc::new(Mutex::new(ssh_pool));
        let mut manager = SwitchManager::new(
            active_with_status.clone(),
            active_with_status.clone(),
            validator_pair,
            ssh_pool_arc
        );
        
        let result = manager.switch_primary_to_unfunded(true).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("SSH connection timeout"));
    }
    
    #[tokio::test]
    async fn test_missing_executable_paths() {
        let active_node = create_test_node("Node1", "1.2.3.4", "/funded1.json", "/unfunded1.json");
        let mut active_with_status = create_test_node_with_status(active_node, NodeStatus::Active, ValidatorType::Firedancer);
        // Don't set fdctl_executable or firedancer_config
        active_with_status.fdctl_executable = None;
        active_with_status.node.paths.fdctl_path = None;
        active_with_status.node.paths.firedancer_config = None;
        
        let validator_pair = ValidatorPair {
            vote_pubkey: "Vote123".to_string(),
            identity_pubkey: "Identity123".to_string(),
            rpc: "https://api.mainnet-beta.solana.com".to_string(),
            local_ssh_key_path: "~/.ssh/id_rsa".to_string(),
            nodes: vec![],
        };
        
        let ssh_pool = MockSshPool::new()
            .with_response("ps aux", "solana 1234 fdctl run"); // Firedancer but no config in ps
        
        let ssh_pool_arc = Arc::new(Mutex::new(ssh_pool));
        let mut manager = SwitchManager::new(
            active_with_status.clone(),
            active_with_status.clone(),
            validator_pair,
            ssh_pool_arc
        );
        
        let result = manager.switch_primary_to_unfunded(true).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("fdctl executable path not found"));
    }
    
    #[tokio::test]
    async fn test_verify_backup_catchup_success() {
        let standby_node = create_test_node("Node2", "5.6.7.8", "/funded2.json", "/unfunded2.json");
        let standby_with_status = create_test_node_with_status(standby_node, NodeStatus::Standby, ValidatorType::Agave);
        
        let validator_pair = ValidatorPair {
            vote_pubkey: "Vote123".to_string(),
            identity_pubkey: "Identity123".to_string(),
            rpc: "https://api.mainnet-beta.solana.com".to_string(),
            local_ssh_key_path: "~/.ssh/id_rsa".to_string(),
            nodes: vec![],
        };
        
        let ssh_pool = MockSshPool::new()
            .with_response("catchup", "Validator has caught up to slot 12345");
        
        let ssh_pool_arc = Arc::new(Mutex::new(ssh_pool));
        let mut manager = SwitchManager::new(
            standby_with_status.clone(),
            standby_with_status.clone(),
            validator_pair,
            ssh_pool_arc.clone()
        );
        
        let result = manager.verify_backup_catchup(false).await;
        assert!(result.is_ok());
        
        let history = ssh_pool_arc.lock().unwrap().command_history.lock().unwrap().clone();
        assert!(history.iter().any(|(_, cmd)| cmd.contains("catchup --our-localhost")));
    }
    
    #[tokio::test]
    async fn test_timing_measurements() {
        let active_node = create_test_node("Node1", "1.2.3.4", "/funded1.json", "/unfunded1.json");
        let standby_node = create_test_node("Node2", "5.6.7.8", "/funded2.json", "/unfunded2.json");
        
        let active_with_status = create_test_node_with_status(active_node, NodeStatus::Active, ValidatorType::Agave);
        let standby_with_status = create_test_node_with_status(standby_node, NodeStatus::Standby, ValidatorType::Agave);
        
        let validator_pair = ValidatorPair {
            vote_pubkey: "Vote123".to_string(),
            identity_pubkey: "Identity123".to_string(),
            rpc: "https://api.mainnet-beta.solana.com".to_string(),
            local_ssh_key_path: "~/.ssh/id_rsa".to_string(),
            nodes: vec![],
        };
        
        let ssh_pool = MockSshPool::new()
            .with_response("ps aux", "solana 1234 agave-validator")
            .with_response("ls -t", "/mnt/solana_ledger/tower-1_9-12345.bin")
            .with_response("base64", "SGVsbG8gV29ybGQK")
            .with_response("catchup", "has caught up");
        
        let ssh_pool_arc = Arc::new(Mutex::new(ssh_pool));
        let mut manager = SwitchManager::new(
            active_with_status,
            standby_with_status,
            validator_pair,
            ssh_pool_arc
        );
        
        let result = manager.execute_switch(false).await;
        assert!(result.is_ok());
        
        // Verify all timing measurements were recorded
        assert!(manager.active_switch_time.is_some());
        assert!(manager.tower_transfer_time.is_some());
        assert!(manager.standby_switch_time.is_some());
        assert!(manager.identity_switch_time.is_some());
        
        // Verify total time is greater than sum of individual steps
        let total_time = manager.identity_switch_time.unwrap();
        let step_times = manager.active_switch_time.unwrap() + 
                        manager.tower_transfer_time.unwrap() + 
                        manager.standby_switch_time.unwrap();
        assert!(total_time >= step_times);
    }
}