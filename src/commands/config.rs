use anyhow::Result;
use inquire::{Select, Confirm};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use crate::config::ConfigManager;
use crate::ssh::{SshManager, validate_node_files};
use crate::types::Config;

pub async fn config_command(list: bool, edit: bool, test: bool) -> Result<()> {
    let config_manager = ConfigManager::new()?;
    
    if list {
        return list_configuration(&config_manager).await;
    }
    
    if edit {
        return edit_configuration(&config_manager).await;
    }
    
    if test {
        return test_connections(&config_manager).await;
    }
    
    // Interactive config menu
    show_config_menu(&config_manager).await
}

async fn show_config_menu(config_manager: &ConfigManager) -> Result<()> {
    loop {
        let options = vec![
            "📋 List current configuration",
            "✏️  Edit configuration", 
            "🔗 Test connections",
            "🏠 Back to main menu"
        ];
        
        let selection = Select::new("What would you like to do?", options.clone())
            .prompt()?;
            
        let index = options.iter().position(|x| x == &selection).unwrap();
            
        match index {
            0 => list_configuration(config_manager).await?,
            1 => edit_configuration(config_manager).await?,
            2 => {
                test_connections(config_manager).await?;
                
                // Exact same UX as Node.js version - always return to menu
                println!("\n{}", "✨ Test completed!".bright_cyan());
                
                let continue_menu = Confirm::new("Return to configuration menu?")
                    .with_default(true)
                    .prompt()?;
                    
                if !continue_menu {
                    break;
                }
                // Continue loop to show menu again
            },
            3 => break, // Back to main menu
            _ => unreachable!(),
        }
    }
    
    Ok(())
}

async fn list_configuration(config_manager: &ConfigManager) -> Result<()> {
    println!("\n{}", "📋 Current Configuration".bright_cyan().bold());
    println!();
    
    let config = config_manager.load()?;
    
    println!("{}: {}", "Version".bright_white(), config.version);
    println!("{}: {}", "Config File".bright_white(), config_manager.get_config_path().display());
    println!("{}: {}", "SSH Key".bright_white(), config.ssh.key_path);
    println!("{}: {}s", "SSH Timeout".bright_white(), config.ssh.timeout);
    println!("{}: {}", "RPC Endpoint".bright_white(), config.rpc.endpoint);
    
    if !config.nodes.is_empty() {
        println!("\n{}", "🖥️ Configured Nodes".bright_cyan().bold());
        for (role, node) in &config.nodes {
            println!("\n{}:", role.bright_yellow());
            println!("  {}: {}", "Label".white(), node.label);
            println!("  {}: {}", "Host".white(), node.host);
            println!("  {}: {}", "Port".white(), node.port);
            println!("  {}: {}", "User".white(), node.user);
            
            println!("  {}:", "Paths".white());
            println!("    {}: {}", "Funded Identity".dimmed(), node.paths.funded_identity);
            println!("    {}: {}", "Unfunded Identity".dimmed(), node.paths.unfunded_identity);
            println!("    {}: {}", "Vote Keypair".dimmed(), node.paths.vote_keypair);
            println!("    {}: {}", "Ledger".dimmed(), node.paths.ledger);
            println!("    {}: {}", "Tower".dimmed(), node.paths.tower);
        }
    }
    
    Ok(())
}

async fn edit_configuration(_config_manager: &ConfigManager) -> Result<()> {
    println!("{}", "✏️ Configuration editing coming soon...".yellow());
    std::thread::sleep(Duration::from_secs(1));
    Ok(())
}

async fn test_connections(config_manager: &ConfigManager) -> Result<()> {
    let config = config_manager.load()?;
    
    if config.nodes.is_empty() {
        println!("{}", "⚠️ No nodes configured. Run setup first.".yellow());
        return Ok(());
    }
    
    // Ask for test type like original Node.js version
    let test_options = vec![
        "🚀 Quick Connection Test",
        "🔍 Full Diagnostics", 
        "🏥 Health Check",
        "🔍 Node Detection",
        "🌟 Complete Test Suite"
    ];
    
    let test_selection = Select::new("What type of test would you like to run?", test_options.clone())
        .prompt()?;
        
    let test_selection_idx = test_options.iter().position(|x| x == &test_selection).unwrap();
    
    match test_selection_idx {
        0 => run_quick_connection_test(&config).await?,
        1 => println!("{}", "🔍 Full diagnostics coming soon...".yellow()),
        2 => println!("{}", "🏥 Health check coming soon...".yellow()),
        3 => println!("{}", "🔍 Node detection coming soon...".yellow()),
        4 => println!("{}", "🌟 Complete test suite coming soon...".yellow()),
        _ => unreachable!(),
    }
    
    Ok(())
}

async fn run_quick_connection_test(config: &Config) -> Result<()> {
    println!("\n{}", "🚀 Quick Connection Test Results".bright_cyan().bold());
    println!();
    
    // Create a table-like display to match Node.js output
    println!("{:<20} {:<15} {:<10} {:<10}", 
        "Node".bright_cyan(), 
        "Status".bright_cyan(), 
        "Latency".bright_cyan(), 
        "Files".bright_cyan()
    );
    println!("{}", "─".repeat(60));
    
    let mut results = Vec::new();
    
    for (role, node) in &config.nodes {
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(ProgressStyle::default_spinner()
            .template("{spinner:.green} Setting up SSH connections...")
            .unwrap());
        spinner.enable_steady_tick(Duration::from_millis(100));
        
        let mut ssh = SshManager::new();
        let connection_result = ssh.connect(node, &config.ssh.key_path).await;
        
        match connection_result {
            Ok(status) => {
                spinner.finish_and_clear();
                
                // Validate files
                let validation = validate_node_files(&mut ssh, node).await?;
                let files_status = format!("{}/{}", validation.valid_files, validation.total_files);
                let files_color = if validation.valid_files == validation.total_files {
                    files_status.green()
                } else if validation.valid_files >= validation.total_files * 80 / 100 {
                    files_status.yellow()
                } else {
                    files_status.red()
                };
                
                println!("{:<20} {:<15} {:<10} {}", 
                    format!("{} ({})", node.label, role.to_uppercase()),
                    "✅ Connected".green(),
                    format!("{}ms", status.latency_ms.unwrap_or(0)),
                    files_color
                );
                
                results.push((role, node, validation));
                ssh.disconnect();
            },
            Err(e) => {
                spinner.finish_and_clear();
                println!("{:<20} {:<15} {:<10} {}", 
                    format!("{} ({})", node.label, role.to_uppercase()),
                    "❌ Failed".red(),
                    "N/A",
                    "N/A".dimmed()
                );
                println!("   Error: {}", e.to_string().red());
            }
        }
    }
    
    // Show detailed file validation results like Node.js version
    for (role, node, validation) in results {
        println!("\n{} File validation details for {} ({}):", 
            "📋".bright_cyan(), 
            node.label, 
            role.to_uppercase()
        );
        
        let passed_checks = validation.valid_files;
        let total_checks = validation.total_files;
        
        if passed_checks > 0 {
            println!("\n{} Passed checks ({}/{}):", 
                "✅".green(), 
                passed_checks, 
                total_checks
            );
            show_passed_validations(&config, node).await?;
        }
        
        if !validation.issues.is_empty() {
            println!("\n{} Issues found ({}):", 
                "⚠️".yellow(), 
                validation.issues.len()
            );
            for issue in &validation.issues {
                println!("   • {}", issue.yellow());
            }
        }
        
        if validation.issues.is_empty() {
            println!("\n{} All validator files validated successfully!", "🎉".green());
        }
    }
    
    Ok(())
}

async fn show_passed_validations(config: &Config, node: &crate::types::NodeConfig) -> Result<()> {
    let mut ssh = SshManager::new();
    let _ = ssh.connect(node, &config.ssh.key_path).await;
    
    // Check each validation that passed and show checkmarks
    
    // Check ledger directory
    if let Ok(_) = ssh.execute_command(&format!("test -d \"{}\"", node.paths.ledger)).await {
        println!("   ✓ Ledger directory: {}", node.paths.ledger.green());
    }
    
    // Check accounts folder
    if let Ok(_) = ssh.execute_command(&format!("test -d \"{}/accounts\"", node.paths.ledger)).await {
        println!("   ✓ Accounts folder in ledger directory");
    }
    
    // Check tower file
    if let Ok(output) = ssh.execute_command(&format!("ls {}/tower-1_9-*.bin 2>/dev/null | head -1", node.paths.ledger)).await {
        if !output.is_empty() {
            println!("   ✓ Tower file: {}", output.trim().green());
        }
    }
    
    // Check keypairs
    if let Ok(_) = ssh.execute_command(&format!("test -f \"{}\"", node.paths.funded_identity)).await {
        println!("   ✓ Funded identity keypair: {}", node.paths.funded_identity.green());
    }
    
    if let Ok(_) = ssh.execute_command(&format!("test -f \"{}\"", node.paths.unfunded_identity)).await {
        println!("   ✓ Unfunded identity keypair: {}", node.paths.unfunded_identity.green());
    }
    
    if let Ok(_) = ssh.execute_command(&format!("test -f \"{}\"", node.paths.vote_keypair)).await {
        println!("   ✓ Vote account keypair: {}", node.paths.vote_keypair.green());
    }
    
    ssh.disconnect();
    Ok(())
}

