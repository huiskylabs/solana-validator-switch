use crate::commands::error_handler::ProgressSpinner;
use anyhow::{anyhow, Result};
use colored::*;
use std::io::Write;
use std::sync::Arc;
use std::time::{Duration, Instant};

// Check if we're in silent mode (called from Telegram)
fn is_silent_mode() -> bool {
    std::env::var("SVS_SILENT_MODE").unwrap_or_default() == "1"
}

// Macro for conditional printing
macro_rules! println_if_not_silent {
    ($($arg:tt)*) => {
        if !is_silent_mode() {
            println!($($arg)*);
        }
    };
}

// Wrapper for progress spinner that respects silent mode
struct ConditionalSpinner {
    spinner: Option<ProgressSpinner>,
}

impl ConditionalSpinner {
    fn new(message: &str) -> Self {
        Self {
            spinner: if is_silent_mode() {
                None
            } else {
                Some(ProgressSpinner::new(message))
            },
        }
    }

    fn stop_with_message(self, message: &str) {
        if let Some(spinner) = self.spinner {
            spinner.stop_with_message(message);
        }
    }
}

pub async fn switch_command(dry_run: bool, app_state: &mut crate::AppState) -> Result<bool> {
    // Clear screen and ensure clean output after menu selection
    print!("\x1B[2J\x1B[1;1H");
    std::io::stdout().flush()?;

    switch_command_with_confirmation(dry_run, app_state, !dry_run).await
}

pub async fn switch_command_with_confirmation(
    dry_run: bool,
    app_state: &mut crate::AppState,
    require_confirmation: bool,
) -> Result<bool> {
    // Validate we have at least one validator configured
    if app_state.config.validators.is_empty() {
        return Err(anyhow!("No validators configured"));
    }

    // Use the selected validator
    let validator_status = &app_state.validator_statuses[app_state.selected_validator_index];
    let validator_pair = &validator_status.validator_pair;

    // Find active and standby nodes with full status information
    let active_node_with_status = validator_status
        .nodes_with_status
        .iter()
        .find(|n| n.status == crate::types::NodeStatus::Active);
    let standby_node_with_status = validator_status
        .nodes_with_status
        .iter()
        .find(|n| n.status == crate::types::NodeStatus::Standby);

    let (active_node_with_status, standby_node_with_status) =
        match (active_node_with_status, standby_node_with_status) {
            (Some(active), Some(standby)) => (active, standby),
            _ => {
                // If we can't determine status, use the first two nodes
                if validator_status.nodes_with_status.len() < 2 {
                    return Err(anyhow!("Validator must have at least 2 nodes configured"));
                }
                (
                    &validator_status.nodes_with_status[0],
                    &validator_status.nodes_with_status[1],
                )
            }
        };

    println_if_not_silent!(
        "\n{}",
        format!(
            "🔄 Validator Switch - {} Mode",
            if dry_run { "DRY RUN" } else { "LIVE" }
        )
        .bright_cyan()
        .bold()
    );
    println_if_not_silent!("{}", "━".repeat(50).dimmed());

    if dry_run {
        println_if_not_silent!(
            "{}",
            "ℹ️  This is a DRY RUN - showing what would be executed".yellow()
        );
        println_if_not_silent!(
            "{}",
            "ℹ️  Tower file transfer will be performed to measure timing".yellow()
        );
        println_if_not_silent!();
    }

    // Targeted validation: Check only what's needed for this specific switch
    let mut validation_errors = Vec::new();
    let mut validation_warnings = Vec::new();

    // Check target (standby) node - this is critical for switch success
    if standby_node_with_status.status == crate::types::NodeStatus::Unknown {
        validation_errors.push(format!(
            "Target node {} is unreachable (SSH connection failed)",
            standby_node_with_status.node.label
        ));
    } else {
        // Since we skip swap readiness checks at startup, we need to check now
        // For standby nodes, we check all requirements except tower file
        println_if_not_silent!("🔍 Checking target node swap readiness...");

        if let Some(ssh_key) = app_state
            .detected_ssh_keys
            .get(&standby_node_with_status.node.host)
        {
            let (is_ready, issues) = crate::startup::check_node_swap_readiness(
                &app_state.ssh_pool,
                &standby_node_with_status.node,
                ssh_key,
                standby_node_with_status.ledger_path.as_ref(),
                Some(true), // is_standby = true, skip tower check
            )
            .await;

            if !is_ready {
                validation_errors.push(format!(
                    "Target node {} is not swap-ready: {}",
                    standby_node_with_status.node.label,
                    issues.join(", ")
                ));
            }
        }
    }

    // Check if we can get SSH key for target node
    if !app_state
        .detected_ssh_keys
        .contains_key(&standby_node_with_status.node.host)
    {
        validation_errors.push(format!(
            "No SSH key available for target node {}",
            standby_node_with_status.node.label
        ));
    }

    // Check source (active) node - this is preferred but not critical (emergency scenarios)
    if active_node_with_status.status == crate::types::NodeStatus::Unknown {
        validation_warnings.push(format!(
            "Source node {} is unreachable - will skip optional steps (tower copy, graceful shutdown)",
            active_node_with_status.node.label
        ));
    }
    // Skip detailed swap readiness check for source node - not critical for switch

    // Check if we can get SSH key for source node
    if !app_state
        .detected_ssh_keys
        .contains_key(&active_node_with_status.node.host)
    {
        validation_warnings.push(format!(
            "No SSH key available for source node {} - will skip optional steps",
            active_node_with_status.node.label
        ));
    }

    // Show validation results
    if !validation_errors.is_empty() {
        println_if_not_silent!("\n{}", "❌ SWITCH VALIDATION FAILED".red().bold());
        println_if_not_silent!("\nCritical issues that prevent switching:\n");
        for error in &validation_errors {
            println_if_not_silent!("  • {}", error.red());
        }
        println_if_not_silent!(
            "\n{}",
            "Please resolve these issues before attempting to switch.".yellow()
        );
        return Err(anyhow::anyhow!(
            "Switch validation failed: {} critical issue(s)",
            validation_errors.len()
        ));
    }

    if !validation_warnings.is_empty() {
        println_if_not_silent!("\n{}", "⚠️  SWITCH WARNINGS".yellow().bold());
        println_if_not_silent!("\nNon-critical issues (switch will continue with limitations):\n");
        for warning in &validation_warnings {
            println_if_not_silent!("  • {}", warning.yellow());
        }

        if require_confirmation && !dry_run {
            println_if_not_silent!(
                "\n{}",
                "Do you want to continue with the switch despite these warnings?".bright_yellow()
            );

            // Actually wait for ANY key press, not just Enter
            use crossterm::event::{self, Event};
            crossterm::terminal::enable_raw_mode().ok();
            loop {
                if let Ok(Event::Key(_)) = event::read() {
                    break;
                }
            }
            crossterm::terminal::disable_raw_mode().ok();
        }
        println_if_not_silent!();
    }

    println_if_not_silent!("✅ Switch validation passed - proceeding with operation\n");

    let mut switch_manager = SwitchManager::new(
        active_node_with_status.clone(),
        standby_node_with_status.clone(),
        validator_pair.clone(),
        app_state.ssh_pool.clone(),
        app_state.detected_ssh_keys.clone(),
    );

    // Pre-warm SSH connections to both nodes for faster switching
    if !dry_run {
        let spinner = ConditionalSpinner::new("Pre-warming SSH connections...");

        // Get SSH keys for both nodes
        let active_ssh_key = app_state
            .detected_ssh_keys
            .get(&active_node_with_status.node.host)
            .ok_or_else(|| anyhow!("No SSH key detected for active node"))?;
        let standby_ssh_key = app_state
            .detected_ssh_keys
            .get(&standby_node_with_status.node.host)
            .ok_or_else(|| anyhow!("No SSH key detected for standby node"))?;

        // Pre-warm both connections (they'll be reused from the pool during switch)
        {
            let pool = app_state.ssh_pool.clone();
            // Trigger connection creation for both nodes
            let _ = pool
                .get_session(&active_node_with_status.node, active_ssh_key)
                .await?;
            let _ = pool
                .get_session(&standby_node_with_status.node, standby_ssh_key)
                .await?;
        }

        spinner.stop_with_message("✅ SSH connections ready");
    }

    // Execute the switch process
    let switch_result = switch_manager
        .execute_switch(dry_run, require_confirmation)
        .await;

    // Send Telegram notification for switch result (only for live switches)
    if !dry_run {
        if let Some(alert_config) = &app_state.config.alert_config {
            let alert_manager = crate::alert::AlertManager::new(alert_config.clone());

            match &switch_result {
                Ok(_) => {
                    // Send success notification
                    let _ = alert_manager
                        .send_switch_result(
                            true,
                            &active_node_with_status.node.label,
                            &standby_node_with_status.node.label,
                            switch_manager.identity_switch_time,
                            None,
                        )
                        .await;
                }
                Err(e) => {
                    // Send failure notification
                    let _ = alert_manager
                        .send_switch_result(
                            false,
                            &active_node_with_status.node.label,
                            &standby_node_with_status.node.label,
                            None,
                            Some(&e.to_string()),
                        )
                        .await;
                }
            }
        }
    }

    // Re-check the result and propagate any error
    let show_status = switch_result?;

    // Show completion message with timing breakdown
    if !dry_run {
        if let Some(total_time) = switch_manager.identity_switch_time {
            println_if_not_silent!("\n{}", "━".repeat(50).dimmed());
            println_if_not_silent!(
                "{} {}",
                "✅ Validator swap completed successfully in"
                    .bright_green()
                    .bold(),
                format!("{}ms", total_time.as_millis())
                    .bright_yellow()
                    .bold()
            );

            // Show timing breakdown
            println_if_not_silent!("\n{}", "📊 Timing breakdown:".dimmed());
            if let Some(active_time) = switch_manager.active_switch_time {
                println_if_not_silent!(
                    "   Step 1 - Active → Unfunded:  {}",
                    format!("{}ms", active_time.as_millis()).bright_yellow()
                );
            }
            if let Some(tower_time) = switch_manager.tower_transfer_time {
                println_if_not_silent!(
                    "   Step 2 - Tower transfer:     {}",
                    format!("{}ms", tower_time.as_millis()).bright_yellow()
                );
            }
            if let Some(standby_time) = switch_manager.standby_switch_time {
                println_if_not_silent!(
                    "   Step 3 - Standby → Funded:   {}",
                    format!("{}ms", standby_time.as_millis()).bright_yellow()
                );
            }
        } else {
            println_if_not_silent!(
                "\n{}",
                "✅ Validator swap completed successfully"
                    .bright_green()
                    .bold()
            );
        }

        // Update the node statuses in app_state to reflect the switch
        if !dry_run && show_status && app_state.validator_statuses.len() > 0 {
            // Find the indices of active and standby nodes
            let mut active_idx = None;
            let mut standby_idx = None;

            for (idx, node_with_status) in app_state.validator_statuses
                [app_state.selected_validator_index]
                .nodes_with_status
                .iter()
                .enumerate()
            {
                match node_with_status.status {
                    crate::types::NodeStatus::Active => active_idx = Some(idx),
                    crate::types::NodeStatus::Standby => standby_idx = Some(idx),
                    _ => {}
                }
            }

            // Swap the statuses
            if let (Some(active), Some(standby)) = (active_idx, standby_idx) {
                app_state.validator_statuses[app_state.selected_validator_index]
                    .nodes_with_status[active]
                    .status = crate::types::NodeStatus::Standby;
                app_state.validator_statuses[app_state.selected_validator_index]
                    .nodes_with_status[standby]
                    .status = crate::types::NodeStatus::Active;
            }
        }

        println_if_not_silent!();
        println_if_not_silent!("{}", "Press any key to view status...".dimmed());
        if !is_silent_mode() {
            // Actually wait for ANY key press, not just Enter
            use crossterm::event::{self, Event};
            crossterm::terminal::enable_raw_mode().ok();
            loop {
                if let Ok(Event::Key(_)) = event::read() {
                    break;
                }
            }
            crossterm::terminal::disable_raw_mode().ok();
        }
    }

    Ok(show_status)
}

pub(crate) struct SwitchManager {
    active_node_with_status: crate::types::NodeWithStatus,
    standby_node_with_status: crate::types::NodeWithStatus,
    #[allow(dead_code)]
    validator_pair: crate::types::ValidatorPair,
    ssh_pool: Arc<crate::ssh::AsyncSshPool>,
    detected_ssh_keys: std::collections::HashMap<String, String>,
    tower_file_name: Option<String>,
    tower_transfer_time: Option<Duration>,
    identity_switch_time: Option<Duration>,
    active_switch_time: Option<Duration>,
    standby_switch_time: Option<Duration>,
}

impl SwitchManager {
    pub(crate) fn new(
        active_node_with_status: crate::types::NodeWithStatus,
        standby_node_with_status: crate::types::NodeWithStatus,
        validator_pair: crate::types::ValidatorPair,
        ssh_pool: Arc<crate::ssh::AsyncSshPool>,
        detected_ssh_keys: std::collections::HashMap<String, String>,
    ) -> Self {
        Self {
            active_node_with_status,
            standby_node_with_status,
            validator_pair,
            ssh_pool,
            detected_ssh_keys,
            tower_file_name: None,
            tower_transfer_time: None,
            identity_switch_time: None,
            active_switch_time: None,
            standby_switch_time: None,
        }
    }

    fn get_ssh_key_for_node(&self, host: &str) -> Result<String> {
        // Use detected key if available
        self.detected_ssh_keys
            .get(host)
            .cloned()
            .ok_or_else(|| anyhow!("No SSH key detected for host: {}", host))
    }

    async fn execute_switch(&mut self, dry_run: bool, require_confirmation: bool) -> Result<bool> {
        // Show confirmation dialog (except for dry run or when explicitly disabled)
        if !dry_run && require_confirmation {
            println!(
                "\n{}",
                "⚠️  Validator Switch Confirmation".bright_yellow().bold()
            );
            println!("{}", "━".repeat(50).dimmed());
            println!();
            println!(
                "  {} → {}",
                format!(
                    "🟢 ACTIVE: {} ({}) {}",
                    self.active_node_with_status.node.label,
                    self.active_node_with_status.node.host,
                    self.active_node_with_status
                        .version
                        .as_ref()
                        .unwrap_or(&"Unknown".to_string())
                )
                .bright_green(),
                "🔄 STANDBY".dimmed()
            );
            println!(
                "  {} → {}",
                format!(
                    "⚪ STANDBY: {} ({}) {}",
                    self.standby_node_with_status.node.label,
                    self.standby_node_with_status.node.host,
                    self.standby_node_with_status
                        .version
                        .as_ref()
                        .unwrap_or(&"Unknown".to_string())
                )
                .white(),
                "🟢 ACTIVE".bright_green()
            );
            println!();
            println!(
                "  {}",
                "This will switch your validator identity between nodes.".yellow()
            );
            println!("  {}", "Estimated time: ~10 seconds".dimmed());
            println!();

            // Use inquire for confirmation
            use inquire::Confirm;
            let confirmed = Confirm::new("Do you want to proceed with the validator switch?")
                .with_default(false)
                .prompt()?;

            if !confirmed {
                println!("\n{}", "❌ Validator switch cancelled by user".red());
                return Ok(false);
            }
            println!();
            // Ensure output is flushed after confirmation
            std::io::stdout().flush()?;
        }

        // Start timing the entire switch operation
        let total_switch_start = Instant::now();

        // Step 1: Switch active node to unfunded identity
        println_if_not_silent!(
            "\n{}",
            "🔄 Step 1: Switch Active Node to Unfunded Identity"
                .bright_blue()
                .bold()
        );
        let active_switch_start = Instant::now();
        self.switch_primary_to_unfunded(dry_run).await?;
        self.active_switch_time = Some(active_switch_start.elapsed());
        if !dry_run {
            println_if_not_silent!(
                "   ✓ Completed in {}",
                format!("{}ms", self.active_switch_time.unwrap().as_millis())
                    .bright_yellow()
                    .bold()
            );
        }

        // Step 2: Transfer tower file
        println_if_not_silent!(
            "\n{}",
            "📤 Step 2: Transfer Tower File".bright_blue().bold()
        );
        self.transfer_tower_file(dry_run).await?;
        // Note: tower_transfer_time is set inside transfer_tower_file method

        // Step 3: Switch standby node to funded identity
        println_if_not_silent!(
            "\n{}",
            "🚀 Step 3: Switch Standby Node to Funded Identity"
                .bright_blue()
                .bold()
        );
        let standby_switch_start = Instant::now();
        self.switch_backup_to_funded(dry_run).await?;
        self.standby_switch_time = Some(standby_switch_start.elapsed());
        if !dry_run {
            println_if_not_silent!(
                "   ✓ Completed in {}",
                format!("{}ms", self.standby_switch_time.unwrap().as_millis())
                    .bright_yellow()
                    .bold()
            );
        }

        // Record total identity switch time
        if !dry_run {
            self.identity_switch_time = Some(total_switch_start.elapsed());
        }

        // Step 4: Verify new active node health (former standby)
        println_if_not_silent!(
            "\n{}",
            "✅ Step 4: Verify New Active Node (Former Standby)"
                .bright_blue()
                .bold()
        );
        self.verify_backup_catchup(dry_run).await?;

        // Summary
        self.print_summary(dry_run);

        Ok(!dry_run)
    }

    pub(crate) async fn switch_primary_to_unfunded(&mut self, dry_run: bool) -> Result<()> {
        // Detect validator type to use appropriate command
        let process_info = {
            let ssh_key = self.get_ssh_key_for_node(&self.active_node_with_status.node.host)?;
            let pool = self.ssh_pool.clone();
            pool.execute_command(
                &self.active_node_with_status.node,
                &ssh_key,
                "ps aux | grep -E 'solana-validator|agave|fdctl|firedancer' | grep -v grep",
            )
            .await?
        };

        let (subtitle, switch_command) = if process_info.contains("fdctl")
            || process_info.contains("firedancer")
        {
            // Use detected fdctl executable path, or extract from running process as fallback
            let fdctl_path = if let Some(ref path) = self.active_node_with_status.fdctl_executable {
                path.clone()
            } else {
                // Fallback: extract fdctl path from running process
                process_info
                    .lines()
                    .find(|line| line.contains("fdctl"))
                    .and_then(|line| {
                        line.split_whitespace()
                            .find(|part| part.contains("fdctl") && (part.ends_with("fdctl") || part.contains("/fdctl")))
                            .map(|s| s.to_string())
                    })
                    .ok_or_else(|| anyhow!("Firedancer fdctl executable path not found in node status or running process"))?
            };

            // Extract config path from the process info (e.g., "fdctl run --config /path/to/config.toml")
            let config_path = if let Some(config_match) = process_info
                .lines()
                .find(|line| line.contains("fdctl") && line.contains("--config"))
                .and_then(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    parts
                        .windows(2)
                        .find(|w| w[0] == "--config")
                        .map(|w| w[1].to_string())
                }) {
                config_match
            } else {
                return Err(anyhow!("Firedancer config path not found in running process. Please ensure fdctl is running with --config parameter"));
            };

            (
                "Using Firedancer fdctl set-identity",
                format!(
                    "{} set-identity --config \"{}\" \"{}\"",
                    fdctl_path,
                    config_path,
                    self.active_node_with_status.node.paths.unfunded_identity
                ),
            )
        } else if process_info.contains("agave-validator") {
            // Use detected agave executable path if available
            let agave_path = self
                .active_node_with_status
                .agave_validator_executable
                .as_ref()
                .ok_or_else(|| anyhow!("Agave validator executable path not found"))?;

            // Use detected ledger path if available, otherwise error
            let ledger_path = self
                .active_node_with_status
                .ledger_path
                .as_ref()
                .ok_or_else(|| anyhow!("Ledger path not detected for active node"))?;

            (
                "Using Agave validator set-identity",
                format!(
                    "{} -l \"{}\" set-identity \"{}\"",
                    agave_path,
                    ledger_path,
                    self.active_node_with_status.node.paths.unfunded_identity
                ),
            )
        } else {
            // Use detected ledger path if available, otherwise error
            let ledger_path = self
                .active_node_with_status
                .ledger_path
                .as_ref()
                .ok_or_else(|| anyhow!("Ledger path not detected for active node"))?;

            (
                "Using Solana validator restart",
                format!("{} exit && solana-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                    "solana-validator",  // Using validator binary directly instead of solana CLI
                    self.active_node_with_status.node.paths.unfunded_identity,
                    self.active_node_with_status.node.paths.vote_keypair,
                    ledger_path)
            )
        };

        println_if_not_silent!("{}", subtitle.dimmed());
        println_if_not_silent!(
            "ssh {}@{} '{}'",
            self.active_node_with_status.node.user,
            self.active_node_with_status.node.host,
            switch_command
        );

        if !dry_run {
            let spinner =
                ConditionalSpinner::new("Switching active validator to unfunded identity...");
            {
                let ssh_key = self.get_ssh_key_for_node(&self.active_node_with_status.node.host)?;
                let pool = self.ssh_pool.clone();

                // Execute the switch command based on validator type
                if process_info.contains("fdctl") || process_info.contains("firedancer") {
                    // Firedancer: fdctl set-identity --config <config> <identity>
                    let fdctl_path = if let Some(ref path) =
                        self.active_node_with_status.fdctl_executable
                    {
                        path.clone()
                    } else {
                        // Fallback: extract fdctl path from running process
                        process_info
                            .lines()
                            .find(|line| line.contains("fdctl"))
                            .and_then(|line| {
                                line.split_whitespace()
                                    .find(|part| part.contains("bin/fdctl"))
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_else(|| panic!("Firedancer fdctl executable path not found"))
                    };
                    let config_path = process_info
                        .lines()
                        .find(|line| line.contains("fdctl") && line.contains("--config"))
                        .and_then(|line| {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            parts.windows(2).find(|w| w[0] == "--config").map(|w| w[1])
                        })
                        .unwrap();

                    let args = vec![
                        "set-identity",
                        "--config",
                        config_path,
                        &self.active_node_with_status.node.paths.unfunded_identity,
                    ];

                    pool.execute_command_with_args(
                        &self.active_node_with_status.node,
                        &ssh_key,
                        &fdctl_path,
                        &args,
                    )
                    .await?;
                } else if process_info.contains("agave-validator") {
                    // Agave: agave-validator -l <ledger> set-identity <identity>
                    let agave_path = self
                        .active_node_with_status
                        .agave_validator_executable
                        .as_ref()
                        .unwrap();
                    let ledger_path = self.active_node_with_status.ledger_path.as_ref().unwrap();

                    let args = vec![
                        "-l",
                        ledger_path,
                        "set-identity",
                        &self.active_node_with_status.node.paths.unfunded_identity,
                    ];

                    pool.execute_command_with_args(
                        &self.active_node_with_status.node,
                        &ssh_key,
                        agave_path,
                        &args,
                    )
                    .await?;
                } else {
                    return Err(anyhow!("Unsupported validator type for set-identity"));
                }
            }
            // No sleep - move immediately to next step!
            spinner.stop_with_message("✅ Active validator switched to unfunded identity");
        }

        Ok(())
    }

    pub(crate) async fn transfer_tower_file(&mut self, dry_run: bool) -> Result<()> {
        // Use the derived tower path from active node
        let tower_path = self
            .active_node_with_status
            .tower_path
            .as_ref()
            .ok_or_else(|| anyhow!("Tower path not available for active node"))?;

        // Verify the tower file exists
        let check_tower_cmd = format!("test -f {} && echo 'exists' || echo 'missing'", tower_path);
        let tower_exists = {
            let ssh_key = self.get_ssh_key_for_node(&self.active_node_with_status.node.host)?;
            let pool = self.ssh_pool.clone();
            pool.execute_command(
                &self.active_node_with_status.node,
                &ssh_key,
                &check_tower_cmd,
            )
            .await?
        };

        if tower_exists.trim() != "exists" {
            return Err(anyhow!(
                "Tower file not found on active node: {}",
                tower_path
            ));
        }

        let tower_filename = tower_path.split('/').last().unwrap_or("tower.bin");
        self.tower_file_name = Some(tower_filename.to_string());

        // Use detected ledger path if available, otherwise error
        let standby_ledger_path = self
            .standby_node_with_status
            .ledger_path
            .as_ref()
            .ok_or_else(|| anyhow!("Ledger path not detected for standby node"))?;

        let dest_path = format!("{}/{}", standby_ledger_path, tower_filename);

        println_if_not_silent!(
            "  📤 {}@{} → {}@{}",
            self.active_node_with_status.node.user,
            self.active_node_with_status.node.host,
            self.standby_node_with_status.node.user,
            self.standby_node_with_status.node.host
        );

        let start_time = Instant::now();

        // Execute the streaming transfer using base64 encoding
        let encoded_data = if !dry_run {
            let spinner = ConditionalSpinner::new("Reading tower file...");
            let ssh_key_active =
                self.get_ssh_key_for_node(&self.active_node_with_status.node.host)?;
            let data = {
                let pool = self.ssh_pool.clone();
                let base64_args = vec![tower_path.as_str()];
                match pool
                    .execute_command_with_args(
                        &self.active_node_with_status.node,
                        &ssh_key_active,
                        "base64",
                        &base64_args,
                    )
                    .await
                {
                    Ok(data) => data,
                    Err(e) => {
                        spinner.stop_with_message(&format!("❌ Failed to read tower file: {}", e));
                        return Err(anyhow!("Failed to read tower file: {}", e));
                    }
                }
            };
            spinner.stop_with_message("");

            let spinner = ConditionalSpinner::new("Transferring tower file...");
            let ssh_key_standby =
                self.get_ssh_key_for_node(&self.standby_node_with_status.node.host)?;
            {
                let pool = self.ssh_pool.clone();
                match pool
                    .transfer_base64_to_file(
                        &self.standby_node_with_status.node,
                        &ssh_key_standby,
                        &dest_path,
                        &data,
                    )
                    .await
                {
                    Ok(_) => {}
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

        println_if_not_silent!(
            "  ✅ Transferred in {} ({:.2} MB/s)",
            format!("{}ms", transfer_duration.as_millis())
                .bright_green()
                .bold(),
            speed_mbps
        );

        if !dry_run {
            // Verify the file on standby
            let verify_result = {
                let ssh_key =
                    self.get_ssh_key_for_node(&self.standby_node_with_status.node.host)?;
                let pool = self.ssh_pool.clone();
                let ls_args = vec!["-la", &dest_path];
                pool.execute_command_with_args(
                    &self.standby_node_with_status.node,
                    &ssh_key,
                    "ls",
                    &ls_args,
                )
                .await?
            };
            if verify_result.trim().is_empty() {
                return Err(anyhow!("Failed to verify tower file on standby"));
            }
        }

        Ok(())
    }

    pub(crate) async fn switch_backup_to_funded(&mut self, dry_run: bool) -> Result<()> {
        // Detect validator type to use appropriate command
        let process_info = {
            let ssh_key = self.get_ssh_key_for_node(&self.standby_node_with_status.node.host)?;
            let pool = self.ssh_pool.clone();
            pool.execute_command(
                &self.standby_node_with_status.node,
                &ssh_key,
                "ps aux | grep -E 'solana-validator|agave|fdctl|firedancer' | grep -v grep",
            )
            .await?
        };

        let (subtitle, switch_command) = if process_info.contains("fdctl")
            || process_info.contains("firedancer")
        {
            // Use detected fdctl executable path, or extract from running process as fallback
            let fdctl_path = if let Some(ref path) = self.standby_node_with_status.fdctl_executable
            {
                path.clone()
            } else {
                // Fallback: extract fdctl path from running process
                process_info
                    .lines()
                    .find(|line| line.contains("fdctl"))
                    .and_then(|line| {
                        line.split_whitespace()
                            .find(|part| part.contains("fdctl") && (part.ends_with("fdctl") || part.contains("/fdctl")))
                            .map(|s| s.to_string())
                    })
                    .ok_or_else(|| anyhow!("Firedancer fdctl executable path not found in node status or running process"))?
            };

            // Extract config path from the process info (e.g., "fdctl run --config /path/to/config.toml")
            let config_path = if let Some(config_match) = process_info
                .lines()
                .find(|line| line.contains("fdctl") && line.contains("--config"))
                .and_then(|line| {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    parts
                        .windows(2)
                        .find(|w| w[0] == "--config")
                        .map(|w| w[1].to_string())
                }) {
                config_match
            } else {
                return Err(anyhow!("Firedancer config path not found in running process. Please ensure fdctl is running with --config parameter"));
            };

            (
                "Using Firedancer fdctl set-identity",
                format!(
                    "{} set-identity --config \"{}\" \"{}\"",
                    fdctl_path,
                    config_path,
                    self.standby_node_with_status.node.paths.funded_identity
                ),
            )
        } else if process_info.contains("agave-validator") {
            // Use detected agave executable path if available
            let agave_path = self
                .standby_node_with_status
                .agave_validator_executable
                .as_ref()
                .ok_or_else(|| anyhow!("Agave validator executable path not found"))?;

            // Use detected ledger path if available, otherwise error
            let ledger_path = self
                .standby_node_with_status
                .ledger_path
                .as_ref()
                .ok_or_else(|| anyhow!("Ledger path not detected for standby node"))?;

            (
                "Using Agave validator set-identity",
                format!(
                    "{} -l \"{}\" set-identity --require-tower \"{}\"",
                    agave_path,
                    ledger_path,
                    self.standby_node_with_status.node.paths.funded_identity
                ),
            )
        } else {
            // Use detected ledger path if available, otherwise error
            let ledger_path = self
                .standby_node_with_status
                .ledger_path
                .as_ref()
                .ok_or_else(|| anyhow!("Ledger path not detected for standby node"))?;

            (
                "Using Solana validator restart",
                format!("{} exit && solana-validator --identity {} --vote-account {} --ledger {} --limit-ledger-size 100000000 --log - &", 
                    "solana-validator",  // Using validator binary directly instead of solana CLI
                    self.standby_node_with_status.node.paths.funded_identity,
                    self.standby_node_with_status.node.paths.vote_keypair,
                    ledger_path)
            )
        };

        println_if_not_silent!("{}", subtitle.dimmed());
        println_if_not_silent!(
            "ssh {}@{} '{}'",
            self.standby_node_with_status.node.user,
            self.standby_node_with_status.node.host,
            switch_command
        );

        if !dry_run {
            let spinner =
                ConditionalSpinner::new("Switching standby validator to funded identity...");
            {
                let ssh_key =
                    self.get_ssh_key_for_node(&self.standby_node_with_status.node.host)?;
                let pool = self.ssh_pool.clone();

                // Execute the switch command based on validator type
                if process_info.contains("fdctl") || process_info.contains("firedancer") {
                    // Firedancer: fdctl set-identity --config <config> <identity>
                    let fdctl_path = if let Some(ref path) =
                        self.standby_node_with_status.fdctl_executable
                    {
                        path.clone()
                    } else {
                        // Fallback: extract fdctl path from running process
                        process_info
                            .lines()
                            .find(|line| line.contains("fdctl"))
                            .and_then(|line| {
                                line.split_whitespace()
                                    .find(|part| part.contains("bin/fdctl"))
                                    .map(|s| s.to_string())
                            })
                            .unwrap_or_else(|| panic!("Firedancer fdctl executable path not found"))
                    };
                    let config_path = process_info
                        .lines()
                        .find(|line| line.contains("fdctl") && line.contains("--config"))
                        .and_then(|line| {
                            let parts: Vec<&str> = line.split_whitespace().collect();
                            parts.windows(2).find(|w| w[0] == "--config").map(|w| w[1])
                        })
                        .unwrap();

                    let args = vec![
                        "set-identity",
                        "--config",
                        config_path,
                        &self.standby_node_with_status.node.paths.funded_identity,
                    ];

                    pool.execute_command_with_args(
                        &self.standby_node_with_status.node,
                        &ssh_key,
                        &fdctl_path,
                        &args,
                    )
                    .await?;
                } else if process_info.contains("agave-validator") {
                    // Agave: agave-validator -l <ledger> set-identity --require-tower <identity>
                    let agave_path = self
                        .standby_node_with_status
                        .agave_validator_executable
                        .as_ref()
                        .unwrap();
                    let ledger_path = self.standby_node_with_status.ledger_path.as_ref().unwrap();

                    let args = vec![
                        "-l",
                        ledger_path,
                        "set-identity",
                        "--require-tower",
                        &self.standby_node_with_status.node.paths.funded_identity,
                    ];

                    pool.execute_command_with_args(
                        &self.standby_node_with_status.node,
                        &ssh_key,
                        agave_path,
                        &args,
                    )
                    .await?;
                } else {
                    return Err(anyhow!("Unsupported validator type for set-identity"));
                }
            }
            // No sleep - switch is complete!
            spinner.stop_with_message("✅ Standby validator switched to funded identity");
        }

        Ok(())
    }

    async fn verify_backup_catchup(&mut self, dry_run: bool) -> Result<()> {
        println_if_not_silent!("Verifying health status of new active validator...");

        if !dry_run {
            // No sleep - verify immediately!
            let spinner = ConditionalSpinner::new(
                "Verifying new active validator (former standby) health status...",
            );

            // Use RPC health check instead of catchup command
            let rpc_port = crate::validator_rpc::get_rpc_port(
                self.standby_node_with_status.validator_type.clone(),
                None,
            );

            let health_result = {
                let ssh_key =
                    self.get_ssh_key_for_node(&self.standby_node_with_status.node.host)?;
                let pool = self.ssh_pool.clone();

                crate::validator_rpc::get_health(
                    &pool,
                    &self.standby_node_with_status.node,
                    &ssh_key,
                    rpc_port,
                )
                .await
            };

            match health_result {
                Ok(true) => {
                    spinner.stop_with_message(
                        "✅ New active validator (former standby) is healthy with funded identity",
                    );
                }
                Ok(false) => {
                    spinner.stop_with_message(
                        "⚠️  New active validator (former standby) is not yet healthy with funded identity",
                    );
                }
                Err(e) => {
                    spinner.stop_with_message(&format!("⚠️  Health check error: {}", e));
                }
            }
        }

        Ok(())
    }

    fn print_summary(&self, dry_run: bool) {
        println_if_not_silent!();
        if dry_run {
            println_if_not_silent!("✅ Dry run completed successfully");
            println_if_not_silent!();
            println_if_not_silent!("{}", "Press any key to continue...".dimmed());
            if !is_silent_mode() {
                // Actually wait for ANY key press, not just Enter
                use crossterm::event::{self, Event};
                crossterm::terminal::enable_raw_mode().ok();
                loop {
                    if let Ok(Event::Key(_)) = event::read() {
                        break;
                    }
                }
                crossterm::terminal::disable_raw_mode().ok();
            }
        } else {
            println_if_not_silent!("✅ Validator identity switch completed successfully");
        }
    }
}

#[cfg(test)]
#[path = "switch_scenarios_test.rs"]
mod switch_scenarios_test;
