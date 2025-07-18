use anyhow::Result;
use colored::*;
use comfy_table::{
    modifiers::UTF8_ROUND_CORNERS, presets::UTF8_BORDERS_ONLY, Attribute, Cell, Color,
    ContentArrangement, Table,
};
use std::collections::HashMap;
use std::io::{stdout, Write};
use std::time::Duration;
use tokio::time::interval;

use crate::solana_rpc::{fetch_vote_account_data, ValidatorVoteData};
use crate::types::{Config, NodeConfig};
use crate::AppState;

pub async fn status_command(app_state: &AppState) -> Result<()> {
    if app_state.config.validators.is_empty() {
        println!(
            "{}",
            "⚠️ No validators configured. Run setup first.".yellow()
        );
        return Ok(());
    }

    // Use enhanced UI by default, allow fallback to standard UI with env var
    if std::env::var("USE_STANDARD_UI").unwrap_or_default() == "1" {
        // Use the standard auto-refresh UI
        crate::commands::status_ui::show_auto_refresh_status_ui(app_state).await
    } else {
        // Use the new enhanced UI with SSH streaming (default)
        crate::commands::status_ui_v2::show_enhanced_status_ui(app_state).await
    }
}

#[allow(dead_code)]
async fn show_comprehensive_status(app_state: &AppState) -> Result<()> {
    println!("\n{}", "📋 Validator Status".bright_cyan().bold());
    println!();

    // Use the pre-loaded validator statuses from startup - no need to re-detect
    display_status_table_from_app_state(app_state);

    Ok(())
}

#[allow(dead_code)]
async fn show_auto_refresh_status(app_state: &AppState) -> Result<()> {
    // Set up Ctrl+C handler
    let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
    let r = running.clone();

    ctrlc::set_handler(move || {
        r.store(false, std::sync::atomic::Ordering::SeqCst);
    })?;

    // Create a 3-second interval
    let mut refresh_interval = interval(Duration::from_secs(3));

    // Display header once
    print!("\x1B[2J\x1B[1;1H"); // Clear screen initially
    println!(
        "{}",
        "📋 Validator Status (Auto-refresh every 3s)"
            .bright_cyan()
            .bold()
    );
    println!("{}", "─".repeat(80).dimmed());
    println!();

    // Store cursor position after header
    print!("\x1B[s"); // Save cursor position
    stdout().flush()?;

    // First run - display the full table
    if let Err(e) = display_status_with_rpc_data(app_state, true).await {
        eprintln!("Error fetching status: {}", e);
    }
    stdout().flush()?;

    // Count lines in the table to know where the last row is
    let table_lines = count_table_lines(app_state);

    while running.load(std::sync::atomic::Ordering::SeqCst) {
        // Wait for next tick first
        refresh_interval.tick().await;

        // Move cursor to the last row of the table
        print!("\x1B[u"); // Restore to saved position
        print!("\x1B[{}B", table_lines - 1); // Move down to last row

        // Clear just the last row
        print!("\x1B[2K"); // Clear current line

        // Update only the vote status row
        if let Err(e) = display_vote_status_row_only(app_state).await {
            eprintln!("Error fetching vote status: {}", e);
        }

        stdout().flush()?;
    }

    Ok(())
}

#[allow(dead_code)]
async fn display_status_with_rpc_data(app_state: &AppState, full_display: bool) -> Result<()> {
    for (index, validator_status) in app_state.validator_statuses.iter().enumerate() {
        let validator_pair = &validator_status.validator_pair;

        if full_display {
            // Display validator info with name if available
            if let Some(ref metadata) = validator_status.metadata {
                if let Some(ref name) = metadata.name {
                    println!(
                        "{} Validator: {}",
                        "🔗".bright_cyan(),
                        name.bright_white().bold()
                    );
                    println!("   Vote: {}", validator_pair.vote_pubkey.dimmed());
                    println!("   Identity: {}", validator_pair.identity_pubkey.dimmed());
                } else {
                    println!(
                        "{} Validator {} - Vote: {}",
                        "🔗".bright_cyan(),
                        index + 1,
                        validator_pair.vote_pubkey
                    );
                }
            } else {
                println!(
                    "{} Validator {} - Vote: {}",
                    "🔗".bright_cyan(),
                    index + 1,
                    validator_pair.vote_pubkey
                );
            }

            println!();
        }

        // Fetch vote account data from RPC
        let vote_data =
            match fetch_vote_account_data(&validator_pair.rpc, &validator_pair.vote_pubkey).await {
                Ok(data) => Some(data),
                Err(e) => {
                    eprintln!("Failed to fetch vote data: {}", e);
                    None
                }
            };

        if full_display {
            // Get the two nodes with their statuses
            let nodes_with_status = &validator_status.nodes_with_status;
            if nodes_with_status.len() >= 2 {
                let node_0 = &nodes_with_status[0];
                let node_1 = &nodes_with_status[1];

                display_simple_status_table_with_rpc(
                    &node_0.node,
                    &node_0.status,
                    &node_1.node,
                    &node_1.status,
                    validator_status,
                    vote_data.as_ref(),
                );
            }

            println!();
        }
    }

    Ok(())
}

#[allow(dead_code)]
fn display_vote_data(vote_data: &ValidatorVoteData) {
    // Voting status
    let voting_status = if vote_data.is_voting {
        "✅ Voting".green()
    } else {
        "⚠️ Not Voting".yellow()
    };
    println!("   Status: {}", voting_status);

    // Display most recent vote exactly like solana vote-account output
    if let Some(recent_vote) = vote_data.recent_votes.first() {
        println!(
            "   Recent Vote: slot: {} (confirmation count: {}) (latency {})",
            recent_vote.slot.to_string().bright_white(),
            recent_vote.confirmation_count,
            recent_vote.latency.to_string().cyan()
        );
    }

    // Display credits and commission
    println!(
        "   Credits: {} | Commission: {}%",
        vote_data
            .vote_account_info
            .credits
            .to_string()
            .bright_white(),
        vote_data.vote_account_info.commission
    );
}

#[allow(dead_code)]
fn display_simple_status_table_with_rpc(
    node_0: &crate::types::NodeConfig,
    node_0_status: &crate::types::NodeStatus,
    node_1: &crate::types::NodeConfig,
    node_1_status: &crate::types::NodeStatus,
    validator_status: &crate::ValidatorStatus,
    vote_data: Option<&ValidatorVoteData>,
) {
    let mut table = Table::new();

    // Create custom table style with minimal borders
    table
        .load_preset(comfy_table::presets::UTF8_BORDERS_ONLY)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Header row with dynamic labels
    let node_0_label = match node_0_status {
        crate::types::NodeStatus::Active => "ACTIVE",
        crate::types::NodeStatus::Standby => "STANDBY",
        crate::types::NodeStatus::Unknown => "UNKNOWN",
    };
    let node_1_label = match node_1_status {
        crate::types::NodeStatus::Active => "ACTIVE",
        crate::types::NodeStatus::Standby => "STANDBY",
        crate::types::NodeStatus::Unknown => "UNKNOWN",
    };

    let node_0_color = if node_0_label == "ACTIVE" {
        Color::Green
    } else if node_0_label == "STANDBY" {
        Color::Yellow
    } else {
        Color::DarkGrey
    };
    let node_1_color = if node_1_label == "ACTIVE" {
        Color::Green
    } else if node_1_label == "STANDBY" {
        Color::Yellow
    } else {
        Color::DarkGrey
    };

    // Node info as header
    let node_0_info = format!("🖥️ {} ({})", node_0.label, node_0.host);
    let node_1_info = format!("🖥️ {} ({})", node_1.label, node_1.host);

    table.add_row(vec![
        Cell::new("Node")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(&node_0_info),
        Cell::new(&node_1_info),
    ]);

    // Add separator line after subheader
    table.add_row(vec![
        Cell::new("─".repeat(15)).fg(Color::DarkGrey),
        Cell::new("─".repeat(25)).fg(Color::DarkGrey),
        Cell::new("─".repeat(25)).fg(Color::DarkGrey),
    ]);

    // Status rows with basic info
    table.add_row(vec![
        Cell::new("Status")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_label).fg(node_0_color),
        Cell::new(node_1_label).fg(node_1_color),
    ]);

    // Add executable paths
    let node_0_agave = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.agave_validator_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_agave = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.agave_validator_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Agave Executable")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_agave),
        Cell::new(node_1_agave),
    ]);

    // Add Solana CLI executable row
    let node_0_solana = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.solana_cli_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_solana = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.solana_cli_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Solana CLI")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_solana),
        Cell::new(node_1_solana),
    ]);

    let node_0_fdctl = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.fdctl_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_fdctl = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.fdctl_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Fdctl Executable")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_fdctl),
        Cell::new(node_1_fdctl),
    ]);

    // Add version information
    let node_0_version = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.version.as_ref())
        .cloned()
        .unwrap_or("N/A".to_string());
    let node_1_version = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.version.as_ref())
        .cloned()
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Version")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_version),
        Cell::new(node_1_version),
    ]);

    // Add sync status
    let node_0_sync = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.sync_status.as_ref())
        .cloned()
        .unwrap_or("Unknown".to_string());
    let node_1_sync = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.sync_status.as_ref())
        .cloned()
        .unwrap_or("Unknown".to_string());

    table.add_row(vec![
        Cell::new("Sync Status")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_sync),
        Cell::new(node_1_sync),
    ]);

    // Add identity pubkey (from current_identity)
    let node_0_identity = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.current_identity.as_ref())
        .cloned()
        .unwrap_or("Unknown".to_string());
    let node_1_identity = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.current_identity.as_ref())
        .cloned()
        .unwrap_or("Unknown".to_string());

    table.add_row(vec![
        Cell::new("Identity")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_identity),
        Cell::new(node_1_identity),
    ]);

    // Add ledger path (detected from running process)
    let node_0_ledger = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.ledger_path.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_ledger = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.ledger_path.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Ledger Path")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_ledger),
        Cell::new(node_1_ledger),
    ]);

    // Add swap readiness
    let node_0_swap = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.swap_ready)
        .map(|ready| if ready { "✅ Ready" } else { "❌ Not Ready" })
        .unwrap_or("❓ Unknown");
    let node_1_swap = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.swap_ready)
        .map(|ready| if ready { "✅ Ready" } else { "❌ Not Ready" })
        .unwrap_or("❓ Unknown");

    table.add_row(vec![
        Cell::new("Swap Ready")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_swap),
        Cell::new(node_1_swap),
    ]);

    // Add RPC voting status row as the last row
    if let Some(vote_data) = vote_data {
        let voting_status = if vote_data.is_voting {
            "✅ Voting"
        } else {
            "⚠️ Not Voting"
        };

        let vote_info = if let Some(recent_vote) = vote_data.recent_votes.first() {
            let current_slot = vote_data.vote_account_info.current_slot.unwrap_or(0);
            let diff = current_slot.saturating_sub(recent_vote.slot);
            format!("(-{})", diff)
        } else {
            "No recent votes".to_string()
        };

        table.add_row(vec![
            Cell::new("Vote Status")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new(voting_status).fg(if vote_data.is_voting {
                Color::Green
            } else {
                Color::Yellow
            }),
            Cell::new(&vote_info),
        ]);
    }

    println!("{}", table);
}

#[allow(dead_code)]
async fn check_comprehensive_status(
    pool: &mut crate::ssh::AsyncSshPool,
    node: &NodeConfig,
    ssh_key_path: &str,
    _validator_pair: &crate::types::ValidatorPair,
    agave_executable: &Option<String>,
) -> Result<ComprehensiveStatus> {
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
        identity_verified: None,
        vote_account_verified: None,
        verification_issues: Vec::new(),
        error: None,
        current_identity: None,
    };

    // Create a single batched command to get all basic info efficiently
    // Note: We no longer use monitor command for identity extraction

    let default_path = String::new();
    let agave_exec_path = agave_executable.as_ref().unwrap_or(&default_path);

    // Use a default ledger path for disk check - this is a dead code function anyway
    let batch_cmd = format!(
        "echo '=== PROCESSES ===' && ps aux | grep -Ei 'solana-validator|agave|fdctl|firedancer' | grep -v grep; \
         echo '=== DISK ===' && df /mnt/solana_ledger | tail -1 | awk '{{print $5}}' | sed 's/%//'; \
         echo '=== LOAD ===' && uptime | awk -F'load average:' '{{print $2}}' | awk '{{print $1}}' | sed 's/,//'; \
         echo '=== SYNC ===' && timeout 10 bash -c 'AGAVE_EXEC=\"{}\"; if [ -n \"$AGAVE_EXEC\" ] && [ -x \"$AGAVE_EXEC\" ]; then \"$AGAVE_EXEC\" catchup --our-localhost 2>&1 | grep -m1 \"has caught up\"; else echo \"no-agave-executable\"; fi' || echo 'sync-timeout'; \
         echo '=== VERSION ===' && timeout 5 {} --version 2>/dev/null || echo 'version-timeout'; \
         echo '=== END ==='",
        agave_exec_path,
        agave_exec_path
    );

    match pool.execute_command(node, ssh_key_path, &batch_cmd).await {
        Ok(output) => {
            parse_batch_output(&output, &mut status);
        }
        Err(_) => {
            status.validator_running = Some(false);
        }
    }

    // Check swap readiness (this is still separate as it needs multiple file checks)
    let (swap_ready, swap_issues, swap_checklist) =
        check_swap_readiness(pool, node, ssh_key_path, None).await;
    status.swap_ready = Some(swap_ready);
    status.swap_issues = swap_issues;
    status.swap_checklist = swap_checklist;

    // Identity is now extracted from catchup command during startup

    Ok(status)
}

#[allow(dead_code)]
fn parse_batch_output(output: &str, status: &mut ComprehensiveStatus) {
    let sections: Vec<&str> = output.split("=== ").collect();

    for section in sections {
        if section.starts_with("PROCESSES ===") {
            let lines: Vec<&str> = section.lines().skip(1).collect();
            let validator_processes: Vec<&str> = lines
                .iter()
                .filter(|line| !line.contains("grep"))
                .filter(|line| {
                    line.contains("solana-validator")
                        || line.contains("agave")
                        || line.contains("fdctl")
                        || line.contains("firedancer")
                })
                .cloned()
                .collect();

            status.validator_running = Some(!validator_processes.is_empty());
        } else if section.starts_with("DISK ===") {
            if let Some(line) = section.lines().nth(1) {
                if let Ok(usage) = line.trim().parse::<u32>() {
                    status.ledger_disk_usage = Some(usage);
                }
            }
        } else if section.starts_with("LOAD ===") {
            if let Some(line) = section.lines().nth(1) {
                if let Ok(load) = line.trim().parse::<f64>() {
                    status.system_load = Some(load);
                }
            }
        } else if section.starts_with("SYNC ===") {
            if let Some(line) = section.lines().nth(1) {
                let sync_output = line.trim();
                if sync_output.contains("has caught up") {
                    // Parse the catchup message: "8CzCcNCwg8nx3C4LfiUzanwok5EXoJza has caught up (us:344297365 them:344297365)"
                    // Extract identity (before "has caught up") and slot information
                    if let Some(caught_up_pos) = sync_output.find(" has caught up") {
                        let identity = sync_output[..caught_up_pos].trim();

                        // Store the identity if we haven't found it yet
                        if status.current_identity.is_none() && !identity.is_empty() {
                            status.current_identity = Some(identity.to_string());
                        }

                        // Extract slot information
                        if let Some(us_start) = sync_output.find("us:") {
                            if let Some(them_start) = sync_output.find("them:") {
                                let us_end = sync_output[us_start + 3..]
                                    .find(' ')
                                    .unwrap_or(sync_output.len() - us_start - 3)
                                    + us_start
                                    + 3;
                                let _them_end = sync_output[them_start + 5..]
                                    .find(')')
                                    .unwrap_or(sync_output.len() - them_start - 5)
                                    + them_start
                                    + 5;
                                let us_slot = &sync_output[us_start + 3..us_end];
                                status.sync_status = Some(format!("Caught up (slot: {})", us_slot));
                            } else {
                                status.sync_status = Some("Caught up".to_string());
                            }
                        } else {
                            status.sync_status = Some("Caught up".to_string());
                        }
                    } else {
                        status.sync_status = Some("Caught up".to_string());
                    }
                } else if sync_output.contains("sync-timeout") || sync_output.contains("timeout") {
                    status.sync_status = Some("Sync Timeout".to_string());
                } else if sync_output.contains("no-agave-executable") {
                    status.sync_status = Some("No Agave Exec".to_string());
                } else if sync_output.contains("behind") {
                    status.sync_status = Some("Behind".to_string());
                } else if !sync_output.is_empty() {
                    status.sync_status = Some("In Sync".to_string());
                }
            }
        } else if section.starts_with("IDENTITY ===") {
            // Look through all lines in the identity section
            for line in section.lines().skip(1) {
                let identity_output = line.trim();
                if identity_output.contains("timeout")
                    || identity_output.contains("no-validator-running")
                {
                    break;
                } else if identity_output.starts_with("Identity: ") {
                    let identity = identity_output.replace("Identity: ", "").trim().to_string();
                    if !identity.is_empty()
                        && identity != "timeout"
                        && identity != "no-validator-running"
                    {
                        status.current_identity = Some(identity);
                        break;
                    }
                }
            }
        } else if section.starts_with("VERSION ===") {
            // Parse --version command output
            for line in section.lines().skip(1) {
                let version_output = line.trim();
                if version_output.contains("version-timeout") || version_output.is_empty() {
                    break;
                } else if version_output.starts_with("solana-cli ") {
                    // Parse version output: "solana-cli 0.505.20216 (src:44f9f393; feat:3073396398, client:Firedancer)"
                    // or "solana-cli 2.1.13 (src:67412607; feat:1725507508, client:Agave)"

                    // Extract version number (second part)
                    let parts: Vec<&str> = version_output.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let version_num = parts[1];

                        // Extract client type from the end
                        if version_output.contains("client:Firedancer") {
                            status.version = Some(format!("Firedancer {}", version_num));
                        } else if version_output.contains("client:Agave") {
                            status.version = Some(format!("Agave {}", version_num));
                        } else {
                            // Fallback based on version number pattern
                            if version_num.starts_with("0.") {
                                status.version = Some(format!("Firedancer {}", version_num));
                            } else if version_num.starts_with("2.") {
                                status.version = Some(format!("Agave {}", version_num));
                            } else {
                                status.version = Some(format!("Unknown {}", version_num));
                            }
                        }
                        break;
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
async fn check_swap_readiness(
    pool: &mut crate::ssh::AsyncSshPool,
    node: &NodeConfig,
    ssh_key_path: &str,
    ledger_path: Option<&String>,
) -> (bool, Vec<String>, Vec<(String, bool)>) {
    let mut issues = Vec::new();
    let mut checklist = Vec::new();
    let mut all_ready = true;

    // Use detected ledger path if available, otherwise use a default
    let ledger = ledger_path
        .map(|s| s.as_str())
        .unwrap_or("/mnt/solana_ledger");
    let tower_pattern = format!("{}/tower-1_9-*.bin", ledger);

    // Batch file checks into single command
    let file_check_cmd = format!(
        "echo '=== FILES ===' && \
         test -r {} && echo 'funded_ok' || echo 'funded_fail'; \
         test -r {} && echo 'unfunded_ok' || echo 'unfunded_fail'; \
         test -r {} && echo 'vote_ok' || echo 'vote_fail'; \
         ls {} >/dev/null 2>&1 && echo 'tower_ok' || echo 'tower_fail'; \
         echo '=== DIRS ===' && \
         test -d {} && test -w {} && echo 'ledger_ok' || echo 'ledger_fail'; \
         echo '=== DISK ===' && \
         df {} | tail -1 | awk '{{print $4}}' | head -1",
        node.paths.funded_identity,
        node.paths.unfunded_identity,
        node.paths.vote_keypair,
        tower_pattern,
        ledger,
        ledger,
        ledger
    );

    match pool
        .execute_command(node, ssh_key_path, &file_check_cmd)
        .await
    {
        Ok(output) => {
            parse_swap_readiness_output(&output, &mut checklist, &mut issues, &mut all_ready);
        }
        Err(_) => {
            all_ready = false;
            issues.push("Failed to check file readiness".to_string());
        }
    }

    (all_ready, issues, checklist)
}

#[allow(dead_code)]
fn parse_swap_readiness_output(
    output: &str,
    checklist: &mut Vec<(String, bool)>,
    issues: &mut Vec<String>,
    all_ready: &mut bool,
) {
    let lines: Vec<&str> = output.lines().collect();

    for line in lines {
        match line.trim() {
            "funded_ok" => checklist.push(("Funded Identity".to_string(), true)),
            "funded_fail" => {
                checklist.push(("Funded Identity".to_string(), false));
                issues.push("Funded identity keypair missing or not readable".to_string());
                *all_ready = false;
            }
            "unfunded_ok" => checklist.push(("Unfunded Identity".to_string(), true)),
            "unfunded_fail" => {
                checklist.push(("Unfunded Identity".to_string(), false));
                issues.push("Unfunded identity keypair missing or not readable".to_string());
                *all_ready = false;
            }
            "vote_ok" => checklist.push(("Vote Keypair".to_string(), true)),
            "vote_fail" => {
                checklist.push(("Vote Keypair".to_string(), false));
                issues.push("Vote keypair missing or not readable".to_string());
                *all_ready = false;
            }
            "tower_ok" => checklist.push(("Tower File".to_string(), true)),
            "tower_fail" => {
                checklist.push(("Tower File".to_string(), false));
                issues.push("Tower file missing".to_string());
                *all_ready = false;
            }
            "ledger_ok" => checklist.push(("Ledger Directory".to_string(), true)),
            "ledger_fail" => {
                checklist.push(("Ledger Directory".to_string(), false));
                issues.push("Ledger directory missing or not writable".to_string());
                *all_ready = false;
            }
            _ => {
                // Check if it's a disk space value
                if let Ok(free_kb) = line.trim().parse::<u64>() {
                    let free_gb = free_kb / 1024 / 1024;
                    if free_gb < 10 {
                        checklist.push(("Disk Space (>10GB)".to_string(), false));
                        issues.push(format!("Low disk space: {}GB free (minimum 10GB)", free_gb));
                        *all_ready = false;
                    } else {
                        checklist.push(("Disk Space (>10GB)".to_string(), true));
                    }
                }
            }
        }
    }
}

#[allow(dead_code)]
async fn detect_validator_version(
    pool: &mut crate::ssh::AsyncSshPool,
    node: &NodeConfig,
    ssh_key_path: &str,
) -> Option<String> {
    // Get process list to detect validator type
    let ps_output = pool
        .execute_command(
            node,
            ssh_key_path,
            "ps aux | grep -Ei 'solana-validator|agave|fdctl|firedancer'",
        )
        .await
        .ok()?;

    // Filter out grep process itself and find validator processes
    let validator_processes: Vec<&str> = ps_output
        .lines()
        .filter(|line| !line.contains("grep"))
        .filter(|line| {
            line.contains("solana-validator")
                || line.contains("agave")
                || line.contains("fdctl")
                || line.contains("firedancer")
        })
        .collect();

    if validator_processes.is_empty() {
        return None;
    }

    // Find the process with the exact patterns you specified
    let process_line = validator_processes.iter().find(|line| {
        line.contains("build/native/gcc/bin/fdctl")
            || line.contains("target/release/agave-validator")
    })?;

    // Look for executable path in the process line
    let mut executable_path = None;

    // Split by whitespace and look for paths containing validator executables
    for part in process_line.split_whitespace() {
        if part.contains("build/native/gcc/bin/fdctl")
            || part.contains("target/release/agave-validator")
        {
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

#[allow(dead_code)]
async fn get_firedancer_version(
    pool: &mut crate::ssh::AsyncSshPool,
    node: &NodeConfig,
    ssh_key_path: &str,
    executable_path: &str,
) -> Option<String> {
    let version_output = pool
        .execute_command(
            node,
            ssh_key_path,
            &format!("{} --version", executable_path),
        )
        .await
        .ok()?;

    // Parse firedancer version format: "0.505.20216 (44f9f393d167138abe1c819f7424990a56e1913e)"
    for line in version_output.lines() {
        if line.contains('.') && (line.contains('(') || line.chars().any(|c| c.is_ascii_digit())) {
            // Extract just the version number part
            let version_part = line.trim().split_whitespace().next().unwrap_or(line.trim());
            return Some(format!("Firedancer {}", version_part));
        }
    }

    None
}

#[allow(dead_code)]
async fn get_jito_agave_version(
    pool: &mut crate::ssh::AsyncSshPool,
    node: &NodeConfig,
    ssh_key_path: &str,
    executable_path: &str,
) -> Option<String> {
    // Try the executable path first
    if let Ok(version_output) = pool
        .execute_command(
            node,
            ssh_key_path,
            &format!("{} --version", executable_path),
        )
        .await
    {
        if let Some(version_line) = version_output.lines().next() {
            let version_line = version_line.trim();
            if !version_line.is_empty() {
                return Some(parse_agave_version(version_line));
            }
        }
    }

    // Fallback to standard commands
    if let Ok(version_output) = pool
        .execute_command_with_args(node, ssh_key_path, "agave-validator", &["--version"])
        .await
    {
        if let Some(version_line) = version_output.lines().next() {
            let version_line = version_line.trim();
            if !version_line.is_empty() {
                return Some(parse_agave_version(version_line));
            }
        }
    }

    // Final fallback
    if let Ok(version_output) = pool
        .execute_command_with_args(node, ssh_key_path, "solana-validator", &["--version"])
        .await
    {
        if let Some(version_line) = version_output.lines().next() {
            let version_line = version_line.trim();
            if !version_line.is_empty() {
                return Some(version_line.to_string());
            }
        }
    }

    None
}

#[allow(dead_code)]
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

#[allow(dead_code)]
async fn get_solana_validator_version(
    pool: &mut crate::ssh::AsyncSshPool,
    node: &NodeConfig,
    ssh_key_path: &str,
    executable_path: &str,
) -> Option<String> {
    let version_output = pool
        .execute_command(
            node,
            ssh_key_path,
            &format!("{} --version", executable_path),
        )
        .await
        .ok()?;
    let version_line = version_output.lines().next()?.trim();
    Some(version_line.to_string())
}

#[allow(dead_code)]
fn display_status_table_from_app_state(app_state: &AppState) {
    println!("\n{}", "📋 Validator Status".bright_cyan().bold());
    println!();

    for (index, validator_status) in app_state.validator_statuses.iter().enumerate() {
        let validator_pair = &validator_status.validator_pair;

        // Display validator info with name if available
        if let Some(ref metadata) = validator_status.metadata {
            if let Some(ref name) = metadata.name {
                println!(
                    "{} Validator: {}",
                    "🔗".bright_cyan(),
                    name.bright_white().bold()
                );
                println!("   Vote: {}", validator_pair.vote_pubkey.dimmed());
                println!("   Identity: {}", validator_pair.identity_pubkey.dimmed());
            } else {
                // No name in metadata
                println!(
                    "{} Validator {} - Vote: {}",
                    "🔗".bright_cyan(),
                    index + 1,
                    validator_pair.vote_pubkey
                );
            }
        } else {
            // No metadata available
            println!(
                "{} Validator {} - Vote: {}",
                "🔗".bright_cyan(),
                index + 1,
                validator_pair.vote_pubkey
            );
        }
        println!();

        // Get the two nodes with their statuses
        let nodes_with_status = &validator_status.nodes_with_status;
        if nodes_with_status.len() >= 2 {
            let node_0 = &nodes_with_status[0];
            let node_1 = &nodes_with_status[1];

            display_simple_status_table(
                &node_0.node,
                &node_0.status,
                &node_1.node,
                &node_1.status,
                validator_status,
            );
        }

        println!();
    }
}

#[allow(dead_code)]
fn display_simple_status_table(
    node_0: &crate::types::NodeConfig,
    node_0_status: &crate::types::NodeStatus,
    node_1: &crate::types::NodeConfig,
    node_1_status: &crate::types::NodeStatus,
    validator_status: &crate::ValidatorStatus,
) {
    let mut table = Table::new();

    // Create custom table style with minimal borders
    table
        .load_preset(comfy_table::presets::UTF8_BORDERS_ONLY)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Header row with dynamic labels
    let node_0_label = match node_0_status {
        crate::types::NodeStatus::Active => "ACTIVE",
        crate::types::NodeStatus::Standby => "STANDBY",
        crate::types::NodeStatus::Unknown => "UNKNOWN",
    };
    let node_1_label = match node_1_status {
        crate::types::NodeStatus::Active => "ACTIVE",
        crate::types::NodeStatus::Standby => "STANDBY",
        crate::types::NodeStatus::Unknown => "UNKNOWN",
    };

    let node_0_color = if node_0_label == "ACTIVE" {
        Color::Green
    } else if node_0_label == "STANDBY" {
        Color::Yellow
    } else {
        Color::DarkGrey
    };
    let node_1_color = if node_1_label == "ACTIVE" {
        Color::Green
    } else if node_1_label == "STANDBY" {
        Color::Yellow
    } else {
        Color::DarkGrey
    };

    // Node info as header
    let node_0_info = format!("🖥️ {} ({})", node_0.label, node_0.host);
    let node_1_info = format!("🖥️ {} ({})", node_1.label, node_1.host);

    table.add_row(vec![
        Cell::new("Node")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(&node_0_info),
        Cell::new(&node_1_info),
    ]);

    // Add separator line after subheader
    table.add_row(vec![
        Cell::new("─".repeat(15)).fg(Color::DarkGrey),
        Cell::new("─".repeat(25)).fg(Color::DarkGrey),
        Cell::new("─".repeat(25)).fg(Color::DarkGrey),
    ]);

    // Status rows with basic info
    table.add_row(vec![
        Cell::new("Status")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_label).fg(node_0_color),
        Cell::new(node_1_label).fg(node_1_color),
    ]);

    // Add executable paths
    let node_0_agave = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.agave_validator_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_agave = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.agave_validator_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Agave Executable")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_agave),
        Cell::new(node_1_agave),
    ]);

    // Add Solana CLI executable row
    let node_0_solana = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.solana_cli_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_solana = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.solana_cli_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Solana CLI")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_solana),
        Cell::new(node_1_solana),
    ]);

    let node_0_fdctl = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.fdctl_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_fdctl = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.fdctl_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Fdctl Executable")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_fdctl),
        Cell::new(node_1_fdctl),
    ]);

    // Add version information
    let node_0_version = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.version.as_ref())
        .cloned()
        .unwrap_or("N/A".to_string());
    let node_1_version = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.version.as_ref())
        .cloned()
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Version")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_version),
        Cell::new(node_1_version),
    ]);

    // Add sync status
    let node_0_sync = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.sync_status.as_ref())
        .cloned()
        .unwrap_or("Unknown".to_string());
    let node_1_sync = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.sync_status.as_ref())
        .cloned()
        .unwrap_or("Unknown".to_string());

    table.add_row(vec![
        Cell::new("Sync Status")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_sync),
        Cell::new(node_1_sync),
    ]);

    // Add identity pubkey (from current_identity)
    let node_0_identity = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.current_identity.as_ref())
        .cloned()
        .unwrap_or("Unknown".to_string());
    let node_1_identity = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.current_identity.as_ref())
        .cloned()
        .unwrap_or("Unknown".to_string());

    table.add_row(vec![
        Cell::new("Identity")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_identity),
        Cell::new(node_1_identity),
    ]);

    // Add ledger path (detected from running process)
    let node_0_ledger = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.ledger_path.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_ledger = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.ledger_path.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Ledger Path")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_ledger),
        Cell::new(node_1_ledger),
    ]);

    // Add swap readiness
    let node_0_swap = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.swap_ready)
        .map(|ready| if ready { "✅ Ready" } else { "❌ Not Ready" })
        .unwrap_or("❓ Unknown");
    let node_1_swap = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.swap_ready)
        .map(|ready| if ready { "✅ Ready" } else { "❌ Not Ready" })
        .unwrap_or("❓ Unknown");

    table.add_row(vec![
        Cell::new("Swap Ready")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_swap),
        Cell::new(node_1_swap),
    ]);

    println!("{}", table);
}

#[allow(dead_code)]
fn display_status_table(
    config: &Config,
    results: &HashMap<String, ComprehensiveStatus>,
    app_state: &AppState,
) {
    println!("\n{}", "📋 Validator Status".bright_cyan().bold());
    println!();

    for (index, validator_pair) in config.validators.iter().enumerate() {
        println!(
            "{} Validator {} - Vote: {}",
            "🔗".bright_cyan(),
            index + 1,
            validator_pair.vote_pubkey
        );
        println!();

        // Get node statuses (no ordering, just display as configured)
        let node_0_status = results
            .get(&format!("validator_{}_node_0 (ACTIVE)", index))
            .or_else(|| results.get(&format!("validator_{}_node_0 (STANDBY)", index)))
            .or_else(|| results.get(&format!("validator_{}_node_0 (UNKNOWN)", index)));
        let node_1_status = results
            .get(&format!("validator_{}_node_1 (ACTIVE)", index))
            .or_else(|| results.get(&format!("validator_{}_node_1 (STANDBY)", index)))
            .or_else(|| results.get(&format!("validator_{}_node_1 (UNKNOWN)", index)));

        // Determine status labels for each node
        let node_0_label = if results.contains_key(&format!("validator_{}_node_0 (ACTIVE)", index))
        {
            "ACTIVE"
        } else if results.contains_key(&format!("validator_{}_node_0 (STANDBY)", index)) {
            "STANDBY"
        } else {
            "UNKNOWN"
        };

        let node_1_label = if results.contains_key(&format!("validator_{}_node_1 (ACTIVE)", index))
        {
            "ACTIVE"
        } else if results.contains_key(&format!("validator_{}_node_1 (STANDBY)", index)) {
            "STANDBY"
        } else {
            "UNKNOWN"
        };

        if let (Some(node_0_status), Some(node_1_status)) = (node_0_status, node_1_status) {
            let validator_status = &app_state.validator_statuses[index];
            display_primary_backup_table(
                Some(&validator_pair.nodes[0]),
                node_0_status,
                Some(&validator_pair.nodes[1]),
                node_1_status,
                node_0_label,
                node_1_label,
                validator_status,
            );
        }

        println!();
    }
}

#[allow(dead_code)]
fn display_node_status(role: &str, node: &NodeConfig, status: &ComprehensiveStatus) {
    // Simplified role display without color conversion
    let role_display = if role == "Primary" {
        role.green()
    } else {
        role.yellow()
    };

    println!("  {} {} ({}):", role_display, node.label, node.host);

    if !status.connected {
        println!("    ❌ Connection failed");
        if let Some(ref error) = status.error {
            println!("    Error: {}", error.red());
        }
        return;
    }

    // Display basic status
    let validator_status = match status.validator_running {
        Some(true) => "✅ Running".green(),
        Some(false) => "❌ Stopped".red(),
        None => "❓ Unknown".dimmed(),
    };
    println!("    Validator: {}", validator_status);

    if let Some(ref version) = status.version {
        println!("    Version: {}", version);
    }

    if let Some(usage) = status.ledger_disk_usage {
        let usage_display = if usage > 90 {
            format!("{}%", usage).red()
        } else if usage > 75 {
            format!("{}%", usage).yellow()
        } else {
            format!("{}%", usage).green()
        };
        println!("    Disk Usage: {}", usage_display);
    }

    if let Some(load) = status.system_load {
        let load_display = if load > 2.0 {
            format!("{:.2}", load).red()
        } else if load > 1.0 {
            format!("{:.2}", load).yellow()
        } else {
            format!("{:.2}", load).green()
        };
        println!("    System Load: {}", load_display);
    }

    if let Some(ref sync) = status.sync_status {
        println!("    Sync Status: {}", sync);
    }
}

#[allow(dead_code)]
fn display_primary_backup_table(
    node_0: Option<&NodeConfig>,
    node_0_status: &ComprehensiveStatus,
    node_1: Option<&NodeConfig>,
    node_1_status: &ComprehensiveStatus,
    node_0_label: &str,
    node_1_label: &str,
    validator_status: &crate::ValidatorStatus,
) {
    let mut table = Table::new();

    // Create custom table style with minimal borders
    table
        .load_preset(comfy_table::presets::UTF8_BORDERS_ONLY)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Header row with dynamic labels
    let node_0_color = if node_0_label == "ACTIVE" {
        Color::Green
    } else if node_0_label == "STANDBY" {
        Color::Yellow
    } else {
        Color::DarkGrey
    };
    let node_1_color = if node_1_label == "ACTIVE" {
        Color::Green
    } else if node_1_label == "STANDBY" {
        Color::Yellow
    } else {
        Color::DarkGrey
    };

    table.add_row(vec![
        Cell::new("").add_attribute(Attribute::Bold),
        Cell::new(node_0_label)
            .add_attribute(Attribute::Bold)
            .fg(node_0_color),
        Cell::new(node_1_label)
            .add_attribute(Attribute::Bold)
            .fg(node_1_color),
    ]);

    // Node info as subheader
    let node_0_info = node_0
        .map(|n| format!("🖥️ {} ({})", n.label, n.host))
        .unwrap_or("🖥️ Node 0".to_string());
    let node_1_info = node_1
        .map(|n| format!("🖥️ {} ({})", n.label, n.host))
        .unwrap_or("🖥️ Node 1".to_string());

    table.add_row(vec![
        Cell::new("Node")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(&node_0_info).fg(Color::Green),
        Cell::new(&node_1_info).fg(Color::Yellow),
    ]);

    // Add separator line after subheader
    table.add_row(vec![
        Cell::new("─".repeat(15)).fg(Color::DarkGrey),
        Cell::new("─".repeat(25)).fg(Color::DarkGrey),
        Cell::new("─".repeat(25)).fg(Color::DarkGrey),
    ]);

    // Status rows with labels on the left
    table.add_row(vec![
        Cell::new("Connection")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(format_connection_status_plain(node_0_status)),
        Cell::new(format_connection_status_plain(node_1_status)),
    ]);

    table.add_row(vec![
        Cell::new("Process")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(format_process_status_plain(node_0_status)),
        Cell::new(format_process_status_plain(node_1_status)),
    ]);

    table.add_row(vec![
        Cell::new("Disk Usage")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(format_disk_usage_plain(node_0_status)),
        Cell::new(format_disk_usage_plain(node_1_status)),
    ]);

    table.add_row(vec![
        Cell::new("Identity")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(format_identity_status(&node_0_status.current_identity)),
        Cell::new(format_identity_status(&node_1_status.current_identity)),
    ]);

    table.add_row(vec![
        Cell::new("System Load")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(format_system_load_plain(node_0_status)),
        Cell::new(format_system_load_plain(node_1_status)),
    ]);

    table.add_row(vec![
        Cell::new("Sync Status")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(format_sync_status_plain(node_0_status)),
        Cell::new(format_sync_status_plain(node_1_status)),
    ]);

    table.add_row(vec![
        Cell::new("Version")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(format_version_plain(node_0_status)),
        Cell::new(format_version_plain(node_1_status)),
    ]);

    // Add executable paths
    let node_0_agave = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.agave_validator_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_agave = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.agave_validator_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Agave Executable")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_agave),
        Cell::new(node_1_agave),
    ]);

    // Add Solana CLI executable row
    let node_0_solana = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.solana_cli_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_solana = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.solana_cli_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Solana CLI")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_solana),
        Cell::new(node_1_solana),
    ]);

    let node_0_fdctl = validator_status
        .nodes_with_status
        .get(0)
        .and_then(|n| n.fdctl_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());
    let node_1_fdctl = validator_status
        .nodes_with_status
        .get(1)
        .and_then(|n| n.fdctl_executable.as_ref())
        .map(|path| truncate_path(path, 30))
        .unwrap_or("N/A".to_string());

    table.add_row(vec![
        Cell::new("Fdctl Executable")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(node_0_fdctl),
        Cell::new(node_1_fdctl),
    ]);

    table.add_row(vec![
        Cell::new("Swap Ready")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new(format_swap_readiness_plain(node_0_status)),
        Cell::new(format_swap_readiness_plain(node_1_status)),
    ]);

    // Add swap checklist as sub-rows
    let node_0_checklist = format_swap_checklist(node_0_status);
    let node_1_checklist = format_swap_checklist(node_1_status);

    if !node_0_checklist.is_empty() || !node_1_checklist.is_empty() {
        let max_lines = node_0_checklist.len().max(node_1_checklist.len());
        for i in 0..max_lines {
            let node_0_item = node_0_checklist.get(i).cloned().unwrap_or_default();
            let node_1_item = node_1_checklist.get(i).cloned().unwrap_or_default();

            let left_label = if i == 0 { "  └ Checklist" } else { "" };

            table.add_row(vec![
                Cell::new(left_label).fg(Color::DarkGrey),
                Cell::new(node_0_item).fg(Color::DarkGrey),
                Cell::new(node_1_item).fg(Color::DarkGrey),
            ]);
        }
    }

    println!("{}", table);

    // Show verification issues if any
    if !node_0_status.verification_issues.is_empty() {
        println!("\n{} Node 0 Verification Issues:", "⚠️".yellow());
        for issue in &node_0_status.verification_issues {
            println!("  • {}", issue.yellow());
        }
    }

    if !node_1_status.verification_issues.is_empty() {
        println!("\n{} Node 1 Verification Issues:", "⚠️".yellow());
        for issue in &node_1_status.verification_issues {
            println!("  • {}", issue.yellow());
        }
    }
}

#[allow(dead_code)]
fn display_all_nodes_table(_config: &Config, results: &HashMap<String, ComprehensiveStatus>) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_BORDERS_ONLY)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Create a 3-column layout for single nodes
    let nodes: Vec<_> = results.iter().collect();

    if nodes.len() == 1 {
        // Single node - use the same layout as primary/backup but with one column
        let (role, status) = nodes[0];
        // For now, just handle the case where we have a single result
        // This is a temporary fix since we changed the structure
        let _node_config: Option<&crate::types::NodeConfig> = None; // Will be fixed when we fully migrate
        let node_label = "Node".to_string(); // Temporary fix

        table.add_row(vec![
            Cell::new("").add_attribute(Attribute::Bold),
            Cell::new(role.to_uppercase())
                .add_attribute(Attribute::Bold)
                .fg(Color::Green),
        ]);

        table.add_row(vec![
            Cell::new("Node")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new(&node_label).fg(Color::Green),
        ]);

        table.add_row(vec![
            Cell::new("Connection")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new(format_connection_status_plain(status)),
        ]);

        table.add_row(vec![
            Cell::new("Process")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new(format_process_status_plain(status)),
        ]);

        table.add_row(vec![
            Cell::new("Disk Usage")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new(format_disk_usage_plain(status)),
        ]);

        table.add_row(vec![
            Cell::new("System Load")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new(format_system_load_plain(status)),
        ]);

        table.add_row(vec![
            Cell::new("Sync Status")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new(format_sync_status_plain(status)),
        ]);

        table.add_row(vec![
            Cell::new("Version")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new(format_version_plain(status)),
        ]);

        table.add_row(vec![
            Cell::new("Swap Ready")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
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
            Cell::new("Node")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Connection")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Process")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Disk")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Load")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Sync")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Version")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
            Cell::new("Swap Ready")
                .add_attribute(Attribute::Bold)
                .fg(Color::Cyan),
        ]);

        for (_role, status) in results {
            // Temporary fix for changed structure
            let node_label = "Node".to_string(); // Temporary fix

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

#[allow(dead_code)]
fn display_other_nodes_table(_config: &Config, other_nodes: &[(&String, &ComprehensiveStatus)]) {
    let mut table = Table::new();
    table
        .load_preset(UTF8_BORDERS_ONLY)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::Dynamic);

    // Header
    table.add_row(vec![
        Cell::new("Node")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("Connection")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("Process")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("Disk")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("Load")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("Sync")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("Version")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
        Cell::new("Swap Ready")
            .add_attribute(Attribute::Bold)
            .fg(Color::Cyan),
    ]);

    // Data rows
    for (_role, status) in other_nodes {
        // Temporary fix for changed structure
        let node_label = "Node".to_string(); // Temporary fix

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
#[allow(dead_code)]
fn format_connection_status_plain(status: &ComprehensiveStatus) -> String {
    if status.connected {
        "✅ Connected".to_string()
    } else {
        "❌ Failed".to_string()
    }
}

#[allow(dead_code)]
fn format_process_status_plain(status: &ComprehensiveStatus) -> String {
    match &status.validator_running {
        Some(true) => "✅ Running".to_string(),
        Some(false) => "❌ Stopped".to_string(),
        None => "❓ Unknown".to_string(),
    }
}

#[allow(dead_code)]
fn format_disk_usage_plain(status: &ComprehensiveStatus) -> String {
    status
        .ledger_disk_usage
        .map(|d| format!("{}%", d))
        .unwrap_or_else(|| "N/A".to_string())
}

#[allow(dead_code)]
fn format_system_load_plain(status: &ComprehensiveStatus) -> String {
    status
        .system_load
        .map(|l| format!(" {:.1}", l))
        .unwrap_or_else(|| " N/A".to_string())
}

#[allow(dead_code)]
fn format_sync_status_plain(status: &ComprehensiveStatus) -> String {
    status
        .sync_status
        .as_ref()
        .map(|s| format!(" {}", s))
        .unwrap_or_else(|| " N/A".to_string())
}

#[allow(dead_code)]
fn format_version_plain(status: &ComprehensiveStatus) -> String {
    status
        .version
        .as_ref()
        .map(|v| v.clone())
        .unwrap_or_else(|| "N/A".to_string())
}

#[allow(dead_code)]
fn format_swap_readiness_plain(status: &ComprehensiveStatus) -> String {
    match status.swap_ready {
        Some(true) => "✅ Ready".to_string(),
        Some(false) => "❌ Not Ready".to_string(),
        None => "❓ Unknown".to_string(),
    }
}

#[allow(dead_code)]
fn format_verification_status(verified: Option<bool>) -> String {
    match verified {
        Some(true) => "✅ Verified".to_string(),
        Some(false) => "❌ Failed".to_string(),
        None => "⏳ Checking".to_string(),
    }
}

#[allow(dead_code)]
fn format_identity_status(identity: &Option<String>) -> String {
    match identity {
        Some(id) => {
            if id.len() > 20 {
                format!("{}...{}", &id[..8], &id[id.len() - 8..])
            } else {
                id.clone()
            }
        }
        None => "N/A".to_string(),
    }
}

#[allow(dead_code)]
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
#[allow(dead_code)]
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
    identity_verified: Option<bool>,
    vote_account_verified: Option<bool>,
    verification_issues: Vec<String>,
    error: Option<String>,
    current_identity: Option<String>, // Identity pubkey from catchup command
}

impl ComprehensiveStatus {
    #[allow(dead_code)]
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
            identity_verified: None,
            vote_account_verified: None,
            verification_issues: Vec::new(),
            error: Some(error),
            current_identity: None,
        }
    }
}

#[allow(dead_code)]
async fn verify_public_keys(
    pool: &mut crate::ssh::AsyncSshPool,
    node: &NodeConfig,
    ssh_key_path: &str,
    validator_pair: &crate::types::ValidatorPair,
    status: &mut ComprehensiveStatus,
) {
    // Verify Identity Pubkey (funded account public key)
    if let Ok(output) = pool
        .execute_command(
            node,
            ssh_key_path,
            &format!("{} address -k {}", "solana", node.paths.funded_identity),
        )
        .await
    {
        let actual_identity = output.trim();
        if actual_identity == validator_pair.identity_pubkey {
            status.identity_verified = Some(true);
        } else {
            status.identity_verified = Some(false);
            status.verification_issues.push(format!(
                "Identity Pubkey mismatch: expected {}, found {}",
                validator_pair.identity_pubkey, actual_identity
            ));
        }
    } else {
        status.identity_verified = Some(false);
        status
            .verification_issues
            .push("Could not verify Identity Pubkey - failed to read funded keypair".to_string());
    }

    // Verify Vote Pubkey
    if let Ok(output) = pool
        .execute_command(
            node,
            ssh_key_path,
            &format!("{} address -k {}", "solana", node.paths.vote_keypair),
        )
        .await
    {
        let actual_vote_account = output.trim();
        if actual_vote_account == validator_pair.vote_pubkey {
            status.vote_account_verified = Some(true);
        } else {
            status.vote_account_verified = Some(false);
            status.verification_issues.push(format!(
                "Vote Pubkey mismatch: expected {}, found {}",
                validator_pair.vote_pubkey, actual_vote_account
            ));
        }
    } else {
        status.vote_account_verified = Some(false);
        status
            .verification_issues
            .push("Could not verify Vote Pubkey - failed to read vote keypair".to_string());
    }
}

fn truncate_path(path: &str, max_length: usize) -> String {
    if path.len() <= max_length {
        path.to_string()
    } else {
        let start = if path.len() > max_length - 3 {
            path.len() - (max_length - 3)
        } else {
            0
        };
        format!("...{}", &path[start..])
    }
}

#[allow(dead_code)]
fn count_table_lines(app_state: &AppState) -> usize {
    // Count the number of lines in the table
    // Each validator has: header + status rows
    let mut lines = 0;
    for _ in app_state.validator_statuses.iter() {
        lines += 4; // Validator header lines
        lines += 11; // Fixed table rows (Node, separator, Status, Agave, Solana CLI, Fdctl, Version, Sync, Identity, Ledger, Swap)
        lines += 1; // Vote Status row
        lines += 1; // Empty line between validators
    }
    lines
}

#[allow(dead_code)]
async fn display_vote_status_row_only(app_state: &AppState) -> Result<()> {
    // Only update the vote status for each validator
    for validator_status in app_state.validator_statuses.iter() {
        let validator_pair = &validator_status.validator_pair;

        // Fetch vote account data from RPC
        let vote_data = fetch_vote_account_data(&validator_pair.rpc, &validator_pair.vote_pubkey)
            .await
            .ok();

        if let Some(vote_data) = vote_data {
            let voting_status = if vote_data.is_voting {
                "✅ Voting"
            } else {
                "⚠️ Not Voting"
            };

            let vote_info = if let Some(recent_vote) = vote_data.recent_votes.first() {
                let current_slot = vote_data.vote_account_info.current_slot.unwrap_or(0);
                let diff = current_slot.saturating_sub(recent_vote.slot);
                format!("(-{})", diff)
            } else {
                "No recent votes".to_string()
            };

            // Print the updated row
            print!(
                "│ {:14} │ {:24} │ {:24} │",
                "Vote Status".cyan().bold(),
                if vote_data.is_voting {
                    voting_status.green()
                } else {
                    voting_status.yellow()
                },
                vote_info
            );
        }
    }

    Ok(())
}
