use anyhow::Result;
use colored::*;
use indicatif::{ProgressBar, ProgressStyle, MultiProgress};
use std::time::Duration;
use std::collections::HashMap;

use crate::config::ConfigManager;
use crate::ssh::SshManager;
use crate::types::{NodeConfig, Config};

pub async fn status_command() -> Result<()> {
    let config_manager = ConfigManager::new()?;
    let config = config_manager.load()?;
    
    if config.nodes.is_empty() {
        println!("{}", "⚠️ No nodes configured. Run setup first.".yellow());
        return Ok(());
    }
    
    // Show comprehensive status in one clean view
    show_comprehensive_status(&config).await
}

async fn show_comprehensive_status(config: &Config) -> Result<()> {
    println!("\n{}", "📋 Validator Status".bright_cyan().bold());
    println!();
    
    let m = MultiProgress::new();
    let mut handles = Vec::new();
    
    // Create progress bars for each node
    for (role, _) in &config.nodes {
        let pb = m.add(ProgressBar::new_spinner());
        pb.set_style(ProgressStyle::default_spinner()
            .template(&format!("{{spinner:.green}} Checking {} validator...", role))?);
        pb.enable_steady_tick(Duration::from_millis(100));
        handles.push((role.clone(), pb));
    }
    
    // Check each node
    let mut results = HashMap::new();
    
    for (role, node) in &config.nodes {
        let pb = handles.iter().find(|(r, _)| r == role).unwrap().1.clone();
        
        let mut ssh = SshManager::new();
        match ssh.connect(node, &config.ssh.key_path).await {
            Ok(_) => {
                let status = check_comprehensive_status(&mut ssh, node).await?;
                results.insert(role.clone(), status);
                pb.finish_with_message(format!("✅ {} validator checked", role));
            }
            Err(e) => {
                pb.finish_with_message(format!("❌ {} connection failed", role));
                results.insert(role.clone(), ComprehensiveStatus::connection_failed(e.to_string()));
            }
        }
    }
    
    m.clear()?;
    
    // Display results in clean table
    display_status_table(&config, &results);
    
    Ok(())
}

async fn check_comprehensive_status(ssh: &mut SshManager, node: &NodeConfig) -> Result<ComprehensiveStatus> {
    let mut status = ComprehensiveStatus {
        connected: true,
        validator_running: None,
        ledger_disk_usage: None,
        system_load: None,
        sync_status: None,
        version: None,
        swap_ready: None,
        swap_issues: Vec::new(),
        swap_checklist: Vec::new(),
        error: None,
    };
    
    // Check if validator process is running
    match ssh.execute_command("ps aux | grep -Ei 'solana-validator|agave|fdctl|firedancer'").await {
        Ok(output) => {
            // Filter out grep process itself and check if any real validator processes exist
            let validator_processes: Vec<&str> = output
                .lines()
                .filter(|line| !line.contains("grep"))
                .filter(|line| line.contains("solana-validator") || 
                              line.contains("agave") || 
                              line.contains("fdctl") || 
                              line.contains("firedancer"))
                .collect();
            status.validator_running = Some(!validator_processes.is_empty());
        }
        Err(_) => {
            status.validator_running = Some(false);
        }
    }
    
    // Check ledger disk usage
    match ssh.execute_command(&format!("df {} | tail -1 | awk '{{print $5}}' | sed 's/%//'", node.paths.ledger)).await {
        Ok(output) => {
            if let Ok(usage) = output.trim().parse::<u32>() {
                status.ledger_disk_usage = Some(usage);
            }
        }
        Err(_) => {}
    }
    
    // Check system load
    match ssh.execute_command("uptime | awk -F'load average:' '{print $2}' | awk '{print $1}' | sed 's/,//'").await {
        Ok(output) => {
            if let Ok(load) = output.trim().parse::<f64>() {
                status.system_load = Some(load);
            }
        }
        Err(_) => {}
    }
    
    // Check sync status using solana catchup
    let solana_cli = &node.paths.solana_cli_path;
    match ssh.execute_command(&format!("{} catchup --our-localhost", solana_cli)).await {
        Ok(output) => {
            if output.contains("behind") {
                status.sync_status = Some("Behind".to_string());
            } else {
                status.sync_status = Some("In Sync".to_string());
            }
        }
        Err(_) => {
            status.sync_status = Some("Unknown".to_string());
        }
    }
    
    // Check version by detecting validator type from process list
    status.version = detect_validator_version(ssh).await;
    
    // Check swap readiness
    let (swap_ready, swap_issues, swap_checklist) = check_swap_readiness(ssh, node).await;
    status.swap_ready = Some(swap_ready);
    status.swap_issues = swap_issues;
    status.swap_checklist = swap_checklist;
    
    Ok(status)
}

async fn check_swap_readiness(ssh: &mut SshManager, node: &NodeConfig) -> (bool, Vec<String>, Vec<(String, bool)>) {
    let mut issues = Vec::new();
    let mut checklist = Vec::new();
    let mut all_ready = true;
    
    // Check critical files exist
    let critical_files = vec![
        (&node.paths.funded_identity, "Funded Identity"),
        (&node.paths.unfunded_identity, "Unfunded Identity"),
        (&node.paths.vote_keypair, "Vote Keypair"),
        (&node.paths.tower, "Tower File"),
    ];
    
    for (path, description) in critical_files {
        match ssh.execute_command(&format!("test -f {}", path)).await {
            Ok(_) => {
                // File exists, now check if it's readable
                match ssh.execute_command(&format!("test -r {}", path)).await {
                    Ok(_) => {
                        checklist.push((description.to_string(), true));
                    }
                    Err(_) => {
                        checklist.push((description.to_string(), false));
                        issues.push(format!("{} exists but not readable: {}", description, path));
                        all_ready = false;
                    }
                }
            }
            Err(_) => {
                checklist.push((description.to_string(), false));
                issues.push(format!("{} missing: {}", description, path));
                all_ready = false;
            }
        }
    }
    
    // Check critical directories exist and are writable
    match ssh.execute_command(&format!("test -d {}", node.paths.ledger)).await {
        Ok(_) => {
            // Directory exists, check if writable
            match ssh.execute_command(&format!("test -w {}", node.paths.ledger)).await {
                Ok(_) => {
                    checklist.push(("Ledger Directory".to_string(), true));
                }
                Err(_) => {
                    checklist.push(("Ledger Directory".to_string(), false));
                    issues.push(format!("Ledger directory exists but not writable: {}", node.paths.ledger));
                    all_ready = false;
                }
            }
        }
        Err(_) => {
            checklist.push(("Ledger Directory".to_string(), false));
            issues.push(format!("Ledger directory missing: {}", node.paths.ledger));
            all_ready = false;
        }
    }
    
    // Check disk space for ledger (should have at least 10GB free)
    let disk_space_ok = if let Ok(output) = ssh.execute_command(&format!("df {} | tail -1 | awk '{{print $4}}'", node.paths.ledger)).await {
        if let Ok(free_kb) = output.trim().parse::<u64>() {
            let free_gb = free_kb / 1024 / 1024;
            if free_gb < 10 {
                issues.push(format!("Low disk space in ledger directory: {}GB free (minimum 10GB)", free_gb));
                all_ready = false;
                false
            } else {
                true
            }
        } else {
            false
        }
    } else {
        false
    };
    checklist.push(("Disk Space (>10GB)".to_string(), disk_space_ok));
    
    // Check if solana CLI is accessible
    let cli_ok = ssh.execute_command(&format!("test -x {}", node.paths.solana_cli_path)).await.is_ok();
    if !cli_ok {
        issues.push(format!("Solana CLI not executable: {}", node.paths.solana_cli_path));
        all_ready = false;
    }
    checklist.push(("Solana CLI".to_string(), cli_ok));
    
    (all_ready, issues, checklist)
}

async fn detect_validator_version(ssh: &mut SshManager) -> Option<String> {
    // Get process list to detect validator type
    let ps_output = ssh.execute_command("ps aux | grep -Ei 'solana-validator|agave|fdctl|firedancer'").await.ok()?;
    
    // Filter out grep process itself and find validator processes
    let validator_processes: Vec<&str> = ps_output
        .lines()
        .filter(|line| !line.contains("grep"))
        .filter(|line| line.contains("solana-validator") || 
                      line.contains("agave") || 
                      line.contains("fdctl") || 
                      line.contains("firedancer"))
        .collect();
    
    if validator_processes.is_empty() {
        return None;
    }
    
    // Find the process with the exact patterns you specified
    let process_line = validator_processes.iter()
        .find(|line| {
            line.contains("build/native/gcc/bin/fdctl") || 
            line.contains("target/release/agave-validator")
        })?;
    
    // Look for executable path in the process line
    let mut executable_path = None;
    
    // Split by whitespace and look for paths containing validator executables
    for part in process_line.split_whitespace() {
        if part.contains("build/native/gcc/bin/fdctl") || 
           part.contains("target/release/agave-validator") {
            executable_path = Some(part);
            break;
        }
    }
    
    let executable_path = executable_path?;
    
    // Detect validator type and get version based on path patterns
    if executable_path.contains("build/native/gcc/bin/fdctl") {
        // Firedancer
        get_firedancer_version(ssh, executable_path).await
    } else if executable_path.contains("target/release/agave-validator") {
        // Jito or Agave
        get_jito_agave_version(ssh, executable_path).await
    } else {
        None
    }
}

async fn get_firedancer_version(ssh: &mut SshManager, executable_path: &str) -> Option<String> {
    let version_output = ssh.execute_command(&format!("{} --version", executable_path)).await.ok()?;
    
    // Parse firedancer version format: "0.505.20216 (44f9f393d167138abe1c819f7424990a56e1913e)"
    for line in version_output.lines() {
        if line.contains('.') && (line.contains('(') || line.chars().any(|c| c.is_ascii_digit())) {
            // Extract just the version number part
            let version_part = line.trim()
                .split_whitespace()
                .next()
                .unwrap_or(line.trim());
            return Some(format!("Firedancer {}", version_part));
        }
    }
    
    None
}

async fn get_jito_agave_version(ssh: &mut SshManager, executable_path: &str) -> Option<String> {
    // Try the executable path first
    if let Ok(version_output) = ssh.execute_command(&format!("{} --version", executable_path)).await {
        if let Some(version_line) = version_output.lines().next() {
            let version_line = version_line.trim();
            if !version_line.is_empty() {
                return Some(parse_agave_version(version_line));
            }
        }
    }
    
    // Fallback to standard commands
    if let Ok(version_output) = ssh.execute_command("agave-validator --version").await {
        if let Some(version_line) = version_output.lines().next() {
            let version_line = version_line.trim();
            if !version_line.is_empty() {
                return Some(parse_agave_version(version_line));
            }
        }
    }
    
    // Final fallback
    if let Ok(version_output) = ssh.execute_command("solana-validator --version").await {
        if let Some(version_line) = version_output.lines().next() {
            let version_line = version_line.trim();
            if !version_line.is_empty() {
                return Some(version_line.to_string());
            }
        }
    }
    
    None
}

fn parse_agave_version(version_line: &str) -> String {
    // Parse version format examples:
    // Jito: "agave-validator 2.2.16 (src:00000000; feat:3073396398, client:JitoLabs)"
    // Agave: "agave-validator 2.1.5 (src:4da190bd; feat:288566304, client:Agave)"
    
    if version_line.contains("client:JitoLabs") {
        // Extract version number and mark as Jito
        if let Some(version_part) = version_line.split_whitespace().nth(1) {
            format!("Jito {}", version_part)
        } else {
            "Jito".to_string()
        }
    } else if version_line.contains("client:Agave") {
        // Regular Agave - extract version number
        if let Some(version_part) = version_line.split_whitespace().nth(1) {
            format!("Agave {}", version_part)
        } else {
            "Agave".to_string()
        }
    } else if version_line.contains("agave-validator") {
        // Agave without client field - extract version number
        if let Some(version_part) = version_line.split_whitespace().nth(1) {
            format!("Agave {}", version_part)
        } else {
            "Agave".to_string()
        }
    } else {
        // Fallback
        version_line.to_string()
    }
}

async fn get_solana_validator_version(ssh: &mut SshManager, executable_path: &str) -> Option<String> {
    let version_output = ssh.execute_command(&format!("{} --version", executable_path)).await.ok()?;
    let version_line = version_output.lines().next()?.trim();
    Some(version_line.to_string())
}

fn display_status_table(config: &Config, results: &HashMap<String, ComprehensiveStatus>) {
    println!("\n{}", "📋 Validator Status Summary".bright_cyan().bold());
    println!();
    
    // Find primary and backup nodes
    let primary_status = results.get("primary");
    let backup_status = results.get("backup");
    
    let primary_node = config.nodes.get("primary");
    let backup_node = config.nodes.get("backup");
    
    // Display primary and backup side by side
    if let (Some(primary), Some(backup)) = (primary_status, backup_status) {
        display_side_by_side_comparison(primary_node, primary, backup_node, backup);
    } else {
        // Fallback to single column if we don't have both primary and backup
        display_single_column_view(config, results);
    }
    
    // Display any other nodes that aren't primary or backup
    let other_nodes: Vec<_> = results.iter()
        .filter(|(role, _)| *role != "primary" && *role != "backup")
        .collect();
    
    if !other_nodes.is_empty() {
        println!("\n{}", "🔧 Other Validators".bright_cyan().bold());
        println!("{}", "─".repeat(150));
        println!("{:<35} {:<15} {:<15} {:<12} {:<10} {:<15} {:<25} {:<15}", 
            "Node".bright_cyan(), 
            "Connection".bright_cyan(), 
            "Process".bright_cyan(), 
            "Disk".bright_cyan(),
            "Load".bright_cyan(),
            "Sync".bright_cyan(),
            "Version".bright_cyan(),
            "Swap Ready".bright_cyan()
        );
        println!("{}", "─".repeat(150));
        
        for (role, status) in other_nodes {
            display_node_row(config, role, status);
        }
        
        println!("{}", "─".repeat(150));
    }
    
    println!();
}

fn display_side_by_side_comparison(
    primary_node: Option<&NodeConfig>, 
    primary_status: &ComprehensiveStatus,
    backup_node: Option<&NodeConfig>,
    backup_status: &ComprehensiveStatus
) {
    // Header
    println!("{}", "┌─ PRIMARY ─────────────────────────────────────────────────┬─ BACKUP ──────────────────────────────────────────────────┐".bright_cyan());
    
    // Node names with IPs - pad manually to account for emoji width
    let primary_label = primary_node.map(|n| format!("{} ({})", n.label, n.host)).unwrap_or("Primary".to_string());
    let backup_label = backup_node.map(|n| format!("{} ({})", n.label, n.host)).unwrap_or("Backup".to_string());
    
    let primary_text = format!("🖥️  {}", primary_label);
    let backup_text = format!("🖥️  {}", backup_label);
    
    // Pad manually since emojis mess up alignment
    let primary_padded = format!("{}{}", primary_text, " ".repeat(57_usize.saturating_sub(primary_text.chars().count() + 1))); // +1 for emoji width
    let backup_padded = format!("{}{}", backup_text, " ".repeat(57_usize.saturating_sub(backup_text.chars().count() + 1)));
    
    println!("│ {} │ {} │", 
        primary_padded.bright_green(),
        backup_padded.bright_yellow()
    );
    
    println!("├───────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────┤");
    
    // Helper function to pad text accounting for emoji width
    let pad_text = |text: &str, width: usize| -> String {
        let emoji_count = text.chars().filter(|c| *c as u32 > 127).count();
        let effective_width = width.saturating_sub(emoji_count);
        format!("{}{}", text, " ".repeat(effective_width.saturating_sub(text.chars().count().saturating_sub(emoji_count))))
    };
    
    // Connection status
    let primary_conn = format_connection_status_plain(primary_status);
    let backup_conn = format_connection_status_plain(backup_status);
    println!("│ Connection: {} │ Connection: {} │", 
        pad_text(&primary_conn, 46), 
        pad_text(&backup_conn, 46));
    
    // Process status
    let primary_proc = format_process_status_plain(primary_status);
    let backup_proc = format_process_status_plain(backup_status);
    println!("│ Process:    {} │ Process:    {} │", 
        pad_text(&primary_proc, 46), 
        pad_text(&backup_proc, 46));
    
    // Disk usage
    let primary_disk = format_disk_usage_plain(primary_status);
    let backup_disk = format_disk_usage_plain(backup_status);
    println!("│ Disk Usage: {} │ Disk Usage: {} │", 
        pad_text(&primary_disk, 46), 
        pad_text(&backup_disk, 46));
    
    // System load
    let primary_load = format_system_load_plain(primary_status);
    let backup_load = format_system_load_plain(backup_status);
    println!("│ System Load:{} │ System Load:{} │", 
        pad_text(&primary_load, 46), 
        pad_text(&backup_load, 46));
    
    // Sync status
    let primary_sync = format_sync_status_plain(primary_status);
    let backup_sync = format_sync_status_plain(backup_status);
    println!("│ Sync Status:{} │ Sync Status:{} │", 
        pad_text(&primary_sync, 46), 
        pad_text(&backup_sync, 46));
    
    // Version
    let primary_version = format_version_plain(primary_status);
    let backup_version = format_version_plain(backup_status);
    println!("│ Version:    {} │ Version:    {} │", 
        pad_text(&primary_version, 46), 
        pad_text(&backup_version, 46));
    
    // Swap readiness with detailed checklist
    let primary_swap = format_swap_readiness_plain(primary_status);
    let backup_swap = format_swap_readiness_plain(backup_status);
    println!("│ Swap Ready: {} │ Swap Ready: {} │", 
        pad_text(&primary_swap, 46), 
        pad_text(&backup_swap, 46));
    
    // Show detailed swap checklist
    println!("├───────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────┤");
    
    let primary_checklist = format_swap_checklist(primary_status);
    let backup_checklist = format_swap_checklist(backup_status);
    
    let max_lines = primary_checklist.len().max(backup_checklist.len());
    let empty_string = String::new();
    for i in 0..max_lines {
        let primary_item = primary_checklist.get(i).unwrap_or(&empty_string);
        let backup_item = backup_checklist.get(i).unwrap_or(&empty_string);
        println!("│ {} │ {} │", 
            pad_text(primary_item, 57), 
            pad_text(backup_item, 57));
    }
    
    // Errors if any
    if primary_status.error.is_some() || backup_status.error.is_some() || 
       !primary_status.swap_issues.is_empty() || !backup_status.swap_issues.is_empty() {
        println!("├───────────────────────────────────────────────────────────┼───────────────────────────────────────────────────────────┤");
        
        // Show connection errors
        if primary_status.error.is_some() || backup_status.error.is_some() {
            let primary_error = primary_status.error.as_ref().map(|e| e.clone()).unwrap_or("".to_string());
            let backup_error = backup_status.error.as_ref().map(|e| e.clone()).unwrap_or("".to_string());
            println!("│ {:<57} │ {:<57} │", primary_error, backup_error);
        }
        
        // Show swap issues
        if !primary_status.swap_issues.is_empty() || !backup_status.swap_issues.is_empty() {
            let max_issues = primary_status.swap_issues.len().max(backup_status.swap_issues.len());
            for i in 0..max_issues {
                let primary_issue = primary_status.swap_issues.get(i).map(|s| s.as_str()).unwrap_or("");
                let backup_issue = backup_status.swap_issues.get(i).map(|s| s.as_str()).unwrap_or("");
                println!("│ {:<57} │ {:<57} │", primary_issue, backup_issue);
            }
        }
    }
    
    println!("└───────────────────────────────────────────────────────────┴───────────────────────────────────────────────────────────┘");
}

fn display_single_column_view(config: &Config, results: &HashMap<String, ComprehensiveStatus>) {
    println!("{}", "─".repeat(150));
    println!("{:<35} {:<15} {:<15} {:<12} {:<10} {:<15} {:<25} {:<15}", 
        "Node".bright_cyan(), 
        "Connection".bright_cyan(), 
        "Process".bright_cyan(), 
        "Disk".bright_cyan(),
        "Load".bright_cyan(),
        "Sync".bright_cyan(),
        "Version".bright_cyan(),
        "Swap Ready".bright_cyan()
    );
    println!("{}", "─".repeat(150));
    
    for (role, status) in results {
        display_node_row(config, role, status);
    }
    
    println!("{}", "─".repeat(150));
}

fn display_node_row(config: &Config, role: &str, status: &ComprehensiveStatus) {
    let node_label = config.nodes.get(role)
        .map(|node| format!("{} ({} - {})", node.label, node.host, role.to_uppercase()))
        .unwrap_or_else(|| role.to_string());
    
    println!("{:<35} {:<15} {:<15} {:<12} {:<10} {:<15} {:<25} {:<15}", 
        node_label,
        format_connection_status(status),
        format_process_status(status),
        format_disk_usage(status),
        format_system_load(status),
        format_sync_status(status),
        format_version(status),
        format_swap_readiness(status)
    );
    
    if let Some(error) = &status.error {
        println!("    {}: {}", "Error".red(), error.red());
    }
    
    if !status.swap_issues.is_empty() {
        for issue in &status.swap_issues {
            println!("    {}: {}", "Swap Issue".yellow(), issue.yellow());
        }
    }
}

// Helper functions for formatting status fields
fn format_connection_status(status: &ComprehensiveStatus) -> String {
    if status.connected { 
        "✅ Connected".green().to_string()
    } else { 
        "❌ Failed".red().to_string()
    }
}

fn format_process_status(status: &ComprehensiveStatus) -> String {
    match &status.validator_running {
        Some(true) => "✅ Running".green().to_string(),
        Some(false) => "❌ Stopped".red().to_string(),
        None => "❓ Unknown".yellow().to_string(),
    }
}

fn format_disk_usage(status: &ComprehensiveStatus) -> String {
    status.ledger_disk_usage
        .map(|d| {
            if d > 90 { format!("{}%", d).red().to_string() }
            else if d > 80 { format!("{}%", d).yellow().to_string() }
            else { format!("{}%", d).green().to_string() }
        })
        .unwrap_or_else(|| "N/A".dimmed().to_string())
}

fn format_system_load(status: &ComprehensiveStatus) -> String {
    status.system_load
        .map(|l| format!(" {:.1}", l))
        .unwrap_or_else(|| " N/A".to_string())
}

fn format_sync_status(status: &ComprehensiveStatus) -> String {
    status.sync_status
        .as_ref()
        .map(|s| {
            if s == "In Sync" { format!(" {}", s).green().to_string() }
            else if s == "Behind" { format!(" {}", s).red().to_string() }
            else { format!(" {}", s).yellow().to_string() }
        })
        .unwrap_or_else(|| " N/A".dimmed().to_string())
}

fn format_version(status: &ComprehensiveStatus) -> String {
    status.version
        .as_ref()
        .map(|v| {
            if v.contains("Jito") { v.bright_magenta().to_string() }
            else if v.contains("Firedancer") { v.bright_blue().to_string() }
            else if v.contains("agave") { v.green().to_string() }
            else { v.to_string() }
        })
        .unwrap_or_else(|| "N/A".dimmed().to_string())
}

// Plain formatting functions for side-by-side display (no colors for alignment)
fn format_connection_status_plain(status: &ComprehensiveStatus) -> String {
    if status.connected { 
        "✅ Connected".to_string()
    } else { 
        "❌ Failed".to_string()
    }
}

fn format_process_status_plain(status: &ComprehensiveStatus) -> String {
    match &status.validator_running {
        Some(true) => "✅ Running".to_string(),
        Some(false) => "❌ Stopped".to_string(),
        None => "❓ Unknown".to_string(),
    }
}

fn format_disk_usage_plain(status: &ComprehensiveStatus) -> String {
    status.ledger_disk_usage
        .map(|d| format!("{}%", d))
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_system_load_plain(status: &ComprehensiveStatus) -> String {
    status.system_load
        .map(|l| format!(" {:.1}", l))
        .unwrap_or_else(|| " N/A".to_string())
}

fn format_sync_status_plain(status: &ComprehensiveStatus) -> String {
    status.sync_status
        .as_ref()
        .map(|s| format!(" {}", s))
        .unwrap_or_else(|| " N/A".to_string())
}

fn format_version_plain(status: &ComprehensiveStatus) -> String {
    status.version
        .as_ref()
        .map(|v| v.clone())
        .unwrap_or_else(|| "N/A".to_string())
}

fn format_swap_readiness(status: &ComprehensiveStatus) -> String {
    match status.swap_ready {
        Some(true) => "✅ Ready".green().to_string(),
        Some(false) => "❌ Not Ready".red().to_string(),
        None => "❓ Unknown".yellow().to_string(),
    }
}

fn format_swap_readiness_plain(status: &ComprehensiveStatus) -> String {
    match status.swap_ready {
        Some(true) => "✅ Ready".to_string(),
        Some(false) => "❌ Not Ready".to_string(),
        None => "❓ Unknown".to_string(),
    }
}

fn format_swap_checklist(status: &ComprehensiveStatus) -> Vec<String> {
    let mut checklist = Vec::new();
    
    if status.swap_checklist.is_empty() {
        checklist.push("No swap checks available".to_string());
        return checklist;
    }
    
    for (description, is_ready) in &status.swap_checklist {
        let icon = if *is_ready { "✅" } else { "❌" };
        checklist.push(format!("  {} {}", icon, description));
    }
    
    checklist
}

#[derive(Debug)]
struct ComprehensiveStatus {
    connected: bool,
    validator_running: Option<bool>,
    ledger_disk_usage: Option<u32>,
    system_load: Option<f64>,
    sync_status: Option<String>,
    version: Option<String>,
    swap_ready: Option<bool>,
    swap_issues: Vec<String>,
    swap_checklist: Vec<(String, bool)>, // (description, is_ready)
    error: Option<String>,
}

impl ComprehensiveStatus {
    fn connection_failed(error: String) -> Self {
        ComprehensiveStatus {
            connected: false,
            validator_running: None,
            ledger_disk_usage: None,
            system_load: None,
            sync_status: None,
            version: None,
            swap_ready: None,
            swap_issues: Vec::new(),
            swap_checklist: Vec::new(),
            error: Some(error),
        }
    }
}