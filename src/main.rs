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
mod alert_tests;
mod commands;
mod config;
mod solana_rpc;
mod ssh;
mod ssh_key_detector;
mod startup;
mod startup_logger;
mod types;
mod validator_metadata;

use commands::{status_command, switch_command, test_alert_command};
use ssh::AsyncSshPool;

#[derive(Parser)]
#[command(name = "svs")]
#[command(about = "Solana Validator Switch - Interactive CLI for validator management")]
#[command(version = env!("CARGO_PKG_VERSION"))]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Check current validator status
    Status,
    /// Switch between primary and backup validators
    Switch {
        /// Preview switch without executing
        #[arg(short, long)]
        dry_run: bool,
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
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize app state with persistent SSH connections
    let app_state = AppState::new().await?;

    match cli.command {
        Some(Commands::Status) => {
            if let Some(state) = app_state.as_ref() {
                status_command(state).await?;
            } else {
                // Startup validation already showed detailed error messages
                std::process::exit(1);
            }
        }
        Some(Commands::Switch { dry_run }) => {
            if let Some(mut state) = app_state {
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
        "🚀 Welcome to Solana Validator Switch CLI v1.0.0"
            .bright_cyan()
            .bold()
    );
    println!(
        "{}",
        "Professional-grade validator switching from your terminal".dimmed()
    );
    println!();

    loop {
        let mut options = vec![
            "📋 Status - Check current validator status",
            "🔄 Switch - Switch between primary and backup validators",
            "🔔 Test Alert - Test alert configuration",
        ];

        options.push("❌ Exit");

        let selection = Select::new("What would you like to do?", options.clone()).prompt()?;

        let index = options.iter().position(|x| x == &selection).unwrap();

        match index {
            0 => {
                status_command(&app_state).await?;
            }
            1 => show_switch_menu(&mut app_state).await?,
            2 => {
                test_alert_command(&app_state).await?;
            }
            3 => {
                // Exit
                println!("{}", "👋 Goodbye!".bright_green());
                break;
            }
            _ => unreachable!(),
        }
    }

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
