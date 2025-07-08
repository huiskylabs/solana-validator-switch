use clap::{Parser, Subcommand};
use anyhow::Result;

mod config;
mod ssh;
mod commands;
mod types;

use commands::{config_command, setup_command};

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
        /// Export configuration to stdout
        #[arg(long)]
        export: bool,
    },
    /// Setup initial configuration
    Setup,
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Config { list, edit, test, export }) => {
            config_command(list, edit, test, export).await?;
        }
        Some(Commands::Setup) => {
            setup_command().await?;
        }
        None => {
            // Interactive main menu like original Node.js version
            show_interactive_menu().await?;
        }
    }

    Ok(())
}

async fn show_interactive_menu() -> Result<()> {
    use inquire::Select;
    use colored::*;

    // Clear screen and show welcome like original
    println!("\x1B[2J\x1B[1;1H"); // Clear screen
    println!("{}", "🚀 Welcome to Solana Validator Switch CLI v1.0.0".bright_cyan().bold());
    println!("{}", "Professional-grade validator switching from your terminal".dimmed());
    println!();

    loop {
        let mut options = vec![
            "🔧 Setup - Configure your validator nodes and SSH keys",
            "📋 Status - Check current validator status",
            "🔄 Switch - Switch between validators", 
            "💚 Health - Detailed health check",
            "📊 Monitor - Real-time monitoring dashboard",
            "⚙️  Config - Manage configuration",
            "📌 Version - Show version information"
        ];
        
        options.push("❌ Exit");
        
        let selection = Select::new("What would you like to do?", options.clone())
            .prompt()?;
            
        let index = options.iter().position(|x| x == &selection).unwrap();
        
        match index {
            0 => setup_command().await?,
            1 => {
                println!("{}", "📋 Status coming soon...".yellow());
                std::thread::sleep(std::time::Duration::from_secs(1));
            },
            2 => show_switch_menu().await?,
            3 => {
                println!("{}", "💚 Health check coming soon...".yellow());
                std::thread::sleep(std::time::Duration::from_secs(1));
            },
            4 => {
                println!("{}", "📊 Monitor coming soon...".yellow());
                std::thread::sleep(std::time::Duration::from_secs(1));
            },
            5 => show_config_menu().await?,
            6 => {
                println!("{}", "📌 Version: 1.0.0".bright_blue());
                std::thread::sleep(std::time::Duration::from_secs(1));
            },
            7 => { // Exit
                println!("{}", "👋 Goodbye!".bright_green());
                break;
            },
            _ => unreachable!(),
        }
    }
    
    Ok(())
}

async fn show_config_menu() -> Result<()> {
    use inquire::Select;
    use colored::*;
    
    loop {
        println!("\n{}", "⚙️  Configuration Management".bright_cyan().bold());
        println!();
        
        let mut options = vec![
            "📋 List - Show current configuration",
            "✏️  Edit - Edit configuration interactively",
            "🧪 Test - Test SSH connections",
            "📤 Export - Export configuration to file",
            "📥 Import - Import configuration from file"
        ];
        
        options.push("⬅️  Back to main menu");
        
        let selection = Select::new("Select configuration action:", options.clone())
            .prompt()?;
            
        let index = options.iter().position(|x| x == &selection).unwrap();
        
        match index {
            0 => config_command(true, false, false, false).await?,
            1 => config_command(false, true, false, false).await?,
            2 => config_command(false, false, true, false).await?,
            3 => config_command(false, false, false, true).await?,
            4 => {
                println!("{}", "📥 Import coming soon...".yellow());
                std::thread::sleep(std::time::Duration::from_secs(1));
            },
            5 => break, // Back to main menu
            _ => unreachable!(),
        }
    }
    
    Ok(())
}

async fn show_switch_menu() -> Result<()> {
    use inquire::Select;
    use colored::*;
    
    loop {
        println!("\n{}", "🔄 Validator Switching".bright_cyan().bold());
        println!();
        
        let mut options = vec![
            "🔄 Switch - Perform validator switch",
            "🧪 Dry Run - Preview switch without executing",
            "⚡ Force - Force switch (skip safety checks)",
            "📊 Status - Check switch readiness"
        ];
        
        options.push("⬅️  Back to main menu");
        
        let selection = Select::new("Select switching action:", options.clone())
            .prompt()?;
            
        let index = options.iter().position(|x| x == &selection).unwrap();
        
        match index {
            0 => {
                println!("{}", "🔄 Switch coming soon...".yellow());
                std::thread::sleep(std::time::Duration::from_secs(1));
            },
            1 => {
                println!("{}", "🧪 Dry run coming soon...".yellow());
                std::thread::sleep(std::time::Duration::from_secs(1));
            },
            2 => {
                println!("{}", "⚡ Force switch coming soon...".yellow());
                std::thread::sleep(std::time::Duration::from_secs(1));
            },
            3 => {
                println!("{}", "📊 Switch status coming soon...".yellow());
                std::thread::sleep(std::time::Duration::from_secs(1));
            },
            4 => break, // Back to main menu
            _ => unreachable!(),
        }
    }
    
    Ok(())
}