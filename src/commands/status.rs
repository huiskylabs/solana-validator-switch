use anyhow::Result;
use colored::*;
use std::collections::HashMap;
use comfy_table::{Table, Cell, Color, Attribute, ContentArrangement, presets::UTF8_BORDERS_ONLY, modifiers::UTF8_ROUND_CORNERS};

use crate::AppState;
use crate::types::{NodeConfig, Config};

pub async fn status_command(app_state: &AppState) -> Result<()> {
    if app_state.config.nodes.is_empty() {
        println!("{}", "⚠️ No nodes configured. Run setup first.".yellow());
        return Ok(());
    }
    
    // Show comprehensive status in one clean view
    show_comprehensive_status(app_state).await
}

async fn show_comprehensive_status(app_state: &AppState) -> Result<()> {
    println!("\n{}", "📋 Validator Status".bright_cyan().bold());
    println!();
    
    // Show simple status message
    println!("{}", "🔍 Checking validator status...".dimmed());
    
    // Check each node
    let mut results = HashMap::new();
    
    // Get the SSH pool
    let mut pool = app_state.ssh_pool.lock().unwrap();
    
    for (role, node) in &app_state.config.nodes {
        print!("  Checking {} validator... ", role);
        
        // Use existing connection from pool
        match pool.execute_command(node, &app_state.config.ssh.key_path, "echo 'test'").await {
            Ok(_) => {
                let status = check_comprehensive_status(&mut *pool, node, &app_state.config.ssh.key_path).await?;
                results.insert(role.clone(), status);
                println!("{}", "✅ Done".green());
            }
            Err(e) => {
                println!("{}", "❌ Failed".red());
                results.insert(role.clone(), ComprehensiveStatus::connection_failed(e.to_string()));
            }
        }
    }
    
    println!();
    
    // Display results in clean table
    display_status_table(&app_state.config, &results);
    
    Ok(())
}

async fn check_comprehensive_status(pool: &mut crate::ssh::SshConnectionPool, node: &NodeConfig, ssh_key_path: &str) -> Result<ComprehensiveStatus> {
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
    match pool.execute_command(node, ssh_key_path, "ps aux | grep -Ei 'solana-validator|agave|fdctl|firedancer'").await {
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
    match pool.execute_command(node, ssh_key_path, &format!("df {} | tail -1 | awk '{{print $5}}' | sed 's/%//'", node.paths.ledger)).await {
        Ok(output) => {
            if let Ok(usage) = output.trim().parse::<u32>() {
                status.ledger_disk_usage = Some(usage);
            }
        }
        Err(_) => {}
    }
    
    // Check system load
    match pool.execute_command(node, ssh_key_path, "uptime | awk -F'load average:' '{print $2}' | awk '{print $1}' | sed 's/,//'").await {
        Ok(output) => {
            if let Ok(load) = output.trim().parse::<f64>() {
                status.system_load = Some(load);
            }
        }
        Err(_) => {}
    }
    
    // Check sync status using solana catchup
    let solana_cli = &node.paths.solana_cli_path;
    match pool.execute_command(node, ssh_key_path, &format!("{} catchup --our-localhost", solana_cli)).await {
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
    status.version = detect_validator_version(pool, node, ssh_key_path).await;
    
    // Check swap readiness
    let (swap_ready, swap_issues, swap_checklist) = check_swap_readiness(pool, node, ssh_key_path).await;
    status.swap_ready = Some(swap_ready);
    status.swap_issues = swap_issues;
    status.swap_checklist = swap_checklist;
    
    Ok(status)
}

async fn check_swap_readiness(pool: &mut crate::ssh::SshConnectionPool, node: &NodeConfig, ssh_key_path: &str) -> (bool, Vec<String>, Vec<(String, bool)>) {
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
        match pool.execute_command(node, ssh_key_path, &format!("test -f {}", path)).await {
            Ok(_) => {
                // File exists, now check if it's readable
                match pool.execute_command(node, ssh_key_path, &format!("test -r {}", path)).await {
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
    match pool.execute_command(node, ssh_key_path, &format!("test -d {}", node.paths.ledger)).await {
        Ok(_) => {
            // Directory exists, check if writable
            match pool.execute_command(node, ssh_key_path, &format!("test -w {}", node.paths.ledger)).await {
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
    let disk_space_ok = if let Ok(output) = pool.execute_command(node, ssh_key_path, &format!("df {} | tail -1 | awk '{{print $4}}'", node.paths.ledger)).await {
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
    let cli_ok = pool.execute_command(node, ssh_key_path, &format!("test -x {}", node.paths.solana_cli_path)).await.is_ok();
    if !cli_ok {
        issues.push(format!("Solana CLI not executable: {}", node.paths.solana_cli_path));
        all_ready = false;
    }
    checklist.push(("Solana CLI".to_string(), cli_ok));
    
    (all_ready, issues, checklist)
}

async fn detect_validator_version(pool: &mut crate::ssh::SshConnectionPool, node: &NodeConfig, ssh_key_path: &str) -> Option<String> {
    // Get process list to detect validator type
    let ps_output = pool.execute_command(node, ssh_key_path, "ps aux | grep -Ei 'solana-validator|agave|fdctl|firedancer'").await.ok()?;
    
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
        get_firedancer_version(pool, node, ssh_key_path, executable_path).await
    } else if executable_path.contains("target/release/agave-validator") {
        // Jito or Agave
        get_jito_agave_version(pool, node, ssh_key_path, executable_path).await
    } else {
        None
    }
}

async fn get_firedancer_version(pool: &mut crate::ssh::SshConnectionPool, node: &NodeConfig, ssh_key_path: &str, executable_path: &str) -> Option<String> {
    let version_output = pool.execute_command(node, ssh_key_path, &format!("{} --version", executable_path)).await.ok()?;
    
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

async fn get_jito_agave_version(pool: &mut crate::ssh::SshConnectionPool, node: &NodeConfig, ssh_key_path: &str, executable_path: &str) -> Option<String> {
    // Try the executable path first
    if let Ok(version_output) = pool.execute_command(node, ssh_key_path, &format!("{} --version", executable_path)).await {
        if let Some(version_line) = version_output.lines().next() {
            let version_line = version_line.trim();
            if !version_line.is_empty() {
                return Some(parse_agave_version(version_line));
            }
        }
    }
    
    // Fallback to standard commands
    if let Ok(version_output) = pool.execute_command(node, ssh_key_path, "agave-validator --version").await {
        if let Some(version_line) = version_output.lines().next() {
            let version_line = version_line.trim();
            if !version_line.is_empty() {
                return Some(parse_agave_version(version_line));
            }
        }
    }
    
    // Final fallback
    if let Ok(version_output) = pool.execute_command(node, ssh_key_path, "solana-validator --version").await {
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

async fn get_solana_validator_version(pool: &mut crate::ssh::SshConnectionPool, node: &NodeConfig, ssh_key_path: &str, executable_path: &str) -> Option<String> {
    let version_output = pool.execute_command(node, ssh_key_path, &format!("{} --version", executable_path)).await.ok()?;
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
        display_primary_backup_table(primary_node, primary, backup_node, backup);
    } else {
        // Fallback to single column if we don't have both primary and backup
        display_all_nodes_table(config, results);
    }
    
    // Display any other nodes that aren't primary or backup
    let other_nodes: Vec<_> = results.iter()
        .filter(|(role, _)| *role != "primary" && *role != "backup")
        .collect();
    
    if !other_nodes.is_empty() {
        println!("\n{}", "🔧 Other Validators".bright_cyan().bold());
        display_other_nodes_table(config, &other_nodes);
    }
    
    println!();
}

fn display_primary_backup_table(
    primary_node: Option<&NodeConfig>, 
    primary_status: &ComprehensiveStatus,
    backup_node: Option<&NodeConfig>,
    backup_status: &ComprehensiveStatus
) {
    let mut table = Table::new();
    
    // Create custom table style with minimal borders
    table.load_preset(comfy_table::presets::UTF8_BORDERS_ONLY)
         .apply_modifier(UTF8_ROUND_CORNERS)
         .set_content_arrangement(ContentArrangement::Dynamic);
    
    // Header row
    table.add_row(vec![
        Cell::new("").add_attribute(Attribute::Bold),
        Cell::new("PRIMARY").add_attribute(Attribute::Bold).fg(Color::Green),
        Cell::new("BACKUP").add_attribute(Attribute::Bold).fg(Color::Yellow),
    ]);
    
    // Node info as subheader
    let primary_label = primary_node.map(|n| format!("🖥️ {} ({})", n.label, n.host)).unwrap_or("🖥️ Primary".to_string());
    let backup_label = backup_node.map(|n| format!("🖥️ {} ({})", n.label, n.host)).unwrap_or("🖥️ Backup".to_string());
    
    table.add_row(vec![
        Cell::new("Node").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new(&primary_label).fg(Color::Green),
        Cell::new(&backup_label).fg(Color::Yellow),
    ]);
    
    // Add separator line after subheader
    table.add_row(vec![
        Cell::new("─".repeat(15)).fg(Color::DarkGrey),
        Cell::new("─".repeat(25)).fg(Color::DarkGrey),
        Cell::new("─".repeat(25)).fg(Color::DarkGrey),
    ]);
    
    // Status rows with labels on the left
    table.add_row(vec![
        Cell::new("Connection").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new(format_connection_status_plain(primary_status)),
        Cell::new(format_connection_status_plain(backup_status)),
    ]);
    
    table.add_row(vec![
        Cell::new("Process").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new(format_process_status_plain(primary_status)),
        Cell::new(format_process_status_plain(backup_status)),
    ]);
    
    table.add_row(vec![
        Cell::new("Disk Usage").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new(format_disk_usage_plain(primary_status)),
        Cell::new(format_disk_usage_plain(backup_status)),
    ]);
    
    table.add_row(vec![
        Cell::new("System Load").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new(format_system_load_plain(primary_status)),
        Cell::new(format_system_load_plain(backup_status)),
    ]);
    
    table.add_row(vec![
        Cell::new("Sync Status").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new(format_sync_status_plain(primary_status)),
        Cell::new(format_sync_status_plain(backup_status)),
    ]);
    
    table.add_row(vec![
        Cell::new("Version").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new(format_version_plain(primary_status)),
        Cell::new(format_version_plain(backup_status)),
    ]);
    
    table.add_row(vec![
        Cell::new("Swap Ready").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new(format_swap_readiness_plain(primary_status)),
        Cell::new(format_swap_readiness_plain(backup_status)),
    ]);
    
    // Add swap checklist as sub-rows
    let primary_checklist = format_swap_checklist(primary_status);
    let backup_checklist = format_swap_checklist(backup_status);
    
    if !primary_checklist.is_empty() || !backup_checklist.is_empty() {
        let max_lines = primary_checklist.len().max(backup_checklist.len());
        for i in 0..max_lines {
            let primary_item = primary_checklist.get(i).cloned().unwrap_or_default();
            let backup_item = backup_checklist.get(i).cloned().unwrap_or_default();
            
            let left_label = if i == 0 { "  └ Checklist" } else { "" };
            
            table.add_row(vec![
                Cell::new(left_label).fg(Color::DarkGrey),
                Cell::new(primary_item).fg(Color::DarkGrey),
                Cell::new(backup_item).fg(Color::DarkGrey),
            ]);
        }
    }
    
    println!("{}", table);
}

fn display_all_nodes_table(config: &Config, results: &HashMap<String, ComprehensiveStatus>) {
    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY)
         .apply_modifier(UTF8_ROUND_CORNERS)
         .set_content_arrangement(ContentArrangement::Dynamic);
    
    // Create a 3-column layout for single nodes
    let nodes: Vec<_> = results.iter().collect();
    
    if nodes.len() == 1 {
        // Single node - use the same layout as primary/backup but with one column
        let (role, status) = nodes[0];
        let node_config = config.nodes.get(role);
        let node_label = node_config.map(|n| format!("🖥️ {} ({})", n.label, n.host))
            .unwrap_or_else(|| format!("🖥️ {}", role.to_uppercase()));
        
        table.add_row(vec![
            Cell::new("").add_attribute(Attribute::Bold),
            Cell::new(role.to_uppercase()).add_attribute(Attribute::Bold).fg(Color::Green),
        ]);
        
        table.add_row(vec![
            Cell::new("Node").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new(&node_label).fg(Color::Green),
        ]);
        
        table.add_row(vec![
            Cell::new("Connection").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new(format_connection_status_plain(status)),
        ]);
        
        table.add_row(vec![
            Cell::new("Process").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new(format_process_status_plain(status)),
        ]);
        
        table.add_row(vec![
            Cell::new("Disk Usage").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new(format_disk_usage_plain(status)),
        ]);
        
        table.add_row(vec![
            Cell::new("System Load").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new(format_system_load_plain(status)),
        ]);
        
        table.add_row(vec![
            Cell::new("Sync Status").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new(format_sync_status_plain(status)),
        ]);
        
        table.add_row(vec![
            Cell::new("Version").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new(format_version_plain(status)),
        ]);
        
        table.add_row(vec![
            Cell::new("Swap Ready").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new(format_swap_readiness_plain(status)),
        ]);
        
        // Add swap checklist as sub-rows
        let checklist = format_swap_checklist(status);
        for (i, item) in checklist.iter().enumerate() {
            let left_label = if i == 0 { "  └ Checklist" } else { "" };
            table.add_row(vec![
                Cell::new(left_label).fg(Color::DarkGrey),
                Cell::new(item).fg(Color::DarkGrey),
            ]);
        }
    } else {
        // Multiple nodes - use traditional table format
        table.add_row(vec![
            Cell::new("Node").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Connection").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Process").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Disk").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Load").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Sync").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Version").add_attribute(Attribute::Bold).fg(Color::Cyan),
            Cell::new("Swap Ready").add_attribute(Attribute::Bold).fg(Color::Cyan),
        ]);
        
        for (role, status) in results {
            let node_label = config.nodes.get(role)
                .map(|node| format!("{} ({} - {})", node.label, node.host, role.to_uppercase()))
                .unwrap_or_else(|| role.to_string());
            
            table.add_row(vec![
                Cell::new(node_label),
                Cell::new(format_connection_status_plain(status)),
                Cell::new(format_process_status_plain(status)),
                Cell::new(format_disk_usage_plain(status)),
                Cell::new(format_system_load_plain(status)),
                Cell::new(format_sync_status_plain(status)),
                Cell::new(format_version_plain(status)),
                Cell::new(format_swap_readiness_plain(status)),
            ]);
        }
    }
    
    println!("{}", table);
}

fn display_other_nodes_table(config: &Config, other_nodes: &[(&String, &ComprehensiveStatus)]) {
    let mut table = Table::new();
    table.load_preset(UTF8_BORDERS_ONLY)
         .apply_modifier(UTF8_ROUND_CORNERS)
         .set_content_arrangement(ContentArrangement::Dynamic);
    
    // Header
    table.add_row(vec![
        Cell::new("Node").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new("Connection").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new("Process").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new("Disk").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new("Load").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new("Sync").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new("Version").add_attribute(Attribute::Bold).fg(Color::Cyan),
        Cell::new("Swap Ready").add_attribute(Attribute::Bold).fg(Color::Cyan),
    ]);
    
    // Data rows
    for (role, status) in other_nodes {
        let node_label = config.nodes.get(*role)
            .map(|node| format!("{} ({} - {})", node.label, node.host, role.to_uppercase()))
            .unwrap_or_else(|| role.to_string());
        
        table.add_row(vec![
            Cell::new(node_label),
            Cell::new(format_connection_status_plain(status)),
            Cell::new(format_process_status_plain(status)),
            Cell::new(format_disk_usage_plain(status)),
            Cell::new(format_system_load_plain(status)),
            Cell::new(format_sync_status_plain(status)),
            Cell::new(format_version_plain(status)),
            Cell::new(format_swap_readiness_plain(status)),
        ]);
    }
    
    println!("{}", table);
}

// Plain formatting functions for table display
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