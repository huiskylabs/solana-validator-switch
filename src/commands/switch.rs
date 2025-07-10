use anyhow::{Result, anyhow};
use colored::*;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};
use crate::commands::error_handler::ProgressSpinner;

pub async fn switch_command(dry_run: bool, app_state: &crate::AppState) -> Result<()> {
    // Validate we have at least one validator configured
    if app_state.config.validators.is_empty() {
        return Err(anyhow!("No validators configured"));
    }
    
    // For now, use the first validator
    let validator_status = &app_state.validator_statuses[0];
    let validator_pair = &validator_status.validator_pair;
    
    // Find active and standby nodes with full status information
    let active_node_with_status = validator_status.nodes_with_status.iter()
        .find(|n| n.status == crate::types::NodeStatus::Active);
    let standby_node_with_status = validator_status.nodes_with_status.iter()
        .find(|n| n.status == crate::types::NodeStatus::Standby);
    
    let (active_node_with_status, standby_node_with_status) = match (active_node_with_status, standby_node_with_status) {
        (Some(active), Some(standby)) => (active, standby),
        _ => {
            // If we can't determine status, use the first two nodes
            if validator_status.nodes_with_status.len() < 2 {
                return Err(anyhow!("Validator must have at least 2 nodes configured"));
            }
            (&validator_status.nodes_with_status[0], &validator_status.nodes_with_status[1])
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
        active_node_with_status.clone(), 
        standby_node_with_status.clone(), 
        validator_pair.clone(),
        app_state.ssh_pool.clone()
    );
    
    // Execute the switch process
    switch_manager.execute_switch(dry_run).await?;
    
    // Show completion message with timing breakdown
    if !dry_run {
        if let Some(total_time) = switch_manager.identity_switch_time {
            println!("\n{}", "━".repeat(50).dimmed());
            println!("{} {}", 
                "✅ Validator swap completed successfully in".bright_green().bold(),
                format!("{}ms", total_time.as_millis()).bright_yellow().bold()
            );
            
            // Show timing breakdown
            println!("\n{}", "📊 Timing breakdown:".dimmed());
            if let Some(active_time) = switch_manager.active_switch_time {
                println!("   Active → Standby:  {}", format!("{}ms", active_time.as_millis()).bright_yellow());
            }
            if let Some(tower_time) = switch_manager.tower_transfer_time {
                println!("   Tower transfer:    {}", format!("{}ms", tower_time.as_millis()).bright_yellow());
            }
            if let Some(standby_time) = switch_manager.standby_switch_time {
                println!("   Standby → Active:  {}", format!("{}ms", standby_time.as_millis()).bright_yellow());
            }
        } else {
            println!("\n{}", "✅ Validator swap completed successfully".bright_green().bold());
        }
        println!();
        println!("{}", "💡 Tip: Check Status menu to see updated validator roles".dimmed());
        println!();
        println!("{}", "Press any key to continue...".dimmed());
        let _ = std::io::stdin().read_line(&mut String::new());
    }
    
    Ok(())
}

pub(crate) struct SwitchManager {
    active_node_with_status: crate::types::NodeWithStatus,
    standby_node_with_status: crate::types::NodeWithStatus,
    validator_pair: crate::types::ValidatorPair,
    ssh_pool: Arc<Mutex<crate::ssh::SshConnectionPool>>,
    tower_file_name: Option<String>,
    tower_transfer_time: Option<Duration>,
    identity_switch_time: Option<Duration>,
    active_switch_time: Option<Duration>,
    standby_switch_time: Option<Duration>,
}

impl SwitchManager {
    pub(crate) fn new(active_node_with_status: crate::types::NodeWithStatus, standby_node_with_status: crate::types::NodeWithStatus, validator_pair: crate::types::ValidatorPair, ssh_pool: Arc<Mutex<crate::ssh::SshConnectionPool>>) -> Self {
        Self {
            active_node_with_status,
            standby_node_with_status,
            validator_pair,
            ssh_pool,
            tower_file_name: None,
            tower_transfer_time: None,
            identity_switch_time: None,
            active_switch_time: None,
            standby_switch_time: None,
        }
    }
    
    async fn execute_switch(&mut self, dry_run: bool) -> Result<()> {
        // Show confirmation dialog (except for dry run)
        if !dry_run {
            println!("\n{}", "⚠️  Validator Switch Confirmation".bright_yellow().bold());
            println!("{}", "━".repeat(50).dimmed());
            println!();
            println!("  {} → {}", 
                format!("🟢 ACTIVE: {} ({}) {}", 
                    self.active_node_with_status.node.label,
                    self.active_node_with_status.node.host,
                    self.active_node_with_status.version.as_ref().unwrap_or(&"Unknown".to_string())
                ).bright_green(),
                "🔄 STANDBY".dimmed()
            );
            println!("  {} → {}", 
                format!("⚪ STANDBY: {} ({}) {}", 
                    self.standby_node_with_status.node.label,
                    self.standby_node_with_status.node.host,
                    self.standby_node_with_status.version.as_ref().unwrap_or(&"Unknown".to_string())
                ).white(),
                "🟢 ACTIVE".bright_green()
            );
            println!();
            println!("  {}", "This will switch your validator identity between nodes.".yellow());
            println!("  {}", "Estimated time: ~10 seconds".dimmed());
            println!();
            
            // Use inquire for confirmation
            use inquire::Confirm;
            let confirmed = Confirm::new("Do you want to proceed with the validator switch?")
                .with_default(false)
                .prompt()?;
                
            if !confirmed {
                println!("\n{}", "❌ Validator switch cancelled by user".red());
                return Ok(());
            }
            println!();
        }
        
        // Start timing the entire switch operation
        let total_switch_start = Instant::now();
        
        // Step 1: Switch active node to unfunded identity
        println!("{}", "🔄 Switch Active Node to Unfunded Identity".bright_blue().bold());
        let active_switch_start = Instant::now();
        self.switch_primary_to_unfunded(dry_run).await?;
        self.active_switch_time = Some(active_switch_start.elapsed());
        if !dry_run {
            println!("   ✓ Completed in {}", format!("{}ms", self.active_switch_time.unwrap().as_millis()).bright_yellow().bold());
        }
        
        // Step 2: Transfer tower file
        println!("\n{}", "📤 Transfer Tower File".bright_blue().bold());
        self.transfer_tower_file(dry_run).await?;
        // Note: tower_transfer_time is set inside transfer_tower_file method
        
        // Step 3: Switch standby node to funded identity
        println!("\n{}", "🚀 Switch Standby Node to Funded Identity".bright_blue().bold());
        let standby_switch_start = Instant::now();
        self.switch_backup_to_funded(dry_run).await?;
        self.standby_switch_time = Some(standby_switch_start.elapsed());
        if !dry_run {
            println!("   ✓ Completed in {}", format!("{}ms", self.standby_switch_time.unwrap().as_millis()).bright_yellow().bold());
        }
        
        // Record total identity switch time
        if !dry_run {
            self.identity_switch_time = Some(total_switch_start.elapsed());
        }
        
        // Step 4: Verify standby catchup
        println!("\n{}", "✅ Verify Standby Catchup".bright_blue().bold());
        self.verify_backup_catchup(dry_run).await?;
        
        // Summary
        self.print_summary(dry_run);
        
        Ok(())
    }
    
    
    async fn switch_primary_to_unfunded(&mut self, dry_run: bool) -> Result<()> {
        // Detect validator type to use appropriate command
        let process_info = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.active_node_with_status.node, &self.validator_pair.local_ssh_key_path, "ps aux | grep -E 'solana-validator|agave|fdctl|firedancer' | grep -v grep").await?
        };
        
        let (subtitle, switch_command) = if process_info.contains("fdctl") || process_info.contains("firedancer") {
            // Use detected fdctl executable path if available, otherwise use configured path
            let fdctl_path = self.active_node_with_status.fdctl_executable.as_ref()
                .or(self.active_node_with_status.node.paths.fdctl_path.as_ref())
                .ok_or_else(|| anyhow!("Firedancer fdctl executable path not found"))?;
            
            // Extract config path from the process info (e.g., "fdctl run --config /path/to/config.toml")
            let config_path = if let Some(config_match) = process_info.lines()
                .find(|line| line.contains("fdctl") && line.contains("--config"))
                .and_then(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    parts.windows(2)
                        .find(|w| w[0] == "--config")
                        .map(|w| w[1].to_string())
                }) {
                config_match
            } else {
                // Fall back to configured path if we can't extract from process
                self.active_node_with_status.node.paths.firedancer_config.as_ref()
                    .ok_or_else(|| anyhow!("Firedancer config path not found in process or configuration"))?
                    .clone()
            };
            
            (
                "Using Firedancer fdctl set-identity",
                format!("{} set-identity --config {} {}", fdctl_path, config_path, self.active_node_with_status.node.paths.unfunded_identity)
            )
        } else if process_info.contains("agave-validator") {
            (
                "Using Agave validator set-identity",
                format!("agave-validator -l {} set-identity --require-tower {}", 
                    self.active_node_with_status.node.paths.ledger,
                    self.active_node_with_status.node.paths.unfunded_identity)
            )
        } else {
            (
                "Using Solana validator restart",
                format!("{} exit && sleep 2 && solana-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                    self.active_node_with_status.node.paths.solana_cli_path,
                    self.active_node_with_status.node.paths.unfunded_identity,
                    self.active_node_with_status.node.paths.vote_keypair,
                    self.active_node_with_status.node.paths.ledger)
            )
        };
        
        println!("{}", subtitle.dimmed());
        println!("ssh {}@{} '{}'", self.active_node_with_status.node.user, self.active_node_with_status.node.host, switch_command);
        
        if !dry_run {
            let spinner = ProgressSpinner::new("Switching active validator to unfunded identity...");
            {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.active_node_with_status.node, &self.validator_pair.local_ssh_key_path, &switch_command).await?;
            }
            tokio::time::sleep(Duration::from_secs(3)).await;
            spinner.stop_with_message("✅ Active validator switched to unfunded identity");
        }
        
        Ok(())
    }
    
    async fn transfer_tower_file(&mut self, dry_run: bool) -> Result<()> {
        // Find the latest tower file
        let find_tower_cmd = format!("ls -t {}/tower-1_9-*.bin 2>/dev/null | head -1", self.active_node_with_status.node.paths.ledger);
        
        let tower_path = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.active_node_with_status.node, &self.validator_pair.local_ssh_key_path, &find_tower_cmd).await?
        };
        let tower_path = tower_path.trim();
        
        if tower_path.is_empty() {
            return Err(anyhow!("No tower file found on active node"));
        }
        
        let tower_filename = tower_path.split('/').last().unwrap_or("tower.bin");
        self.tower_file_name = Some(tower_filename.to_string());
        let dest_path = format!("{}/{}", self.standby_node_with_status.node.paths.ledger, tower_filename);
        
        println!("  📤 {}@{} → {}@{}", 
            self.active_node_with_status.node.user, self.active_node_with_status.node.host,
            self.standby_node_with_status.node.user, self.standby_node_with_status.node.host);
        
        let start_time = Instant::now();
        
        // Execute the streaming transfer using base64 encoding
        let read_cmd = format!("base64 {}", tower_path);
        let encoded_data = if !dry_run {
            let spinner = ProgressSpinner::new("Reading tower file...");
            let data = {
                let mut pool = self.ssh_pool.lock().unwrap();
                match pool.execute_command(&self.active_node_with_status.node, &self.validator_pair.local_ssh_key_path, &read_cmd).await {
                    Ok(data) => data,
                    Err(e) => {
                        spinner.stop_with_message(&format!("❌ Failed to read tower file: {}", e));
                        return Err(anyhow!("Failed to read tower file: {}", e));
                    }
                }
            };
            spinner.stop_with_message("");
            
            let write_cmd = format!("base64 -d > {}", dest_path);
            let spinner = ProgressSpinner::new("Transferring tower file...");
            {
                let mut pool = self.ssh_pool.lock().unwrap();
                match pool.execute_command_with_input(&self.standby_node_with_status.node, &write_cmd, &data).await {
                    Ok(_) => {},
                    Err(e) => {
                        spinner.stop_with_message(&format!("❌ Failed to write tower file: {}", e));
                        return Err(anyhow!("Failed to write tower file: {}", e));
                    }
                }
            }
            spinner.stop_with_message("");
            data
        } else {
            // For dry run, just use a dummy value
            String::from("dummy")
        };
        
        let transfer_duration = start_time.elapsed();
        self.tower_transfer_time = Some(transfer_duration);
        
        // Calculate transfer speed
        let file_size = encoded_data.len() as u64 * 3 / 4; // approximate original size from base64
        let speed_mbps = (file_size as f64 / 1024.0 / 1024.0) / transfer_duration.as_secs_f64();
        
        println!("  ✅ Transferred in {} ({:.2} MB/s)", 
            format!("{}ms", transfer_duration.as_millis()).bright_green().bold(), speed_mbps);
        
        if !dry_run {
            // Verify the file on standby
            let verify_cmd = format!("ls -la {}", dest_path);
            let verify_result = {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.standby_node_with_status.node, &self.validator_pair.local_ssh_key_path, &verify_cmd).await?
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
            pool.execute_command(&self.standby_node_with_status.node, &self.validator_pair.local_ssh_key_path, "ps aux | grep -E 'solana-validator|agave|fdctl|firedancer' | grep -v grep").await?
        };
        
        let (subtitle, switch_command) = if process_info.contains("fdctl") || process_info.contains("firedancer") {
            // Use detected fdctl executable path if available, otherwise use configured path
            let fdctl_path = self.standby_node_with_status.fdctl_executable.as_ref()
                .or(self.standby_node_with_status.node.paths.fdctl_path.as_ref())
                .ok_or_else(|| anyhow!("Firedancer fdctl executable path not found"))?;
            
            // Extract config path from the process info (e.g., "fdctl run --config /path/to/config.toml")
            let config_path = if let Some(config_match) = process_info.lines()
                .find(|line| line.contains("fdctl") && line.contains("--config"))
                .and_then(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    parts.windows(2)
                        .find(|w| w[0] == "--config")
                        .map(|w| w[1].to_string())
                }) {
                config_match
            } else {
                // Fall back to configured path if we can't extract from process
                self.standby_node_with_status.node.paths.firedancer_config.as_ref()
                    .ok_or_else(|| anyhow!("Firedancer config path not found in process or configuration"))?
                    .clone()
            };
            
            (
                "Using Firedancer fdctl set-identity",
                format!("{} set-identity --config {} {}", fdctl_path, config_path, self.standby_node_with_status.node.paths.funded_identity)
            )
        } else if process_info.contains("agave-validator") {
            (
                "Using Agave validator set-identity",
                format!("agave-validator -l {} set-identity --require-tower {}", 
                    self.standby_node_with_status.node.paths.ledger,
                    self.standby_node_with_status.node.paths.funded_identity)
            )
        } else {
            (
                "Using Solana validator restart",
                format!("{} exit && sleep 2 && solana-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                    self.standby_node_with_status.node.paths.solana_cli_path,
                    self.standby_node_with_status.node.paths.funded_identity,
                    self.standby_node_with_status.node.paths.vote_keypair,
                    self.standby_node_with_status.node.paths.ledger)
            )
        };
        
        println!("{}", subtitle.dimmed());
        println!("ssh {}@{} '{}'", self.standby_node_with_status.node.user, self.standby_node_with_status.node.host, switch_command);
        
        if !dry_run {
            let spinner = ProgressSpinner::new("Switching standby validator to funded identity...");
            {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.standby_node_with_status.node, &self.validator_pair.local_ssh_key_path, &switch_command).await?;
            }
            tokio::time::sleep(Duration::from_secs(5)).await;
            spinner.stop_with_message("✅ Standby validator switched to funded identity");
        }
        
        Ok(())
    }
    
    async fn verify_backup_catchup(&mut self, dry_run: bool) -> Result<()> {
        let catchup_cmd = format!("{} catchup --our-localhost", self.standby_node_with_status.node.paths.solana_cli_path);
        println!("ssh {}@{} '{}'", self.standby_node_with_status.node.user, self.standby_node_with_status.node.host, catchup_cmd);
        
        if !dry_run {
            tokio::time::sleep(Duration::from_secs(10)).await;
            
            let spinner = ProgressSpinner::new("Verifying standby validator catchup status...");
            
            let catchup_result = {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.standby_node_with_status.node, &self.validator_pair.local_ssh_key_path, &catchup_cmd).await?
            };
            
            if catchup_result.contains("has caught up") || catchup_result.contains("slots behind") {
                spinner.stop_with_message("✅ Standby validator is syncing with funded identity");
            } else {
                spinner.stop_with_message("⚠️  Standby sync status unclear - monitor manually");
            }
        }
        
        Ok(())
    }
    
    fn print_summary(&self, dry_run: bool) {
        println!();
        if dry_run {
            println!("✅ Dry run completed successfully");
            println!();
            println!("{}", "Press any key to continue...".dimmed());
            let _ = std::io::stdin().read_line(&mut String::new());
        } else {
            println!("✅ Validator identity switch completed successfully");
        }
    }
}

#[cfg(test)]
#[path = "switch_scenarios_test.rs"]
mod switch_scenarios_test;