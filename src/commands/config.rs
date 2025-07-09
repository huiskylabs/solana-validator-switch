use anyhow::Result;
use inquire::{Select, Confirm};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

use crate::config::ConfigManager;
use crate::ssh::{SshManager, validate_node_files};
use crate::types::Config;

pub async fn config_command(list: bool, edit: bool, test: bool, app_state: Option<&crate::AppState>) -> Result<()> {
    let config_manager = ConfigManager::new()?;
    
    if list {
        return list_configuration(&config_manager).await;
    }
    
    if edit {
        return edit_configuration(&config_manager).await;
    }
    
    if test {
        return test_connections_with_state(&config_manager, app_state).await;
    }
    
    // Interactive config menu
    show_config_menu_with_state(&config_manager, app_state).await
}

async fn show_config_menu_with_state(config_manager: &ConfigManager, app_state: Option<&crate::AppState>) -> Result<()> {
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
                test_connections_with_state(config_manager, app_state).await?;
                
                // Exact same UX as Node.js version - always return to menu
                println!("\n{}", "✨ Test completed!".bright_cyan());
                
                let continue_menu = Confirm::new("Return to configuration menu?")
                    .with_default(true)
                    .prompt()?;
                    
                if !continue_menu {
                    break;
                }
            }
            3 => break, // Back to main menu
            _ => unreachable!(),
        }
    }
    
    Ok(())
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
    println!("{}: {}", "RPC Endpoint".bright_white(), config.rpc.endpoint);
    
    if !config.nodes.is_empty() {
        println!("\n{}", "🖥️ Configured Node Pairs".bright_cyan().bold());
        for (index, node_pair) in config.nodes.iter().enumerate() {
            println!("\n{} {}:", "Node Pair".bright_yellow(), index + 1);
            
            println!("  {}:", "Validator Identity".white());
            println!("    {}: {}", "Vote Pubkey".dimmed(), node_pair.vote_pubkey);
            println!("    {}: {}", "Identity Pubkey".dimmed(), node_pair.identity_pubkey);
            
            println!("  {}:", "Primary Node".green());
            println!("    {}: {}", "Label".white(), node_pair.primary.label);
            println!("    {}: {}", "Host".white(), node_pair.primary.host);
            println!("    {}: {}", "Port".white(), node_pair.primary.port);
            println!("    {}: {}", "User".white(), node_pair.primary.user);
            
            println!("  {}:", "Backup Node".yellow());
            println!("    {}: {}", "Label".white(), node_pair.backup.label);
            println!("    {}: {}", "Host".white(), node_pair.backup.host);
            println!("    {}: {}", "Port".white(), node_pair.backup.port);
            println!("    {}: {}", "User".white(), node_pair.backup.user);
            
            println!("  {}:", "Paths (shared)".white());
            println!("    {}: {}", "Funded Identity".dimmed(), node_pair.primary.paths.funded_identity);
            println!("    {}: {}", "Unfunded Identity".dimmed(), node_pair.primary.paths.unfunded_identity);
            println!("    {}: {}", "Vote Keypair".dimmed(), node_pair.primary.paths.vote_keypair);
            println!("    {}: {}", "Ledger".dimmed(), node_pair.primary.paths.ledger);
            println!("    {}: {}", "Tower".dimmed(), node_pair.primary.paths.tower);
        }
    }
    
    Ok(())
}

async fn edit_configuration(_config_manager: &ConfigManager) -> Result<()> {
    println!("{}", "✏️ Configuration editing coming soon...".yellow());
    std::thread::sleep(Duration::from_secs(1));
    Ok(())
}

async fn test_connections_with_state(config_manager: &ConfigManager, app_state: Option<&crate::AppState>) -> Result<()> {
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
        0 => {
            if let Some(state) = app_state {
                run_quick_connection_test_with_pool(&config, state).await?
            } else {
                run_quick_connection_test(&config).await?
            }
        },
        1 => println!("{}", "🔍 Full diagnostics coming soon...".yellow()),
        2 => println!("{}", "🏥 Health check coming soon...".yellow()),
        3 => println!("{}", "🔍 Node detection coming soon...".yellow()),
        4 => println!("{}", "🌟 Complete test suite coming soon...".yellow()),
        _ => unreachable!(),
    }
    
    Ok(())
}

async fn test_connections(config_manager: &ConfigManager) -> Result<()> {
    test_connections_with_state(config_manager, None).await
}

async fn run_quick_connection_test_with_pool(config: &Config, app_state: &crate::AppState) -> Result<()> {
    println!("\n{}", "🚀 Quick Connection Test Results (using shared pool)".bright_cyan().bold());
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
    
    // Use the shared SSH pool from app_state
    let mut ssh_pool = app_state.ssh_pool.lock().unwrap();
    
    for (pair_index, node_pair) in config.nodes.iter().enumerate() {
        let _pair_name = format!("pair_{}", pair_index);
        
        // Test primary node - connection should already exist
        let start_time = std::time::Instant::now();
        let connection_result = ssh_pool.connect(&node_pair.primary, &config.ssh_key_path).await;
        let latency = start_time.elapsed().as_millis() as u64;
        
        match connection_result {
            Ok(_) => {
                // Validate files
                let validation = crate::ssh::validate_node_files_with_pool(&mut *ssh_pool, &node_pair.primary, &config.ssh_key_path).await?;
                let files_status = format!("{}/{}", validation.valid_files, validation.total_files);
                let files_color = if validation.valid_files == validation.total_files {
                    files_status.green()
                } else if validation.valid_files >= validation.total_files * 80 / 100 {
                    files_status.yellow()
                } else {
                    files_status.red()
                };
                
                println!("{:<20} {:<15} {:<10} {}", 
                    format!("{} (PRIMARY)", node_pair.primary.label),
                    "✅ Connected".green(),
                    format!("{}ms", latency),
                    files_color
                );
                
                results.push(("primary", &node_pair.primary, validation));
            },
            Err(e) => {
                println!("{:<20} {:<15} {:<10} {}", 
                    format!("{} (PRIMARY)", node_pair.primary.label),
                    "❌ Failed".red(),
                    "N/A",
                    "N/A".dimmed()
                );
                println!("   Error: {}", e.to_string().red());
            }
        }
        
        // Test backup node - connection should already exist
        let start_time = std::time::Instant::now();
        let connection_result = ssh_pool.connect(&node_pair.backup, &config.ssh_key_path).await;
        let latency = start_time.elapsed().as_millis() as u64;
        
        match connection_result {
            Ok(_) => {
                // Validate files
                let validation = crate::ssh::validate_node_files_with_pool(&mut *ssh_pool, &node_pair.backup, &config.ssh_key_path).await?;
                let files_status = format!("{}/{}", validation.valid_files, validation.total_files);
                let files_color = if validation.valid_files == validation.total_files {
                    files_status.green()
                } else if validation.valid_files >= validation.total_files * 80 / 100 {
                    files_status.yellow()
                } else {
                    files_status.red()
                };
                
                println!("{:<20} {:<15} {:<10} {}", 
                    format!("{} (BACKUP)", node_pair.backup.label),
                    "✅ Connected".green(),
                    format!("{}ms", latency),
                    files_color
                );
                
                results.push(("backup", &node_pair.backup, validation));
            },
            Err(e) => {
                println!("{:<20} {:<15} {:<10} {}", 
                    format!("{} (BACKUP)", node_pair.backup.label),
                    "❌ Failed".red(),
                    "N/A",
                    "N/A".dimmed()
                );
                println!("   Error: {}", e.to_string().red());
            }
        }
    }
    
    println!("\n{}", "📊 Summary".bright_cyan().bold());
    println!("Total nodes tested: {}", results.len());
    println!("Pool stats: {} total connections", ssh_pool.get_pool_stats().total_connections);
    
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
    
    // Use a single SSH connection pool for the entire test
    let mut ssh_pool = crate::ssh::SshConnectionPool::new();
    
    for (pair_index, node_pair) in config.nodes.iter().enumerate() {
        let pair_name = format!("pair_{}", pair_index);
        
        // Test primary node
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(ProgressStyle::default_spinner()
            .template("{spinner:.green} Testing SSH connections...")
            .unwrap());
        spinner.enable_steady_tick(Duration::from_millis(100));
        
        let start_time = std::time::Instant::now();
        let connection_result = ssh_pool.connect(&node_pair.primary, &config.ssh_key_path).await;
        let latency = start_time.elapsed().as_millis() as u64;
        
        match connection_result {
            Ok(_) => {
                spinner.finish_and_clear();
                
                // Validate files
                let validation = crate::ssh::validate_node_files_with_pool(&mut ssh_pool, &node_pair.primary, &config.ssh_key_path).await?;
                let files_status = format!("{}/{}", validation.valid_files, validation.total_files);
                let files_color = if validation.valid_files == validation.total_files {
                    files_status.green()
                } else if validation.valid_files >= validation.total_files * 80 / 100 {
                    files_status.yellow()
                } else {
                    files_status.red()
                };
                
                println!("{:<20} {:<15} {:<10} {}", 
                    format!("{} (PRIMARY)", node_pair.primary.label),
                    "✅ Connected".green(),
                    format!("{}ms", latency),
                    files_color
                );
                
                results.push(("primary", &node_pair.primary, validation));
            },
            Err(e) => {
                spinner.finish_and_clear();
                println!("{:<20} {:<15} {:<10} {}", 
                    format!("{} (PRIMARY)", node_pair.primary.label),
                    "❌ Failed".red(),
                    "N/A",
                    "N/A".dimmed()
                );
                println!("   Error: {}", e.to_string().red());
            }
        }
        
        // Test backup node
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(ProgressStyle::default_spinner()
            .template("{spinner:.green} Testing SSH connections...")
            .unwrap());
        spinner.enable_steady_tick(Duration::from_millis(100));
        
        let start_time = std::time::Instant::now();
        let connection_result = ssh_pool.connect(&node_pair.backup, &config.ssh_key_path).await;
        let latency = start_time.elapsed().as_millis() as u64;
        
        match connection_result {
            Ok(_) => {
                spinner.finish_and_clear();
                
                // Validate files
                let validation = crate::ssh::validate_node_files_with_pool(&mut ssh_pool, &node_pair.backup, &config.ssh_key_path).await?;
                let files_status = format!("{}/{}", validation.valid_files, validation.total_files);
                let files_color = if validation.valid_files == validation.total_files {
                    files_status.green()
                } else if validation.valid_files >= validation.total_files * 80 / 100 {
                    files_status.yellow()
                } else {
                    files_status.red()
                };
                
                println!("{:<20} {:<15} {:<10} {}", 
                    format!("{} (BACKUP)", node_pair.backup.label),
                    "✅ Connected".green(),
                    format!("{}ms", latency),
                    files_color
                );
                
                results.push(("backup", &node_pair.backup, validation));
            },
            Err(e) => {
                spinner.finish_and_clear();
                println!("{:<20} {:<15} {:<10} {}", 
                    format!("{} (BACKUP)", node_pair.backup.label),
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
    let _ = ssh.connect(node, &config.ssh_key_path).await;
    
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

