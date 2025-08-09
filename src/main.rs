#![allow(clippy::uninlined_format_args)]
#![allow(clippy::trim_split_whitespace)]
#![allow(clippy::get_first)]
#![allow(clippy::for_kv_map)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::useless_asref)]
#![allow(clippy::await_holding_lock)]
#![allow(clippy::double_ended_iterator_last)]
#![allow(clippy::new_without_default)]

use anyhow::Result;
use clap::{Parser, Subcommand};
use std::sync::Arc;

mod alert;
#[cfg(test)]
mod alert_integration_tests;
#[cfg(test)]
mod alert_logic_tests;
#[cfg(test)]
mod alert_tests;
#[cfg(test)]
mod auto_failover_tests;
mod commands;
mod config;
mod emergency_failover;
mod executable_utils;
mod solana_rpc;
mod ssh;
mod ssh_key_detector;
mod startup;
mod startup_checks;
mod startup_logger;
#[cfg(test)]
mod startup_validation_tests;
#[cfg(test)]
mod status_ui_alert_tests;
#[cfg(test)]
mod switch_validation_tests;
mod types;
mod validator_metadata;
mod validator_rpc;

use commands::{status_command, switch_command, test_alert_command};
use ssh::AsyncSshPool;

#[derive(Parser)]
#[command(name = "svs")]
#[command(about = "Solana Validator Switch - Interactive CLI for validator management")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    /// Path to custom configuration file (default: ~/.solana-validator-switch/config.yaml)
    #[arg(short, long, global = true)]
    config: Option<String>,
    
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Check current validator status
    Status {
        /// Select validator by index (0-based) or identity prefix
        #[arg(short, long)]
        validator: Option<String>,
    },
    /// Switch between primary and backup validators
    Switch {
        /// Preview switch without executing
        #[arg(short, long)]
        dry_run: bool,
        /// Select validator by index (0-based) or identity prefix
        #[arg(short, long)]
        validator: Option<String>,
    },
    /// Test alert configuration
    TestAlert,
}

/// Application state that persists throughout the CLI session
#[derive(Clone)]
pub struct AppState {
    pub ssh_pool: Arc<AsyncSshPool>,
    pub config: types::Config,
    pub validator_statuses: Vec<ValidatorStatus>,
    pub metadata_cache: Arc<tokio::sync::Mutex<validator_metadata::MetadataCache>>,
    pub detected_ssh_keys: std::collections::HashMap<String, String>, // host -> key_path mapping
    pub selected_validator_index: usize, // Currently selected validator pair
}

#[derive(Debug, Clone)]
pub struct ValidatorStatus {
    pub validator_pair: types::ValidatorPair,
    pub nodes_with_status: Vec<types::NodeWithStatus>,
    pub metadata: Option<validator_metadata::ValidatorMetadata>,
}

impl AppState {
    async fn new() -> Result<Option<Self>> {
        // Use the comprehensive startup checklist
        startup::run_startup_checklist().await
    }

    async fn new_with_config(config_path: Option<String>) -> Result<Option<Self>> {
        // Use the comprehensive startup checklist with custom config
        startup::run_startup_checklist_with_config(config_path).await
    }

    /// Parse validator selection from CLI argument
    fn select_validator_from_arg(&mut self, validator_arg: &str) -> Result<()> {
        // Try parsing as index first
        if let Ok(index) = validator_arg.parse::<usize>() {
            if index < self.validator_statuses.len() {
                self.selected_validator_index = index;
                return Ok(());
            } else {
                return Err(anyhow::anyhow!(
                    "Validator index {} out of range (max: {})",
                    index,
                    self.validator_statuses.len() - 1
                ));
            }
        }

        // Try matching by identity prefix
        let matches: Vec<(usize, &ValidatorStatus)> = self
            .validator_statuses
            .iter()
            .enumerate()
            .filter(|(_, v)| v.validator_pair.identity_pubkey.starts_with(validator_arg))
            .collect();

        match matches.len() {
            0 => Err(anyhow::anyhow!(
                "No validator found matching '{}'",
                validator_arg
            )),
            1 => {
                self.selected_validator_index = matches[0].0;
                Ok(())
            }
            _ => Err(anyhow::anyhow!(
                "Multiple validators match '{}'. Please be more specific.",
                validator_arg
            )),
        }
    }
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize app state with persistent SSH connections
    let app_state = AppState::new_with_config(cli.config).await?;

    match cli.command {
        Some(Commands::Status { validator }) => {
            if let Some(mut state) = app_state {
                // Apply validator selection if provided
                if let Some(validator_arg) = validator {
                    state.select_validator_from_arg(&validator_arg)?;
                }
                status_command(&state).await?;
            } else {
                // Startup validation already showed detailed error messages
                std::process::exit(1);
            }
        }
        Some(Commands::Switch { dry_run, validator }) => {
            if let Some(mut state) = app_state {
                // Apply validator selection if provided
                if let Some(validator_arg) = validator {
                    state.select_validator_from_arg(&validator_arg)?;
                }
                let show_status = switch_command(dry_run, &mut state).await?;
                if show_status && !dry_run {
                    status_command(&state).await?;
                }
            } else {
                // Startup validation already showed detailed error messages
                std::process::exit(1);
            }
        }
        Some(Commands::TestAlert) => {
            if let Some(state) = app_state.as_ref() {
                test_alert_command(state).await?;
            } else {
                // Startup validation already showed detailed error messages
                std::process::exit(1);
            }
        }
        None => {
            // Interactive main menu only if app state is valid
            if let Some(state) = app_state {
                show_interactive_menu(state).await?;
            } else {
                // Startup validation already showed detailed error messages
                // Exit silently to avoid redundant generic error messages
                std::process::exit(1);
            }
        }
    }

    // Note: SSH connections are kept alive for performance - they'll be cleaned up on process exit

    Ok(())
}

async fn show_interactive_menu(mut app_state: AppState) -> Result<()> {
    use colored::*;
    use inquire::Select;

    // Clear screen and show welcome like original
    println!("\x1B[2J\x1B[1;1H"); // Clear screen
    println!(
        "{}",
        "🚀 Welcome to Solana Validator Switch CLI v1.3.3"
            .bright_cyan()
            .bold()
    );
    println!(
        "{}",
        "Professional-grade validator switching from your terminal".dimmed()
    );
    println!();

    // Show current validator info if multiple validators
    if app_state.validator_statuses.len() > 1 {
        println!(
            "{}",
            format!(
                "Currently managing {} validator pairs:",
                app_state.validator_statuses.len()
            )
            .bright_yellow()
        );
        for (idx, validator_status) in app_state.validator_statuses.iter().enumerate() {
            let identity = &validator_status.validator_pair.identity_pubkey;
            let label = validator_status
                .nodes_with_status
                .first()
                .map(|n| n.node.label.clone())
                .unwrap_or_else(|| format!("Validator {}", idx + 1));
            let short_identity = if identity.len() > 8 {
                format!("{}...{}", &identity[..4], &identity[identity.len() - 4..])
            } else {
                identity.clone()
            };

            let marker = if idx == app_state.selected_validator_index {
                "▶"
            } else {
                " "
            };
            println!("{} {}. {} ({})", marker, idx + 1, label, short_identity);
        }
        println!();
    }

    loop {
        let mut options = vec![];

        // Add validator selection option if multiple validators
        if app_state.validator_statuses.len() > 1 {
            options.push("🎯 Select Validator - Choose which validator to manage");
        }

        options.extend_from_slice(&[
            "📋 Status - Check current validator status",
            "🔄 Switch - Switch between primary and backup validators",
            "🔔 Test Alert - Test alert configuration",
            "❌ Exit",
        ]);

        let selection = Select::new("What would you like to do?", options.clone()).prompt()?;

        let selected_option = selection;

        if app_state.validator_statuses.len() > 1
            && selected_option == "🎯 Select Validator - Choose which validator to manage"
        {
            select_validator(&mut app_state).await?;
        } else if selected_option == "📋 Status - Check current validator status" {
            status_command(&app_state).await?;
        } else if selected_option == "🔄 Switch - Switch between primary and backup validators" {
            show_switch_menu(&mut app_state).await?;
        } else if selected_option == "🔔 Test Alert - Test alert configuration" {
            test_alert_command(&app_state).await?;
        } else if selected_option == "❌ Exit" {
            println!("{}", "👋 Goodbye!".bright_green());
            break;
        }
    }

    Ok(())
}

async fn select_validator(app_state: &mut AppState) -> Result<()> {
    use colored::*;
    use inquire::Select;

    println!("\n{}", "🎯 Select Validator".bright_cyan().bold());

    let mut options = Vec::new();
    for (idx, validator_status) in app_state.validator_statuses.iter().enumerate() {
        let identity = &validator_status.validator_pair.identity_pubkey;
        let label = validator_status
            .nodes_with_status
            .first()
            .map(|n| n.node.label.clone())
            .unwrap_or_else(|| format!("Validator {}", idx + 1));
        let short_identity = if identity.len() > 8 {
            format!("{}...{}", &identity[..4], &identity[identity.len() - 4..])
        } else {
            identity.clone()
        };

        let option = format!("{}. {} ({})", idx + 1, label, short_identity);
        options.push(option);
    }

    let selection = Select::new("Choose a validator to manage:", options.clone()).prompt()?;

    let index = options.iter().position(|x| x == &selection).unwrap();
    app_state.selected_validator_index = index;

    println!("{}", format!("✅ Selected: {}", selection).bright_green());
    println!();

    Ok(())
}

async fn show_switch_menu(app_state: &mut AppState) -> Result<()> {
    use colored::*;
    use inquire::Select;

    loop {
        println!("\n{}", "🔄 Validator Switching".bright_cyan().bold());
        println!();

        let mut options = vec![
            "🔄 Switch - Switch between primary and backup validators",
            "🧪 Dry Run - Preview switch without executing",
        ];

        options.push("⬅️  Back to main menu");

        let selection = Select::new("Select switching action:", options.clone()).prompt()?;

        let index = options.iter().position(|x| x == &selection).unwrap();

        match index {
            0 => {
                let show_status = switch_command(false, app_state).await?;
                if show_status {
                    status_command(app_state).await?;
                }
                // After a live switch, return to main menu
                break;
            }
            1 => {
                let _ = switch_command(true, app_state).await?;
                // Dry run doesn't show status
            }
            2 => break, // Back to main menu
            _ => unreachable!(),
        }
    }

    Ok(())
}
