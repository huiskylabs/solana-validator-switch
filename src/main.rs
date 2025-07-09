use clap::{Parser, Subcommand};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use colored::*;

mod config;
mod ssh;
mod commands;
mod types;
mod startup;

use commands::{setup_command, status_command, switch_command};
use ssh::SshConnectionPool;

#[derive(Parser)]
#[command(name = "svs")]
#[command(about = "Solana Validator Switch - Interactive CLI for validator management")]
#[command(version = "1.0.0")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Interactive setup wizard for initial configuration
    Setup,
    /// Check current validator status
    Status,
    /// Switch between primary and backup validators
    Switch {
        /// Preview switch without executing
        #[arg(short, long)]
        dry_run: bool,
    },
}

/// Application state that persists throughout the CLI session
pub struct AppState {
    pub ssh_pool: Arc<Mutex<SshConnectionPool>>,
    pub config: types::Config,
    pub validator_statuses: Vec<ValidatorStatus>,
}

#[derive(Debug, Clone)]
pub struct ValidatorStatus {
    pub validator_pair: types::ValidatorPair,
    pub nodes_with_status: Vec<types::NodeWithStatus>,
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
        Some(Commands::Setup) => {
            setup_command().await?;
        }
        Some(Commands::Status) => {
            if let Some(state) = app_state.as_ref() {
                status_command(state).await?;
            } else {
                println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
            }
        }
        Some(Commands::Switch { dry_run }) => {
            if let Some(state) = app_state.as_ref() {
                switch_command(dry_run, state).await?;
            } else {
                println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
            }
        }
        None => {
            // Interactive main menu only if app state is valid
            if let Some(state) = app_state.as_ref() {
                show_interactive_menu(Some(state)).await?;
            } else {
                println!("{}", "❌ Cannot start interactive mode without valid configuration.".red());
                println!("{}", "Please run 'svs setup' to configure the application first.".yellow());
                std::process::exit(1);
            }
        }
    }

    // Note: SSH connections are kept alive for performance - they'll be cleaned up on process exit

    Ok(())
}

async fn show_interactive_menu(app_state: Option<&AppState>) -> Result<()> {
    use inquire::Select;
    use colored::*;

    // Clear screen and show welcome like original
    println!("\x1B[2J\x1B[1;1H"); // Clear screen
    println!("{}", "🚀 Welcome to Solana Validator Switch CLI v1.0.0".bright_cyan().bold());
    println!("{}", "Professional-grade validator switching from your terminal".dimmed());
    println!();

    loop {
        let mut options = vec![
            "📋 Status - Check current validator status",
            "🔄 Switch - Switch between primary and backup validators"
        ];
        
        options.push("❌ Exit");
        
        let selection = Select::new("What would you like to do?", options.clone())
            .prompt()?;
            
        let index = options.iter().position(|x| x == &selection).unwrap();
        
        match index {
            0 => {
                if let Some(ref state) = app_state {
                    status_command(state).await?;
                } else {
                    println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
                }
            },
            1 => show_switch_menu(app_state).await?,
            2 => { // Exit
                println!("{}", "👋 Goodbye!".bright_green());
                break;
            },
            _ => unreachable!(),
        }
    }
    
    Ok(())
}


async fn show_switch_menu(app_state: Option<&AppState>) -> Result<()> {
    use inquire::Select;
    use colored::*;
    
    loop {
        println!("\n{}", "🔄 Validator Switching".bright_cyan().bold());
        println!();
        
        let mut options = vec![
            "🔄 Switch - Switch between primary and backup validators",
            "🧪 Dry Run - Preview switch without executing"
        ];
        
        options.push("⬅️  Back to main menu");
        
        let selection = Select::new("Select switching action:", options.clone())
            .prompt()?;
            
        let index = options.iter().position(|x| x == &selection).unwrap();
        
        match index {
            0 => {
                if let Some(state) = app_state {
                    switch_command(false, state).await?;
                } else {
                    println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
                }
            },
            1 => {
                if let Some(state) = app_state {
                    switch_command(true, state).await?;
                } else {
                    println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
                }
            },
            2 => break, // Back to main menu
            _ => unreachable!(),
        }
    }
    
    Ok(())
}