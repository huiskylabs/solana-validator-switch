use clap::{Parser, Subcommand};
use anyhow::Result;
use std::sync::{Arc, Mutex};
use colored::*;

mod config;
mod ssh;
mod commands;
mod types;
mod startup;

use commands::{config_command, setup_command, status_command, switch_command};
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
    /// Manage configuration settings
    Config {
        /// List current configuration
        #[arg(short, long)]
        list: bool,
        /// Edit configuration file
        #[arg(short, long)]
        edit: bool,
        /// Test connections to configured nodes
        #[arg(short, long)]
        test: bool,
    },
    /// Check current validator status
    Status,
    /// Switch between primary and backup validators
    Switch {
        /// Preview switch without executing
        #[arg(short, long)]
        dry_run: bool,
        /// Force switch (skip tower copy)
        #[arg(short, long)]
        force: bool,
    },
}

/// Application state that persists throughout the CLI session
pub struct AppState {
    pub ssh_pool: Arc<Mutex<SshConnectionPool>>,
    pub config: types::Config,
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
        Some(Commands::Config { list, edit, test }) => {
            config_command(list, edit, test, app_state.as_ref()).await?;
        }
        Some(Commands::Status) => {
            if let Some(state) = app_state.as_ref() {
                status_command(state).await?;
            } else {
                println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
            }
        }
        Some(Commands::Switch { dry_run, force }) => {
            if let Some(state) = app_state.as_ref() {
                switch_command(dry_run, force, state).await?;
            } else {
                println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
            }
        }
        None => {
            // Interactive main menu
            show_interactive_menu(app_state.as_ref()).await?;
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
            "⚙️  Config - Manage configuration",
            "📋 Status - Check current validator status",
            "🔄 Switch - Switch between primary and backup validators"
        ];
        
        options.push("❌ Exit");
        
        let selection = Select::new("What would you like to do?", options.clone())
            .prompt()?;
            
        let index = options.iter().position(|x| x == &selection).unwrap();
        
        match index {
            0 => show_config_menu(app_state).await?,
            1 => {
                if let Some(ref state) = app_state {
                    status_command(state).await?;
                } else {
                    println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
                }
            },
            2 => show_switch_menu(app_state).await?,
            3 => { // Exit
                println!("{}", "👋 Goodbye!".bright_green());
                break;
            },
            _ => unreachable!(),
        }
    }
    
    Ok(())
}

async fn show_config_menu(app_state: Option<&AppState>) -> Result<()> {
    use inquire::Select;
    use colored::*;
    
    loop {
        println!("\n{}", "⚙️  Configuration Management".bright_cyan().bold());
        println!();
        
        let mut options = vec![
            "🔧 Setup - Configure your validator nodes and SSH keys",
            "📋 List - Show current configuration",
            "✏️  Edit - Edit configuration interactively",
            "🧪 Test - Test SSH connections"
        ];
        
        options.push("⬅️  Back to main menu");
        
        let selection = Select::new("Select configuration action:", options.clone())
            .prompt()?;
            
        let index = options.iter().position(|x| x == &selection).unwrap();
        
        match index {
            0 => setup_command().await?,
            1 => config_command(true, false, false, app_state).await?,
            2 => config_command(false, true, false, app_state).await?,
            3 => config_command(false, false, true, app_state).await?,
            4 => break, // Back to main menu
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
            "🧪 Dry Run - Preview switch without executing", 
            "⚡ Force - Force switch (skip tower copy)"
        ];
        
        options.push("⬅️  Back to main menu");
        
        let selection = Select::new("Select switching action:", options.clone())
            .prompt()?;
            
        let index = options.iter().position(|x| x == &selection).unwrap();
        
        match index {
            0 => {
                if let Some(state) = app_state {
                    switch_command(false, false, state).await?;
                } else {
                    println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
                }
            },
            1 => {
                if let Some(state) = app_state {
                    switch_command(true, false, state).await?;
                } else {
                    println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
                }
            },
            2 => {
                if let Some(state) = app_state {
                    switch_command(false, true, state).await?;
                } else {
                    println!("{}", "⚠️ No configuration found. Please run setup first.".yellow());
                }
            },
            3 => break, // Back to main menu
            _ => unreachable!(),
        }
    }
    
    Ok(())
}