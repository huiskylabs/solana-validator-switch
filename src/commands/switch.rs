use anyhow::{Result, anyhow};
use colored::*;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

use crate::types::NodeConfig;

pub async fn switch_command(dry_run: bool, app_state: &crate::AppState) -> Result<()> {
    // Validate we have at least one validator configured
    if app_state.config.validators.is_empty() {
        return Err(anyhow!("No validators configured"));
    }
    
    // For now, use the first validator
    let validator_status = &app_state.validator_statuses[0];
    let validator_pair = &validator_status.validator_pair;
    
    // Find active and standby nodes
    let active_node = validator_status.nodes_with_status.iter()
        .find(|n| n.status == crate::types::NodeStatus::Active)
        .map(|n| &n.node);
    let standby_node = validator_status.nodes_with_status.iter()
        .find(|n| n.status == crate::types::NodeStatus::Standby)
        .map(|n| &n.node);
    
    let (active_node, standby_node) = match (active_node, standby_node) {
        (Some(active), Some(standby)) => (active, standby),
        _ => {
            // If we can't determine status, use the first two nodes
            if validator_pair.nodes.len() < 2 {
                return Err(anyhow!("Validator must have at least 2 nodes configured"));
            }
            (&validator_pair.nodes[0], &validator_pair.nodes[1])
        }
    };
    
    println!("\n{}", format!("🔄 Validator Switch - {} Mode", if dry_run { "DRY RUN" } else { "LIVE" }).bright_cyan().bold());
    println!("{}", "━".repeat(50).dimmed());
    
    if dry_run {
        println!("{}", "ℹ️  This is a DRY RUN - showing what would be executed".yellow());
        println!("{}", "ℹ️  Tower file transfer will be performed to measure timing".yellow());
        println!();
    }
    
    let mut switch_manager = SwitchManager::new(
        active_node.clone(), 
        standby_node.clone(), 
        validator_pair.clone(),
        app_state.ssh_pool.clone()
    );
    
    // Execute the switch process
    switch_manager.execute_switch(dry_run).await?;
    
    Ok(())
}

struct SwitchManager {
    active_node: NodeConfig,
    standby_node: NodeConfig,
    validator_pair: crate::types::ValidatorPair,
    ssh_pool: Arc<Mutex<crate::ssh::SshConnectionPool>>,
    tower_file_name: Option<String>,
    tower_transfer_time: Option<Duration>,
}

impl SwitchManager {
    fn new(active_node: NodeConfig, standby_node: NodeConfig, validator_pair: crate::types::ValidatorPair, ssh_pool: Arc<Mutex<crate::ssh::SshConnectionPool>>) -> Self {
        Self {
            active_node,
            standby_node,
            validator_pair,
            ssh_pool,
            tower_file_name: None,
            tower_transfer_time: None,
        }
    }
    
    async fn execute_switch(&mut self, dry_run: bool) -> Result<()> {
        // Phase 1: Pre-flight checks (skip for dry run)
        if !dry_run {
            println!("{}", "📋 Phase 1: Pre-flight Checks".bright_blue().bold());
            self.preflight_checks(dry_run).await?;
        }
        
        // Phase 2: Switch active node to unfunded identity
        println!("{}\n{}", if dry_run { "" } else { "\n" }, "🔄 Switch Active Node to Unfunded Identity".bright_blue().bold());
        self.switch_primary_to_unfunded(dry_run).await?;
        
        // Phase 3: Transfer tower file
        println!("\n{}", "📤 Transfer Tower File".bright_blue().bold());
        self.transfer_tower_file(dry_run).await?;
        
        // Phase 4: Switch standby node to funded identity
        println!("\n{}", "🚀 Switch Standby Node to Funded Identity".bright_blue().bold());
        self.switch_backup_to_funded(dry_run).await?;
        
        // Phase 5: Verify standby catchup
        println!("\n{}", "✅ Verify Standby Catchup".bright_blue().bold());
        self.verify_backup_catchup(dry_run).await?;
        
        // Summary
        self.print_summary(dry_run);
        
        Ok(())
    }
    
    async fn preflight_checks(&mut self, dry_run: bool) -> Result<()> {
        println!("  ⏱️  Estimated time: 2-3 seconds");
        println!();
        
        // Use existing SSH connections from the pool
        println!("  📊 Checking validator states:");
        
        // Check both validators are running
        let validator_check_cmd = "ps aux | grep -E 'solana-validator|agave|fdctl|firedancer' | grep -v grep";
        
        // Check active node is running
        if dry_run {
            println!("  Active Node: {}", format!("ssh {}@{} '{}'", self.active_node.user, self.active_node.host, validator_check_cmd).dimmed());
        }
        
        let active_running = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.active_node, &self.validator_pair.local_ssh_key_path, validator_check_cmd).await?
        };
        let active_is_running = !active_running.trim().is_empty();
        
        if active_is_running {
            println!("    ✅ Active validator is running");
        } else {
            return Err(anyhow!("❌ Active validator is not running - cannot perform switch"));
        }
        
        // Check standby node is also running (both should be running for identity swap)
        if dry_run {
            println!("  Standby Node: {}", format!("ssh {}@{} '{}'", self.standby_node.user, self.standby_node.host, validator_check_cmd).dimmed());
        }
        
        let standby_running = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.standby_node, &self.validator_pair.local_ssh_key_path, validator_check_cmd).await?
        };
        let standby_is_running = !standby_running.trim().is_empty();
        
        if standby_is_running {
            println!("    ✅ Standby validator is running");
        } else {
            return Err(anyhow!("❌ Standby validator is not running - both validators must be running for identity swap"));
        }
        
        // Detect validator type for proper switching commands
        let validator_type = if active_running.contains("fdctl") || active_running.contains("firedancer") {
            "firedancer"
        } else if active_running.contains("agave") {
            "agave"
        } else {
            "solana"
        };
        
        println!("    🔍 Detected validator type: {}", validator_type);
        
        // Check tower file exists on active node
        let tower_check_cmd = format!("ls -la {}/tower-1_9-*.bin 2>/dev/null | head -1", self.active_node.paths.ledger);
        if dry_run {
            println!("\n  Tower file check: {}", format!("ssh {}@{} '{}'", self.active_node.user, self.active_node.host, tower_check_cmd).dimmed());
        }
        
        let tower_result = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.active_node, &self.validator_pair.local_ssh_key_path, &tower_check_cmd).await?
        };
        
        if tower_result.trim().is_empty() {
            return Err(anyhow!("❌ No tower file found on active validator"));
        } else {
            println!("    ✅ Tower file found on active node");
        }
        
        Ok(())
    }
    
    async fn switch_primary_to_unfunded(&mut self, dry_run: bool) -> Result<()> {
        // Detect validator type to use appropriate command
        let process_info = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.active_node, &self.validator_pair.local_ssh_key_path, "ps aux | grep -E 'solana-validator|agave|fdctl|firedancer' | grep -v grep").await?
        };
        
        let (subtitle, switch_command) = if process_info.contains("fdctl") || process_info.contains("firedancer") {
            let default_fdctl = "fdctl".to_string();
            let default_config = "firedancer-config.toml".to_string();
            let fdctl_path = self.active_node.paths.fdctl_path.as_ref().unwrap_or(&default_fdctl);
            let config_path = self.active_node.paths.firedancer_config.as_ref().unwrap_or(&default_config);
            (
                "Using Firedancer fdctl set-identity",
                format!("{} set-identity --config {} {}", fdctl_path, config_path, self.active_node.paths.unfunded_identity)
            )
        } else if process_info.contains("agave-validator") {
            (
                "Using Agave validator restart",
                format!("{} exit && sleep 2 && agave-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                    self.active_node.paths.solana_cli_path,
                    self.active_node.paths.unfunded_identity,
                    self.active_node.paths.vote_keypair,
                    self.active_node.paths.ledger)
            )
        } else {
            (
                "Using Solana validator restart",
                format!("{} exit && sleep 2 && solana-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                    self.active_node.paths.solana_cli_path,
                    self.active_node.paths.unfunded_identity,
                    self.active_node.paths.vote_keypair,
                    self.active_node.paths.ledger)
            )
        };
        
        println!("{}", subtitle.dimmed());
        println!("ssh {}@{} '{}'", self.active_node.user, self.active_node.host, switch_command);
        
        if !dry_run {
            {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.active_node, &self.validator_pair.local_ssh_key_path, &switch_command).await?;
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
            println!("✅ Active validator switched to unfunded identity");
        }
        
        Ok(())
    }
    
    async fn transfer_tower_file(&mut self, dry_run: bool) -> Result<()> {
        // Find the latest tower file
        let find_tower_cmd = format!("ls -t {}/tower-1_9-*.bin 2>/dev/null | head -1", self.active_node.paths.ledger);
        
        let tower_path = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.active_node, &self.validator_pair.local_ssh_key_path, &find_tower_cmd).await?
        };
        let tower_path = tower_path.trim();
        
        if tower_path.is_empty() {
            return Err(anyhow!("No tower file found on active node"));
        }
        
        let tower_filename = tower_path.split('/').last().unwrap_or("tower.bin");
        self.tower_file_name = Some(tower_filename.to_string());
        let dest_path = format!("{}/{}", self.standby_node.paths.ledger, tower_filename);
        
        println!("Transfer from {}@{}:{} to {}@{}:{}", 
            self.active_node.user, self.active_node.host, tower_path,
            self.standby_node.user, self.standby_node.host, dest_path);
        
        let start_time = Instant::now();
        
        // Execute the streaming transfer using base64 encoding
        let read_cmd = format!("base64 {}", tower_path);
        let encoded_data = {
            let mut pool = self.ssh_pool.lock().unwrap();
            match pool.execute_command(&self.active_node, &self.validator_pair.local_ssh_key_path, &read_cmd).await {
                Ok(data) => data,
                Err(e) => return Err(anyhow!("Failed to read tower file: {}", e)),
            }
        };
        
        let write_cmd = format!("base64 -d > {}", dest_path);
        {
            let mut pool = self.ssh_pool.lock().unwrap();
            match pool.execute_command_with_input(&self.standby_node, &write_cmd, &encoded_data).await {
                Ok(_) => {},
                Err(e) => return Err(anyhow!("Failed to write tower file: {}", e)),
            }
        }
        
        let transfer_duration = start_time.elapsed();
        self.tower_transfer_time = Some(transfer_duration);
        
        // Calculate transfer speed
        let file_size = encoded_data.len() as u64 * 3 / 4; // approximate original size from base64
        let speed_mbps = (file_size as f64 / 1024.0 / 1024.0) / transfer_duration.as_secs_f64();
        
        println!("✅ Transferred in {:.0}ms ({:.2} MB/s)", transfer_duration.as_millis(), speed_mbps);
        
        if !dry_run {
            // Verify the file on standby
            let verify_cmd = format!("ls -la {}", dest_path);
            let verify_result = {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.standby_node, &self.validator_pair.local_ssh_key_path, &verify_cmd).await?
            };
            if verify_result.trim().is_empty() {
                return Err(anyhow!("Failed to verify tower file on standby"));
            }
        }
        
        Ok(())
    }
    
    async fn switch_backup_to_funded(&mut self, dry_run: bool) -> Result<()> {
        // Detect validator type to use appropriate command
        let process_info = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.standby_node, &self.validator_pair.local_ssh_key_path, "ps aux | grep -E 'solana-validator|agave|fdctl|firedancer' | grep -v grep").await?
        };
        
        let (subtitle, switch_command) = if process_info.contains("fdctl") || process_info.contains("firedancer") {
            let default_fdctl = "fdctl".to_string();
            let default_config = "firedancer-config.toml".to_string();
            let fdctl_path = self.standby_node.paths.fdctl_path.as_ref().unwrap_or(&default_fdctl);
            let config_path = self.standby_node.paths.firedancer_config.as_ref().unwrap_or(&default_config);
            (
                "Using Firedancer fdctl set-identity",
                format!("{} set-identity --config {} {}", fdctl_path, config_path, self.standby_node.paths.funded_identity)
            )
        } else if process_info.contains("agave-validator") {
            (
                "Using Agave validator restart",
                format!("{} exit && sleep 2 && agave-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                    self.standby_node.paths.solana_cli_path,
                    self.standby_node.paths.funded_identity,
                    self.standby_node.paths.vote_keypair,
                    self.standby_node.paths.ledger)
            )
        } else {
            (
                "Using Solana validator restart",
                format!("{} exit && sleep 2 && solana-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                    self.standby_node.paths.solana_cli_path,
                    self.standby_node.paths.funded_identity,
                    self.standby_node.paths.vote_keypair,
                    self.standby_node.paths.ledger)
            )
        };
        
        println!("{}", subtitle.dimmed());
        println!("ssh {}@{} '{}'", self.standby_node.user, self.standby_node.host, switch_command);
        
        if !dry_run {
            {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.standby_node, &self.validator_pair.local_ssh_key_path, &switch_command).await?;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
            println!("✅ Standby validator switched to funded identity");
        }
        
        Ok(())
    }
    
    async fn verify_backup_catchup(&mut self, dry_run: bool) -> Result<()> {
        let catchup_cmd = format!("{} catchup --our-localhost", self.standby_node.paths.solana_cli_path);
        println!("ssh {}@{} '{}'", self.standby_node.user, self.standby_node.host, catchup_cmd);
        
        if !dry_run {
            tokio::time::sleep(Duration::from_secs(10)).await;
            
            let catchup_result = {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.standby_node, &self.validator_pair.local_ssh_key_path, &catchup_cmd).await?
            };
            
            if catchup_result.contains("has caught up") || catchup_result.contains("slots behind") {
                println!("✅ Standby validator is syncing with funded identity");
            } else {
                println!("⚠️  Standby sync status unclear - monitor manually");
            }
        }
        
        Ok(())
    }
    
    fn print_summary(&self, dry_run: bool) {
        println!();
        if dry_run {
            println!("✅ Dry run completed successfully");
        } else {
            println!("✅ Validator identity switch completed successfully");
        }
    }
}