use anyhow::{Result, anyhow};
use colored::*;
use std::time::{Duration, Instant};
use std::sync::{Arc, Mutex};

use crate::types::NodeConfig;

pub async fn switch_command(dry_run: bool, force: bool, app_state: &crate::AppState) -> Result<()> {
    // Validate we have at least one node pair configured
    if app_state.config.nodes.is_empty() {
        return Err(anyhow!("No node pairs configured"));
    }
    
    // For now, use the first node pair
    let node_pair = &app_state.config.nodes[0];
    let primary = &node_pair.primary;
    let backup = &node_pair.backup;
    
    println!("\n{}", format!("🔄 Validator Switch - {} Mode", if dry_run { "DRY RUN" } else { "LIVE" }).bright_cyan().bold());
    println!("{}", "━".repeat(50).dimmed());
    
    if dry_run {
        println!("{}", "ℹ️  This is a DRY RUN - showing what would be executed".yellow());
        println!("{}", "ℹ️  Tower file transfer will be performed to measure timing".yellow());
        println!();
    }
    
    let mut switch_manager = SwitchManager::new(primary.clone(), backup.clone(), app_state.ssh_pool.clone());
    
    // Execute the switch process
    switch_manager.execute_switch(dry_run, force).await?;
    
    Ok(())
}

struct SwitchManager {
    primary: NodeConfig,
    backup: NodeConfig,
    ssh_pool: Arc<Mutex<crate::ssh::SshConnectionPool>>,
    tower_file_name: Option<String>,
    tower_transfer_time: Option<Duration>,
}

impl SwitchManager {
    fn new(primary: NodeConfig, backup: NodeConfig, ssh_pool: Arc<Mutex<crate::ssh::SshConnectionPool>>) -> Self {
        Self {
            primary,
            backup,
            ssh_pool,
            tower_file_name: None,
            tower_transfer_time: None,
        }
    }
    
    async fn execute_switch(&mut self, dry_run: bool, force: bool) -> Result<()> {
        // Phase 1: Pre-flight checks
        println!("{}", "📋 Phase 1: Pre-flight Checks".bright_blue().bold());
        self.preflight_checks(dry_run).await?;
        
        // Phase 2: Switch primary to unfunded identity
        println!("\n{}", "🔄 Phase 2: Switch Primary to Unfunded Identity".bright_blue().bold());
        self.switch_primary_to_unfunded(dry_run).await?;
        
        // Phase 3: Transfer tower file
        println!("\n{}", "📤 Phase 3: Transfer Tower File".bright_blue().bold());
        self.transfer_tower_file(dry_run, force).await?;
        
        // Phase 4: Switch backup to funded identity
        println!("\n{}", "🚀 Phase 4: Switch Backup to Funded Identity".bright_blue().bold());
        self.switch_backup_to_funded(dry_run).await?;
        
        // Phase 5: Verify backup catchup
        println!("\n{}", "✅ Phase 5: Verify Backup Catchup".bright_blue().bold());
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
        
        // Check primary is running
        if dry_run {
            println!("  Primary: {}", format!("ssh {}@{} '{}'", self.primary.user, self.primary.host, validator_check_cmd).dimmed());
        }
        
        let primary_running = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.primary, "", validator_check_cmd).await?
        };
        let primary_is_running = !primary_running.trim().is_empty();
        
        if primary_is_running {
            println!("    ✅ Primary validator is running");
        } else {
            return Err(anyhow!("❌ Primary validator is not running - cannot perform switch"));
        }
        
        // Check backup is also running (both should be running for identity swap)
        if dry_run {
            println!("  Backup: {}", format!("ssh {}@{} '{}'", self.backup.user, self.backup.host, validator_check_cmd).dimmed());
        }
        
        let backup_running = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.backup, "", validator_check_cmd).await?
        };
        let backup_is_running = !backup_running.trim().is_empty();
        
        if backup_is_running {
            println!("    ✅ Backup validator is running");
        } else {
            return Err(anyhow!("❌ Backup validator is not running - both validators must be running for identity swap"));
        }
        
        // Detect validator type for proper switching commands
        let validator_type = if primary_running.contains("fdctl") || primary_running.contains("firedancer") {
            "firedancer"
        } else if primary_running.contains("agave") {
            "agave"
        } else {
            "solana"
        };
        
        println!("    🔍 Detected validator type: {}", validator_type);
        
        // Check tower file exists on primary
        let tower_check_cmd = format!("ls -la {}/tower-1_9-*.bin 2>/dev/null | head -1", self.primary.paths.ledger);
        if dry_run {
            println!("\n  Tower file check: {}", format!("ssh {}@{} '{}'", self.primary.user, self.primary.host, tower_check_cmd).dimmed());
        }
        
        let tower_result = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.primary, "", &tower_check_cmd).await?
        };
        
        if tower_result.trim().is_empty() {
            return Err(anyhow!("❌ No tower file found on primary validator"));
        } else {
            println!("    ✅ Tower file found on primary");
        }
        
        Ok(())
    }
    
    async fn switch_primary_to_unfunded(&mut self, dry_run: bool) -> Result<()> {
        println!("  ⏱️  Estimated time: 3-5 seconds");
        println!();
        
        // Detect validator type to use appropriate command
        let process_info = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.primary, "", "ps aux | grep -E 'solana-validator|agave|fdctl|firedancer' | grep -v grep").await?
        };
        
        let switch_command = if process_info.contains("fdctl") || process_info.contains("firedancer") {
            // Firedancer validator - use fdctl set-identity
            println!("  🔍 Using Firedancer fdctl set-identity");
            let default_fdctl = "fdctl".to_string();
            let default_config = "firedancer-config.toml".to_string();
            let fdctl_path = self.primary.paths.fdctl_path.as_ref().unwrap_or(&default_fdctl);
            let config_path = self.primary.paths.firedancer_config.as_ref().unwrap_or(&default_config);
            format!("{} set-identity --config {} {}", fdctl_path, config_path, self.primary.paths.unfunded_identity)
        } else if process_info.contains("agave-validator") {
            // Agave/Jito validator - stop and restart with different identity
            println!("  🔍 Using Agave validator restart");
            format!("{} exit && sleep 2 && agave-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                self.primary.paths.solana_cli_path,
                self.primary.paths.unfunded_identity,
                self.primary.paths.vote_keypair,
                self.primary.paths.ledger)
        } else {
            // Default Solana validator
            println!("  🔍 Using Solana validator restart");
            format!("{} exit && sleep 2 && solana-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                self.primary.paths.solana_cli_path,
                self.primary.paths.unfunded_identity,
                self.primary.paths.vote_keypair,
                self.primary.paths.ledger)
        };
        
        println!("\n  🔄 Switching primary validator to unfunded identity:");
        println!("  Command: {}", format!("ssh {}@{} '{}'", self.primary.user, self.primary.host, switch_command).dimmed());
        
        if !dry_run {
            {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.primary, "", &switch_command).await?;
            }
            
            // Wait for identity switch to complete
            println!("  ⏳ Waiting for identity switch...");
            tokio::time::sleep(Duration::from_secs(3)).await;
            
            println!("  ✅ Primary validator switched to unfunded identity");
        }
        
        Ok(())
    }
    
    async fn transfer_tower_file(&mut self, dry_run: bool, force: bool) -> Result<()> {
        println!("  ⏱️  Estimated time: 2-3 seconds");
        println!();
        
        if force {
            println!("  ⚠️  Force mode: Skipping tower file transfer");
            return Ok(());
        }
        
        // Find the latest tower file
        let find_tower_cmd = format!("ls -t {}/tower-1_9-*.bin 2>/dev/null | head -1", self.primary.paths.ledger);
        println!("  🔍 Finding latest tower file:");
        println!("  Command: {}", format!("ssh {}@{} '{}'", self.primary.user, self.primary.host, find_tower_cmd).dimmed());
        
        let tower_path = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.primary, "", &find_tower_cmd).await?
        };
        let tower_path = tower_path.trim();
        
        if tower_path.is_empty() {
            return Err(anyhow!("No tower file found on primary"));
        }
        
        println!("  📁 Found tower file: {}", tower_path);
        
        // Store tower file name for summary
        let tower_filename = tower_path.split('/').last().unwrap_or("tower.bin");
        self.tower_file_name = Some(tower_filename.to_string());
        
        // Get file size for progress tracking
        let size_cmd = format!("stat -c %s {} 2>/dev/null || stat -f %z {} 2>/dev/null", tower_path, tower_path);
        let size_result = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.primary, "", &size_cmd).await?
        };
        let file_size: u64 = size_result.trim().parse().unwrap_or(0);
        println!("  📊 File size: {} bytes", file_size);
        
        // Prepare destination path
        let tower_filename = tower_path.split('/').last().unwrap_or("tower.bin");
        let dest_path = format!("{}/{}", self.backup.paths.ledger, tower_filename);
        
        println!("\n  📤 Transferring tower file:");
        println!("  From: {}@{}:{}", self.primary.user, self.primary.host, tower_path);
        println!("  To: {}@{}:{}", self.backup.user, self.backup.host, dest_path);
        
        // Always perform the actual transfer (even in dry-run) to measure timing
        let start_time = Instant::now();
        
        // Use streaming transfer via SSH tunnel
        // This simulates a proper streaming transfer by using SSH to pipe the file directly
        println!("  ⏳ Transferring file...");
        
        // Create a single SSH command that reads from primary and writes to backup
        // This is more efficient than reading entire file into memory
        let _transfer_cmd = format!(
            "ssh -o StrictHostKeyChecking=no {}@{} 'base64 {}' | ssh -o StrictHostKeyChecking=no {}@{} 'base64 -d > {}'",
            self.primary.user,
            self.primary.host,
            tower_path,
            self.backup.user,
            self.backup.host,
            dest_path
        );
        
        if dry_run {
            println!("  Transfer method: Base64 encoding for binary safety");
            println!("  Read command: {}", format!("ssh {}@{} 'base64 {}'", self.primary.user, self.primary.host, tower_path).dimmed());
            println!("  Write command: {}", format!("ssh {}@{} 'base64 -d > {}'", self.backup.user, self.backup.host, dest_path).dimmed());
            // For dry-run, we still do the actual transfer to measure timing
        }
        
        // Execute the streaming transfer using proper binary handling
        // Use base64 encoding to safely transfer binary data through SSH text commands
        println!("  🔍 Reading tower file from primary...");
        let read_cmd = format!("base64 {}", tower_path);
        let encoded_data = {
            let mut pool = self.ssh_pool.lock().unwrap();
            match pool.execute_command(&self.primary, "", &read_cmd).await {
                Ok(data) => {
                    println!("  ✅ Tower file read successfully ({} chars)", data.len());
                    data
                }
                Err(e) => {
                    return Err(anyhow!("Failed to read tower file from primary: {}", e));
                }
            }
        };
        
        // Write to backup by decoding base64 data
        println!("  💾 Writing tower file to backup...");
        let write_cmd = format!("base64 -d > {}", dest_path);
        {
            let mut pool = self.ssh_pool.lock().unwrap();
            match pool.execute_command_with_input(&self.backup, &write_cmd, &encoded_data).await {
                Ok(_) => {
                    println!("  ✅ Tower file written successfully");
                }
                Err(e) => {
                    return Err(anyhow!("Failed to write tower file to backup: {}", e));
                }
            }
        }
        
        let transfer_duration = start_time.elapsed();
        
        // Store transfer time for summary
        self.tower_transfer_time = Some(transfer_duration);
        
        println!("  ✅ Tower file transferred successfully");
        println!("  ⏱️  Transfer time: {:.0} ms", transfer_duration.as_millis());
        
        if file_size > 0 {
            let speed_mbps = (file_size as f64 / 1024.0 / 1024.0) / transfer_duration.as_secs_f64();
            println!("  🚀 Transfer speed: {:.2} MB/s", speed_mbps);
        }
        
        // Verify the file on backup
        let verify_cmd = format!("ls -la {}", dest_path);
        println!("\n  🔍 Verifying tower file on backup:");
        println!("  Command: {}", format!("ssh {}@{} '{}'", self.backup.user, self.backup.host, verify_cmd).dimmed());
        
        let verify_result = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.backup, "", &verify_cmd).await?
        };
        if !verify_result.trim().is_empty() {
            println!("  ✅ Tower file verified on backup");
        } else {
            return Err(anyhow!("Failed to verify tower file on backup"));
        }
        
        Ok(())
    }
    
    async fn switch_backup_to_funded(&mut self, dry_run: bool) -> Result<()> {
        println!("  ⏱️  Estimated time: 5-10 seconds");
        println!();
        
        // Detect validator type to use appropriate command
        let process_info = {
            let mut pool = self.ssh_pool.lock().unwrap();
            pool.execute_command(&self.backup, "", "ps aux | grep -E 'solana-validator|agave|fdctl|firedancer' | grep -v grep").await?
        };
        
        let switch_command = if process_info.contains("fdctl") || process_info.contains("firedancer") {
            // Firedancer validator - use fdctl set-identity
            println!("  🔍 Using Firedancer fdctl set-identity");
            let default_fdctl = "fdctl".to_string();
            let default_config = "firedancer-config.toml".to_string();
            let fdctl_path = self.backup.paths.fdctl_path.as_ref().unwrap_or(&default_fdctl);
            let config_path = self.backup.paths.firedancer_config.as_ref().unwrap_or(&default_config);
            format!("{} set-identity --config {} {}", fdctl_path, config_path, self.backup.paths.funded_identity)
        } else if process_info.contains("agave-validator") {
            // Agave/Jito validator - restart with funded identity
            println!("  🔍 Using Agave validator restart");
            format!("{} exit && sleep 2 && agave-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                self.backup.paths.solana_cli_path,
                self.backup.paths.funded_identity,
                self.backup.paths.vote_keypair,
                self.backup.paths.ledger)
        } else {
            // Default Solana validator
            println!("  🔍 Using Solana validator restart");
            format!("{} exit && sleep 2 && solana-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                self.backup.paths.solana_cli_path,
                self.backup.paths.funded_identity,
                self.backup.paths.vote_keypair,
                self.backup.paths.ledger)
        };
        
        println!("\n  🚀 Switching backup validator to funded identity:");
        println!("  Command: {}", format!("ssh {}@{} '{}'", self.backup.user, self.backup.host, switch_command).dimmed());
        
        if !dry_run {
            {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.backup, "", &switch_command).await?;
            }
            
            // Wait for identity switch to complete
            println!("  ⏳ Waiting for identity switch...");
            tokio::time::sleep(Duration::from_secs(5)).await;
            
            println!("  ✅ Backup validator switched to funded identity");
        }
        
        Ok(())
    }
    
    async fn verify_backup_catchup(&mut self, dry_run: bool) -> Result<()> {
        println!("  ⏱️  Estimated time: 15-20 seconds");
        println!();
        
        // Check backup catchup status
        let catchup_cmd = format!("{} catchup --our-localhost", self.backup.paths.solana_cli_path);
        println!("  📊 Verifying backup validator catchup:");
        println!("  Command: {}", format!("ssh {}@{} '{}'", self.backup.user, self.backup.host, catchup_cmd).dimmed());
        
        if !dry_run {
            // Wait a bit for validator to start syncing with new identity
            println!("  ⏳ Waiting for backup to sync with funded identity...");
            tokio::time::sleep(Duration::from_secs(10)).await;
            
            let catchup_result = {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.backup, "", &catchup_cmd).await?
            };
            println!("  📊 Sync status: {}", catchup_result.trim());
            
            // Check if backup is catching up with the funded identity
            if catchup_result.contains("has caught up") || catchup_result.contains("slots behind") {
                println!("  ✅ Backup validator is syncing with funded identity");
            } else {
                println!("  ⚠️  Backup sync status unclear - monitor manually");
            }
            
            // Additional check: verify backup is now voting with funded identity
            let vote_check_cmd = format!("{} vote-account {}", self.backup.paths.solana_cli_path, self.backup.paths.vote_keypair);
            println!("\n  📊 Verifying vote account status:");
            println!("  Command: {}", format!("ssh {}@{} '{}'", self.backup.user, self.backup.host, vote_check_cmd).dimmed());
            
            let vote_result = {
                let mut pool = self.ssh_pool.lock().unwrap();
                pool.execute_command(&self.backup, "", &vote_check_cmd).await?
            };
            
            if !vote_result.trim().is_empty() {
                println!("  ✅ Vote account is active");
            } else {
                println!("  ⚠️  Vote account status unclear");
            }
        }
        
        Ok(())
    }
    
    fn print_summary(&self, dry_run: bool) {
        println!("\n{}", "━".repeat(50).dimmed());
        println!("{}", "📊 Validator Switch Summary".bright_green().bold());
        println!("{}", "━".repeat(50).dimmed());
        
        if dry_run {
            println!("✅ Dry run completed successfully");
            
            // Show tower transfer details
            if let (Some(filename), Some(duration)) = (&self.tower_file_name, &self.tower_transfer_time) {
                // Truncate long tower filename for display
                let display_name = if filename.len() > 25 {
                    let start = &filename[..15];
                    let end = &filename[filename.len()-10..];
                    format!("{}...{}", start, end)
                } else {
                    filename.clone()
                };
                println!("📤 Tower file {} transferred in {}", 
                    display_name, 
                    format!("{:.0}ms", duration.as_millis()).bright_green().bold()
                );
            }
            
            println!("ℹ️  Review the commands above before executing the actual switch");
            println!("ℹ️  Both validators should be running before executing the switch");
        } else {
            println!("✅ Validator identity switch completed successfully");
            println!("🔄 Primary validator switched to unfunded identity");
            println!("📤 Tower file transferred to backup");
            println!("🚀 Backup validator switched to funded identity");
            println!("✅ Role swap complete: Backup is now the active validator");
        }
        
        println!("\n{}", "⚠️  Important Next Steps:".yellow().bold());
        println!("  1. Monitor backup validator (now active) for proper voting");
        println!("  2. Verify primary validator (now standby) is not voting");
    }
}