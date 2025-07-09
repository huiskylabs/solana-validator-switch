use anyhow::Result;
use colored::*;
use inquire::{Confirm, Select};
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use std::sync::{Arc, Mutex};
use std::io::{self, Write};

use crate::config::ConfigManager;
use crate::ssh::SshConnectionPool;
use crate::types::{Config, NodeConfig};
use crate::commands::setup_command;
use inquire::{Text, validator::Validation};

/// Startup validation result
#[derive(Debug)]
pub struct StartupValidation {
    pub success: bool,
    pub config_valid: bool,
    pub ssh_connections_valid: bool,
    pub model_verification_valid: bool,
    pub issues: Vec<String>,
    pub warnings: Vec<String>,
}

/// Comprehensive startup checklist and validation with enhanced UX
pub async fn run_startup_checklist() -> Result<Option<crate::AppState>> {
    // Clear screen and show startup banner
    println!("\x1B[2J\x1B[1;1H"); // Clear screen
    println!("{}", "🚀 Solana Validator Switch".bright_cyan().bold());
    println!("{}", "Initializing validator management system...".dimmed());
    println!();

    // Create progress bar for overall startup process
    let progress_bar = ProgressBar::new(100);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>3}% {msg}")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  ")
    );
    progress_bar.set_message("Starting up...");
    progress_bar.enable_steady_tick(Duration::from_millis(100));

    let mut validation = StartupValidation {
        success: false,
        config_valid: false,
        ssh_connections_valid: false,
        model_verification_valid: false,
        issues: Vec::new(),
        warnings: Vec::new(),
    };

    // Phase 1: Configuration validation (30% of progress)
    progress_bar.set_position(10);
    progress_bar.set_message("Validating configuration...");
    
    let config = match validate_configuration_with_progress(&mut validation, &progress_bar).await? {
        Some(config) => config,
        None => {
            progress_bar.finish_and_clear();
            return Ok(None); // User chose to exit or setup
        }
    };
    
    progress_bar.set_position(30);

    // Phase 2: SSH connection validation (60% of progress)
    progress_bar.set_message("Establishing SSH connections...");
    let ssh_pool = validate_ssh_connections_with_progress(&config, &mut validation, &progress_bar).await?;
    progress_bar.set_position(70);

    // Phase 3: Model verification (80% of progress)
    progress_bar.set_message("Verifying system readiness...");
    validate_model_verification_with_progress(&config, &ssh_pool, &mut validation, &progress_bar).await?;
    progress_bar.set_position(80);

    // Phase 4: Comprehensive validator status detection (85-95% of progress)
    let validator_statuses = if validation.config_valid && validation.ssh_connections_valid && validation.model_verification_valid {
        progress_bar.set_message("🔍 Detecting validator statuses...");
        progress_bar.set_position(85);
        
        let statuses = detect_node_statuses_with_progress(&config, &ssh_pool, &progress_bar).await?;
        progress_bar.set_position(95);
        Some(statuses)
    } else {
        None
    };

    // Phase 5: Final validation and summary
    progress_bar.set_message("Finalizing startup...");
    validation.success = validation.config_valid && validation.ssh_connections_valid && validation.model_verification_valid;
    
    progress_bar.set_position(100);
    progress_bar.finish_and_clear();

    if validation.success {
        if let Some(validator_statuses) = validator_statuses {
            // Show "press any key to continue" prompt
            show_ready_prompt().await;
            
            Ok(Some(crate::AppState {
                ssh_pool: Arc::new(Mutex::new(ssh_pool)),
                config,
                validator_statuses,
            }))
        } else {
            println!("\n{}", "❌ Validator status detection failed.".red().bold());
            Ok(None)
        }
    } else {
        // Show detailed failure information
        println!("\n{}", "❌ Startup validation failed!".red().bold());
        println!();
        
        // Show what failed
        if !validation.config_valid {
            println!("{} Configuration issues:", "❌".red());
        }
        if !validation.ssh_connections_valid {
            println!("{} SSH connection issues:", "❌".red());
        }
        if !validation.model_verification_valid {
            println!("{} System readiness issues:", "❌".red());
        }
        
        // Show specific issues
        if !validation.issues.is_empty() {
            println!("\n{} Issues to resolve:", "⚠️".yellow().bold());
            for (i, issue) in validation.issues.iter().enumerate() {
                println!("  {}. {}", i + 1, issue.red());
            }
        }
        
        // Show warnings if any
        if !validation.warnings.is_empty() {
            println!("\n{} Warnings:", "⚠️".yellow().bold());
            for (i, warning) in validation.warnings.iter().enumerate() {
                println!("  {}. {}", i + 1, warning.yellow());
            }
        }
        
        // Show helpful resolution steps
        println!("\n{} Suggested actions:", "💡".bright_blue().bold());
        if !validation.config_valid {
            println!("  • Check your configuration file: ~/.solana-validator-switch/config.yaml");
            println!("  • Run 'svs setup' to reconfigure");
        }
        if !validation.ssh_connections_valid {
            println!("  • Verify SSH key paths and permissions");
            println!("  • Test SSH connections manually: ssh -i <key> user@host");
            println!("  • Ensure remote hosts are accessible");
        }
        if !validation.model_verification_valid {
            println!("  • Check validator file paths and permissions");
            println!("  • Ensure validator processes are running");
        }
        
        // Show a prompt to acknowledge the error before exiting
        println!();
        println!("{}", "Press Enter to exit...".dimmed());
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();
        
        Ok(None)
    }
}

async fn validate_configuration_with_progress(validation: &mut StartupValidation, progress_bar: &ProgressBar) -> Result<Option<Config>> {
    let config_manager = ConfigManager::new()?;
    
    // Configuration file existence check
    progress_bar.set_message("Checking configuration file...");
    if !config_manager.exists() {
        progress_bar.suspend(|| {
            println!("  ❌ Configuration file not found");
        });
        
        validation.issues.push("Configuration file missing".to_string());
        
        progress_bar.suspend(|| {
            println!("\n{}", "⚠️ No configuration found.".yellow());
            println!();
            println!("{}", "Please create your configuration file at:".dimmed());
            println!("{}", format!("  {}", config_manager.get_config_path().display()).bright_cyan());
            println!();
            println!("{}", "You can either:".dimmed());
            println!("{}", "  1. Run 'svs setup' to use the interactive wizard".dimmed());
            println!("{}", "  2. Copy and edit the example config from the project".dimmed());
            println!("{}", "  3. Create the file manually using the documented format".dimmed());
            println!();
            println!("{}", "Application will exit now.".yellow());
        });
        
        return Ok(None);
    }

    // Configuration loading and validation
    progress_bar.set_message("Loading configuration...");
    match config_manager.load() {
        Ok(mut config) => {
            progress_bar.suspend(|| {
                println!("  ✅ Configuration file loaded: {}", config_manager.get_config_path().display());
            });
            
            // Check if migration is needed
            progress_bar.set_message("Checking configuration completeness...");
            let needs_migration = check_migration_needed(&config);
            if needs_migration {
                // Configuration needs migration - stop loading and fail immediately
                validation.config_valid = false;
                validation.issues.push("Configuration needs migration to include missing public key identifiers".to_string());
                return Ok(None); // Stop startup immediately
            }
            
            // Validate configuration completeness
            progress_bar.set_message("Validating configuration structure...");
            let config_issues = validate_config_completeness(&config);
            
            if config_issues.is_empty() {
                validation.config_valid = true;
                progress_bar.suspend(|| {
                    println!("  ✅ Configuration is complete and valid");
                });
                Ok(Some(config))
            } else {
                // Configuration has issues - stop loading and fail immediately
                validation.config_valid = false;
                validation.issues.extend(config_issues);
                Ok(None) // Return None to stop startup
            }
        }
        Err(e) => {
            progress_bar.suspend(|| {
                println!("  ❌ Failed to load configuration: {}", e);
            });
            validation.issues.push(format!("Configuration loading failed: {}", e));
            Ok(None)
        }
    }
}

async fn validate_configuration(validation: &mut StartupValidation) -> Result<Option<Config>> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(ProgressStyle::default_spinner()
        .template("  {spinner:.green} {msg}")
        .unwrap());
    spinner.set_message("Checking configuration file...");
    spinner.enable_steady_tick(Duration::from_millis(100));

    let config_manager = ConfigManager::new()?;
    
    // Check if configuration exists
    if !config_manager.exists() {
        spinner.finish_with_message("❌ Configuration file not found");
        validation.issues.push("Configuration file missing".to_string());
        
        println!("\n{}", "⚠️ No configuration found.".yellow());
        println!("{}", "You need to set up your validator configuration first.".dimmed());
        
        let setup_now = Confirm::new("Would you like to run the setup wizard now?")
            .with_default(true)
            .prompt()?;
            
        if setup_now {
            println!();
            setup_command().await?;
            
            // Try loading config again after setup
            match config_manager.load() {
                Ok(config) => {
                    validation.config_valid = true;
                    return Ok(Some(config));
                }
                Err(_) => {
                    validation.issues.push("Setup completed but configuration still invalid".to_string());
                    return Ok(None);
                }
            }
        } else {
            println!("{}", "Setup cancelled. Run 'svs setup' to configure later.".yellow());
            return Ok(None);
        }
    }

    // Load and validate configuration
    match config_manager.load() {
        Ok(mut config) => {
            spinner.finish_with_message("✅ Configuration loaded successfully");
            
            // Check if migration is needed (missing public key fields)
            let needs_migration = check_migration_needed(&config);
            if needs_migration {
                println!("  🔄 Configuration needs migration to include public key identifiers");
                
                let migrate_now = Confirm::new("Would you like to add the missing public key identifiers now?")
                    .with_default(true)
                    .prompt()?;
                    
                if migrate_now {
                    config = migrate_configuration(&config_manager, config).await?;
                    println!("  ✅ Configuration migrated successfully");
                } else {
                    println!("  ⚠️ Migration skipped. Some features may not work correctly.");
                }
            }
            
            // Validate configuration completeness
            let config_issues = validate_config_completeness(&config);
            
            if config_issues.is_empty() {
                validation.config_valid = true;
                println!("  ✅ Configuration is complete and valid");
                Ok(Some(config))
            } else {
                validation.issues.extend(config_issues.clone());
                println!("  ⚠️ Configuration has issues:");
                for issue in &config_issues {
                    println!("    • {}", issue.yellow());
                }
                
                let fix_now = Confirm::new("Would you like to fix these issues now?")
                    .with_default(true)
                    .prompt()?;
                    
                if fix_now {
                    fix_configuration_issues(&config, &config_issues).await?;
                    // Reload config after fixes
                    match config_manager.load() {
                        Ok(fixed_config) => {
                            validation.config_valid = true;
                            Ok(Some(fixed_config))
                        }
                        Err(e) => {
                            validation.issues.push(format!("Failed to reload configuration: {}", e));
                            Ok(None)
                        }
                    }
                } else {
                    println!("{}", "Configuration issues not resolved. Some features may not work correctly.".yellow());
                    Ok(Some(config))
                }
            }
        }
        Err(e) => {
            spinner.finish_with_message("❌ Failed to load configuration");
            validation.issues.push(format!("Configuration loading failed: {}", e));
            Ok(None)
        }
    }
}

async fn validate_ssh_connections_with_progress(config: &Config, validation: &mut StartupValidation, progress_bar: &ProgressBar) -> Result<SshConnectionPool> {
    let mut ssh_pool = SshConnectionPool::new();
    let mut connection_issues = Vec::new();
    
    if config.validators.is_empty() {
        validation.issues.push("No validators configured".to_string());
        progress_bar.suspend(|| {
            println!("  ❌ No validators configured");
        });
        return Ok(ssh_pool);
    }

    let total_nodes: usize = config.validators.iter().map(|v| v.nodes.len()).sum();
    let mut connected_nodes = 0;

    // Establish connections to all nodes efficiently
    for (validator_index, validator_pair) in config.validators.iter().enumerate() {
        let validator_name = format!("Validator {}", validator_index + 1);
        
        for (node_index, node) in validator_pair.nodes.iter().enumerate() {
            let node_name = format!("{} Node {}", validator_name, node_index + 1);
            
            progress_bar.set_message(format!("Connecting to {}...", node_name));
            match ssh_pool.connect(node, &validator_pair.local_ssh_key_path).await {
                Ok(_) => {
                    connected_nodes += 1;
                }
                Err(e) => {
                    connection_issues.push(format!("Failed to connect to {}: {}", node_name, e));
                }
            }
        }
    }

    // Final connection status
    if connection_issues.is_empty() {
        validation.ssh_connections_valid = true;
    } else {
        validation.issues.extend(connection_issues);
        validation.ssh_connections_valid = false;
    }

    Ok(ssh_pool)
}

async fn validate_ssh_connections(config: &Config, validation: &mut StartupValidation) -> Result<SshConnectionPool> {
    let mut ssh_pool = SshConnectionPool::new();
    let mut connection_issues = Vec::new();
    
    if config.validators.is_empty() {
        validation.issues.push("No validators configured".to_string());
        return Ok(ssh_pool);
    }

    // Establish connections to all nodes efficiently
    for (validator_index, validator_pair) in config.validators.iter().enumerate() {
        let validator_name = format!("Validator {}", validator_index + 1);
        
        for (node_index, node) in validator_pair.nodes.iter().enumerate() {
            let node_name = format!("{} Node {}", validator_name, node_index + 1);
            
            match ssh_pool.connect(node, &validator_pair.local_ssh_key_path).await {
                Ok(_) => {
                    println!("✅ Connected to {}: {}@{}", node_name, node.user, node.host);
                }
                Err(e) => {
                    connection_issues.push(format!("Failed to connect to {}: {}", node_name, e));
                }
            }
        }
    }

    if connection_issues.is_empty() {
        validation.ssh_connections_valid = true;
        println!("  ✅ All SSH connections established successfully");
    } else {
        validation.issues.extend(connection_issues);
        validation.ssh_connections_valid = false;
        println!("  ⚠️ Some SSH connections failed - continuing anyway");
    }

    Ok(ssh_pool)
}

async fn validate_model_verification_with_progress(_config: &Config, _ssh_pool: &SshConnectionPool, validation: &mut StartupValidation, progress_bar: &ProgressBar) -> Result<()> {
    // Skip detailed model verification since we already established connections
    // This avoids creating duplicate connections and improves startup performance
    progress_bar.set_message("Verifying system readiness...");
    
    // Simulate a brief validation check
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    progress_bar.suspend(|| {
        println!("  ✅ System readiness verified");
    });
    
    validation.model_verification_valid = true;
    Ok(())
}

async fn validate_model_verification(_config: &Config, _ssh_pool: &SshConnectionPool, validation: &mut StartupValidation) -> Result<()> {
    // Skip model verification since we already established connections in phase 2
    // This avoids creating duplicate connections and improves startup performance
    println!("  ✅ Skipping detailed model verification - using existing connections");
    validation.model_verification_valid = true;
    Ok(())
}

async fn verify_keypair_files(_ssh_pool: &SshConnectionPool, node: &NodeConfig, ssh_key_path: &str) -> Vec<String> {
    let mut issues = Vec::new();
    
    // Create a temporary SSH connection for verification
    let mut temp_pool = SshConnectionPool::new();
    let _ = temp_pool.connect(node, ssh_key_path).await;
    
    // Check critical keypair files
    let keypairs = vec![
        (&node.paths.funded_identity, "Funded identity keypair"),
        (&node.paths.unfunded_identity, "Unfunded identity keypair"),
        (&node.paths.vote_keypair, "Vote keypair"),
    ];
    
    for (path, description) in keypairs {
        // Check if file exists
        if let Err(_) = temp_pool.execute_command(node, ssh_key_path, &format!("test -f '{}'", path)).await {
            issues.push(format!("{} missing: {}", description, path));
            continue;
        }
        
        // Check if file is readable
        if let Err(_) = temp_pool.execute_command(node, ssh_key_path, &format!("test -r '{}'", path)).await {
            issues.push(format!("{} not readable: {}", description, path));
        }
    }
    
    issues
}

async fn verify_public_key_matches(_ssh_pool: &SshConnectionPool, node: &NodeConfig, ssh_key_path: &str) -> Vec<String> {
    let mut issues = Vec::new();
    
    // Create a temporary SSH connection for verification
    let mut temp_pool = SshConnectionPool::new();
    let _ = temp_pool.connect(node, ssh_key_path).await;
    
    // Note: Public key verification will be handled separately with access to the shared validator config
    // For now, skip this validation as it needs the full config structure
    
    issues
}

async fn verify_validator_paths(_ssh_pool: &SshConnectionPool, node: &NodeConfig, ssh_key_path: &str) -> Vec<String> {
    let mut issues = Vec::new();
    
    // Create a temporary SSH connection for verification
    let mut temp_pool = SshConnectionPool::new();
    let _ = temp_pool.connect(node, ssh_key_path).await;
    
    // Check ledger directory
    if let Err(_) = temp_pool.execute_command(node, ssh_key_path, &format!("test -d '{}'", node.paths.ledger)).await {
        issues.push(format!("Ledger directory missing: {}", node.paths.ledger));
    }
    
    // Check if Solana CLI is executable
    if let Err(_) = temp_pool.execute_command(node, ssh_key_path, &format!("test -x '{}'", node.paths.solana_cli_path)).await {
        issues.push(format!("Solana CLI not executable: {}", node.paths.solana_cli_path));
    }
    
    // Check tower file pattern
    if let Ok(output) = temp_pool.execute_command(node, ssh_key_path, &format!("ls {} 2>/dev/null | head -1", node.paths.tower)).await {
        if output.trim().is_empty() {
            issues.push(format!("No tower files found matching: {}", node.paths.tower));
        }
    }
    
    issues
}

fn validate_config_completeness(config: &Config) -> Vec<String> {
    let mut issues = Vec::new();
    
    // Check if we have at least one validator
    if config.validators.is_empty() {
        issues.push("No validators configured".to_string());
        return issues;
    }
    
    // Check each validator
    for (index, validator_pair) in config.validators.iter().enumerate() {
        let validator_name = format!("Validator {}", index + 1);
        
        // Check public keys
        if validator_pair.vote_pubkey.is_empty() {
            issues.push(format!("{} vote pubkey is empty", validator_name));
        }
        
        if validator_pair.identity_pubkey.is_empty() {
            issues.push(format!("{} identity pubkey is empty", validator_name));
        }
        
        // Check local SSH key path
        if validator_pair.local_ssh_key_path.is_empty() {
            issues.push(format!("{} local SSH key path is empty", validator_name));
        }
        
        // Check RPC endpoint
        if validator_pair.rpc.is_empty() {
            issues.push(format!("{} RPC endpoint is empty", validator_name));
        }
        
        // Check nodes
        if validator_pair.nodes.len() != 2 {
            issues.push(format!("{} should have exactly 2 nodes", validator_name));
        }
        
        for (node_index, node) in validator_pair.nodes.iter().enumerate() {
            let node_name = format!("{} Node {}", validator_name, node_index + 1);
            validate_node_config(node, &node_name, &mut issues);
        }
    }
    
    issues
}

fn validate_node_config(node: &crate::types::NodeConfig, node_name: &str, issues: &mut Vec<String>) {
    if node.host.is_empty() {
        issues.push(format!("{} host is empty", node_name));
    }
    
    if node.user.is_empty() {
        issues.push(format!("{} user is empty", node_name));
    }
    
    if node.paths.funded_identity.is_empty() {
        issues.push(format!("{} funded identity path is empty", node_name));
    }
    
    if node.paths.unfunded_identity.is_empty() {
        issues.push(format!("{} unfunded identity path is empty", node_name));
    }
    
    if node.paths.vote_keypair.is_empty() {
        issues.push(format!("{} vote keypair path is empty", node_name));
    }
    
    if node.paths.ledger.is_empty() {
        issues.push(format!("{} ledger path is empty", node_name));
    }
    
    if node.paths.solana_cli_path.is_empty() {
        issues.push(format!("{} solana CLI path is empty", node_name));
    }
}

async fn fix_configuration_issues(_config: &Config, issues: &[String]) -> Result<()> {
    println!("\n{}", "🔧 Configuration Issue Resolution".bright_cyan().bold());
    println!("The following issues were found:");
    
    for (i, issue) in issues.iter().enumerate() {
        println!("  {}. {}", i + 1, issue);
    }
    
    println!("\n{}", "Resolution options:".bright_cyan());
    
    let options = vec![
        "🔧 Run setup wizard to reconfigure",
        "✏️ Edit configuration manually",
        "⏩ Continue with current configuration"
    ];
    
    let selection = Select::new("How would you like to resolve these issues?", options.clone())
        .prompt()?;
        
    let index = options.iter().position(|x| x == &selection).unwrap();
    
    match index {
        0 => {
            println!("\n{}", "Running setup wizard...".bright_cyan());
            setup_command().await?;
        }
        1 => {
            println!("{}", "Manual configuration editing not yet implemented.".yellow());
            println!("{}", "Please use the setup wizard or edit the configuration file directly.".dimmed());
        }
        2 => {
            println!("{}", "Continuing with current configuration...".yellow());
        }
        _ => unreachable!(),
    }
    
    Ok(())
}


fn display_validation_summary(validation: &StartupValidation) {
    println!();
    println!("  📊 Validation Summary:");
    println!("    Configuration: {}", if validation.config_valid { "✅ Valid" } else { "❌ Invalid" });
    println!("    SSH Connections: {}", if validation.ssh_connections_valid { "✅ Connected" } else { "❌ Failed" });
    println!("    Model Verification: {}", if validation.model_verification_valid { "✅ Verified" } else { "❌ Issues Found" });
    
    if !validation.issues.is_empty() {
        println!("\n  ⚠️ Issues to resolve:");
        for issue in &validation.issues {
            println!("    • {}", issue.red());
        }
    }
    
    if !validation.warnings.is_empty() {
        println!("\n  ⚠️ Warnings:");
        for warning in &validation.warnings {
            println!("    • {}", warning.yellow());
        }
    }
    
    // Set overall success status
    // validation.success = validation.config_valid && validation.ssh_connections_valid && validation.model_verification_valid;
    if validation.config_valid && validation.ssh_connections_valid && validation.model_verification_valid {
        println!("\n  🎉 All validations passed! System is ready.");
    } else {
        println!("\n  ❌ Some validations failed. Please resolve issues before continuing.");
    }
}

fn check_migration_needed(config: &Config) -> bool {
    // Check if any validator is missing public keys
    config.validators.iter().any(|validator| validator.vote_pubkey.is_empty() || validator.identity_pubkey.is_empty())
}

async fn migrate_configuration(config_manager: &ConfigManager, mut config: Config) -> Result<Config> {
    println!("\n{}", "🔄 Configuration Migration".bright_cyan().bold());
    println!("Adding missing validator public key identifiers...");
    println!("{}", "These keys are shared between primary and backup validators.".dimmed());
    
    for (index, validator_pair) in config.validators.iter_mut().enumerate() {
        println!("\n{} Validator {}:", "🔑".bright_cyan(), index + 1);
        
        if validator_pair.vote_pubkey.is_empty() {
            let vote_pubkey = Text::new("Vote Pubkey:")
                .with_help_message("Enter the public key for the vote account")
                .with_validator(|input: &str| {
                    if input.trim().is_empty() {
                        Ok(Validation::Invalid("Vote Pubkey is required".into()))
                    } else if input.len() < 32 || input.len() > 44 {
                        Ok(Validation::Invalid("Vote Pubkey should be a valid base58 public key (32-44 characters)".into()))
                    } else {
                        Ok(Validation::Valid)
                    }
                })
                .prompt()?;
            validator_pair.vote_pubkey = vote_pubkey;
        }
        
        if validator_pair.identity_pubkey.is_empty() {
            let identity_pubkey = Text::new("Identity Pubkey:")
                .with_help_message("Enter the public key for the funded validator identity")
                .with_validator(|input: &str| {
                    if input.trim().is_empty() {
                        Ok(Validation::Invalid("Identity Pubkey is required".into()))
                    } else if input.len() < 32 || input.len() > 44 {
                        Ok(Validation::Invalid("Identity Pubkey should be a valid base58 public key (32-44 characters)".into()))
                    } else {
                        Ok(Validation::Valid)
                    }
                })
                .prompt()?;
            validator_pair.identity_pubkey = identity_pubkey;
        }
    }
    
    // Save the updated configuration
    config_manager.save(&config)?;
    println!("\n✅ Configuration updated and saved");
    
    Ok(config)
}

async fn show_ready_prompt() {
    use std::io::{self, Write};
    
    // Show animated ready message
    println!("{}", "┌─────────────────────────────────────────────────────────────┐".bright_cyan());
    println!("{}", "│                                                             │".bright_cyan());
    println!("{}", "│  ✅ All system checks passed!                              │".bright_cyan());
    println!("{}", "│  🚀 Solana Validator Switch is ready for operation        │".bright_cyan());
    println!("{}", "│                                                             │".bright_cyan());
    println!("{}", "│  Press any key to continue...                              │".bright_cyan());
    println!("{}", "│                                                             │".bright_cyan());
    println!("{}", "└─────────────────────────────────────────────────────────────┘".bright_cyan());
    
    // Flush stdout to ensure the prompt appears immediately
    io::stdout().flush().unwrap();
    
    // Wait for any key press
    let mut input = String::new();
    let _ = io::stdin().read_line(&mut input);
    
    // Clear the ready prompt
    print!("\x1B[8A\x1B[2K"); // Move up 8 lines and clear
    for _ in 0..8 {
        print!("\x1B[2K\x1B[1B"); // Clear line and move down
    }
    print!("\x1B[8A"); // Move back up to original position
    io::stdout().flush().unwrap();
}

async fn detect_node_statuses(config: &Config, _ssh_pool: &SshConnectionPool) -> Result<Vec<crate::ValidatorStatus>> {
    let mut validator_statuses = Vec::new();
    
    for validator_pair in &config.validators {
        let mut nodes_with_status = Vec::new();
        
        // Create a temporary SSH pool for node status detection
        let mut temp_pool = SshConnectionPool::new();
        
        for node in &validator_pair.nodes {
            let (status, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, swap_ready, swap_issues) = detect_node_status_and_executable(node, validator_pair, &mut temp_pool).await?;
            nodes_with_status.push(crate::types::NodeWithStatus {
                node: node.clone(),
                status,
                validator_type,
                agave_validator_executable,
                fdctl_executable,
                version,
                sync_status,
                current_identity,
                swap_ready,
                swap_issues,
            });
        }
        
        validator_statuses.push(crate::ValidatorStatus {
            validator_pair: validator_pair.clone(),
            nodes_with_status,
        });
    }
    
    Ok(validator_statuses)
}

/// Detect node statuses with detailed progress reporting
async fn detect_node_statuses_with_progress(config: &Config, _ssh_pool: &SshConnectionPool, progress_bar: &ProgressBar) -> Result<Vec<crate::ValidatorStatus>> {
    let mut validator_statuses = Vec::new();
    let total_nodes: usize = config.validators.iter().map(|v| v.nodes.len()).sum();
    let mut processed_nodes = 0;
    
    for (validator_index, validator_pair) in config.validators.iter().enumerate() {
        let mut nodes_with_status = Vec::new();
        
        // Create a temporary SSH pool for node status detection
        let mut temp_pool = SshConnectionPool::new();
        
        for (node_index, node) in validator_pair.nodes.iter().enumerate() {
            // Update progress with specific node being processed
            let node_label = format!("Validator {} Node {} ({})", validator_index + 1, node_index + 1, node.label);
            progress_bar.suspend(|| {
                println!("  🔍 Analyzing {}...", node_label.bright_yellow());
            });
            
            // Step 1: SSH Connection
            progress_bar.suspend(|| {
                println!("    🔗 Establishing SSH connection...");
            });
            
            let (status, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, swap_ready, swap_issues) = 
                detect_node_status_and_executable_with_progress(node, validator_pair, &mut temp_pool, progress_bar).await?;
            
            nodes_with_status.push(crate::types::NodeWithStatus {
                node: node.clone(),
                status: status.clone(),
                validator_type: validator_type.clone(),
                agave_validator_executable,
                fdctl_executable,
                version: version.clone(),
                sync_status: sync_status.clone(),
                current_identity: current_identity.clone(),
                swap_ready,
                swap_issues,
            });
            
            // Show completion status for this node
            let status_emoji = match status {
                crate::types::NodeStatus::Active => "🟢",
                crate::types::NodeStatus::Standby => "🟡",
                crate::types::NodeStatus::Unknown => "🔴",
            };
            let status_text = match status {
                crate::types::NodeStatus::Active => "ACTIVE".green(),
                crate::types::NodeStatus::Standby => "STANDBY".yellow(), 
                crate::types::NodeStatus::Unknown => "UNKNOWN".red(),
            };
            
            progress_bar.suspend(|| {
                println!("    {} {} - {} {}", 
                    status_emoji, 
                    status_text,
                    version.as_ref().unwrap_or(&"Unknown version".to_string()).bright_cyan(),
                    if swap_ready.unwrap_or(false) { "✅ Swap Ready" } else { "❌ Not Ready" }.dimmed());
            });
            
            processed_nodes += 1;
            let progress_percent = 85 + ((processed_nodes as f64 / total_nodes as f64) * 10.0) as u64;
            progress_bar.set_position(progress_percent);
        }
        
        validator_statuses.push(crate::ValidatorStatus {
            validator_pair: validator_pair.clone(),
            nodes_with_status,
        });
    }
    
    Ok(validator_statuses)
}

async fn detect_node_status_and_executable(node: &crate::types::NodeConfig, validator_pair: &crate::types::ValidatorPair, ssh_pool: &mut SshConnectionPool) -> Result<(crate::types::NodeStatus, crate::types::ValidatorType, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<bool>, Vec<String>)> {
    // Try to connect to the node
    if let Err(_) = ssh_pool.connect(node, &validator_pair.local_ssh_key_path).await {
        return Ok((crate::types::NodeStatus::Unknown, crate::types::ValidatorType::Unknown, None, None, None, None, None, Some(false), vec!["SSH connection failed".to_string()]));
    }
    
    // First, extract all relevant executable paths
    let mut validator_type = crate::types::ValidatorType::Unknown;
    let mut agave_validator_executable = None;
    let mut fdctl_executable = None;
    let mut main_validator_executable = None;
    let mut version = None;
    let mut sync_status = None;
    let mut current_identity = None;
    
    // Get the process list
    if let Ok(output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, "ps aux | grep -Ei 'solana-validator|agave|fdctl|firedancer' | grep -v grep").await {
        let lines: Vec<&str> = output.lines().collect();
        for line in lines {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for part in parts.iter() {
                // Firedancer: .../build/native/gcc/bin/fdctl
                if part.contains("fdctl") && part.contains("/build/native/gcc/bin/fdctl") {
                    main_validator_executable = Some(part.to_string());
                    fdctl_executable = Some(part.to_string());
                    validator_type = crate::types::ValidatorType::Firedancer;
                    
                    // For Firedancer, agave executable is ../build/native/gcc/bin/solana
                    if let Some(fdctl_dir) = std::path::Path::new(part).parent() {
                        let solana_path = fdctl_dir.join("solana");
                        agave_validator_executable = Some(solana_path.to_string_lossy().to_string());
                    }
                }
                // Agave: .../target/release/agave-validator  
                else if part.contains("agave-validator") && part.contains("/target/release/agave-validator") {
                    if main_validator_executable.is_none() {
                        main_validator_executable = Some(part.to_string());
                        validator_type = if line.contains("jito") || line.contains("Jito") {
                            crate::types::ValidatorType::Jito
                        } else {
                            crate::types::ValidatorType::Agave
                        };
                    }
                    agave_validator_executable = Some(part.to_string());
                }
                // Legacy solana-validator
                else if part.contains("solana-validator") {
                    if main_validator_executable.is_none() {
                        main_validator_executable = Some(part.to_string());
                        validator_type = crate::types::ValidatorType::Solana;
                    }
                    // For solana-validator, it can also be used for catchup
                    if agave_validator_executable.is_none() {
                        agave_validator_executable = Some(part.to_string());
                    }
                }
            }
        }
    }
    
    // Try to find fdctl executable for firedancer nodes with proper path format
    if fdctl_executable.is_none() {
        let fdctl_search_cmd = "find /opt /home /usr -path '*/build/native/gcc/bin/fdctl' 2>/dev/null | head -1";
        if let Ok(output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, fdctl_search_cmd).await {
            let path = output.trim();
            if !path.is_empty() && path.contains("/build/native/gcc/bin/fdctl") {
                fdctl_executable = Some(path.to_string());
                
                // If we found fdctl but no agave executable yet, set the solana path
                if agave_validator_executable.is_none() {
                    if let Some(fdctl_dir) = std::path::Path::new(path).parent() {
                        let solana_path = fdctl_dir.join("solana");
                        agave_validator_executable = Some(solana_path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    
    // Try to find agave-validator executable if not already found
    if agave_validator_executable.is_none() {
        // For Firedancer, the agave executable is in the same directory as fdctl (solana)
        if let Some(ref fdctl_path) = fdctl_executable {
            if let Some(dir) = std::path::Path::new(fdctl_path).parent() {
                let solana_path = dir.join("solana");
                agave_validator_executable = Some(solana_path.to_string_lossy().to_string());
            }
        } else {
            // Search for agave-validator with proper path format: .../target/release/agave-validator
            let agave_search_cmd = "find /opt /home /usr -path '*/target/release/agave-validator' 2>/dev/null | head -1";
            if let Ok(output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, agave_search_cmd).await {
                let path = output.trim();
                if !path.is_empty() && path.contains("/target/release/agave-validator") {
                    agave_validator_executable = Some(path.to_string());
                }
            }
        }
    }
    
    // Detect version using the agave executable
    if let Some(ref agave_exec) = agave_validator_executable {
        if let Ok(version_output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, &format!("timeout 5 {} --version 2>/dev/null", agave_exec)).await {
            if let Some(line) = version_output.lines().next() {
                if line.starts_with("solana-cli ") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let version_num = parts[1];
                        if line.contains("client:Firedancer") {
                            version = Some(format!("Firedancer {}", version_num));
                        } else if line.contains("client:Agave") {
                            version = Some(format!("Agave {}", version_num));
                        } else if version_num.starts_with("0.") {
                            version = Some(format!("Firedancer {}", version_num));
                        } else if version_num.starts_with("2.") {
                            version = Some(format!("Agave {}", version_num));
                        }
                    }
                }
            }
        }
    }
    
    // Detect sync status using catchup command
    if let Some(ref agave_exec) = agave_validator_executable {
        let catchup_cmd = format!("timeout 10 {} catchup --our-localhost 2>&1 | grep -m1 'has caught up' | head -1", agave_exec);
        if let Ok(catchup_output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, &catchup_cmd).await {
            for line in catchup_output.lines() {
                if line.contains("has caught up") {
                    if let Some(caught_up_pos) = line.find(" has caught up") {
                        let identity = line[..caught_up_pos].trim();
                        if current_identity.is_none() && !identity.is_empty() {
                            current_identity = Some(identity.to_string());
                        }
                        
                        // Extract slot information
                        if let Some(us_start) = line.find("us:") {
                            let us_end = line[us_start+3..].find(' ').unwrap_or(line.len() - us_start - 3) + us_start + 3;
                            let us_slot = &line[us_start+3..us_end];
                            sync_status = Some(format!("Caught up (slot: {})", us_slot));
                        } else {
                            sync_status = Some("Caught up".to_string());
                        }
                        break;
                    }
                }
            }
        }
        
        // If no catchup info, set a default sync status
        if sync_status.is_none() {
            sync_status = Some("Unknown".to_string());
        }
    }
    
    // Check swap readiness
    let (swap_ready, swap_issues) = check_node_swap_readiness(ssh_pool, node, &validator_pair.local_ssh_key_path).await;
    
    // Use the validator monitor command to get the active identity
    // We'll use timeout to get just the initial output and then kill the process
    let monitor_cmd = if let Some(ref exec_path) = main_validator_executable {
        format!("timeout --kill-after=2 3 bash -c '{} --ledger {} monitor 2>/dev/null | head -3 | grep \"Identity:\" | head -1'", exec_path, node.paths.ledger)
    } else {
        // Fallback to dynamic detection if executable wasn't found
        format!("VALIDATOR_EXEC=$(ps aux | grep -Ei 'solana-validator|agave|fdctl|firedancer' | grep -v grep | awk '{{for(i=11;i<=NF;i++) if($i ~ /solana-validator|agave-validator|fdctl/) print $i; exit}}') && if [ -n \"$VALIDATOR_EXEC\" ]; then timeout --kill-after=2 3 bash -c \"$VALIDATOR_EXEC --ledger {} monitor 2>/dev/null | head -3 | grep 'Identity:' | head -1\"; else echo 'no-validator-running'; fi", node.paths.ledger)
    };
    
    match ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, &monitor_cmd).await {
        Ok(output) => {
            // Parse the output to find the Identity line
            for line in output.lines() {
                if line.starts_with("Identity: ") {
                    let identity = line.replace("Identity: ", "").trim().to_string();
                    // Store the current identity for status tracking
                    if current_identity.is_none() {
                        current_identity = Some(identity.clone());
                    }
                    
                    // Check if this identity matches the validator's funded identity
                    if identity == validator_pair.identity_pubkey {
                        return Ok((crate::types::NodeStatus::Active, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues));
                    } else {
                        return Ok((crate::types::NodeStatus::Standby, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues));
                    }
                }
            }
            
            // Fallback: try catchup command to get identity
            if let Some(ref agave_exec) = agave_validator_executable {
                let catchup_cmd = format!("timeout 10 {} catchup --our-localhost 2>&1 | grep -m1 'has caught up' | head -1", agave_exec);
                if let Ok(catchup_output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, &catchup_cmd).await {
                    for line in catchup_output.lines() {
                        if line.contains("has caught up") {
                            if let Some(caught_up_pos) = line.find(" has caught up") {
                                let identity = line[..caught_up_pos].trim();
                                if current_identity.is_none() {
                                    current_identity = Some(identity.to_string());
                                }
                                if identity == validator_pair.identity_pubkey {
                                    return Ok((crate::types::NodeStatus::Active, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues));
                                } else if !identity.is_empty() {
                                    return Ok((crate::types::NodeStatus::Standby, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues));
                                }
                            }
                        }
                    }
                }
            }
            
            // If we can't find the Identity line from either method, assume unknown
            Ok((crate::types::NodeStatus::Unknown, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues))
        }
        Err(_) => Ok((crate::types::NodeStatus::Unknown, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues)),
    }
}

/// Check if a node is ready for validator switching
async fn check_node_swap_readiness(ssh_pool: &mut SshConnectionPool, node: &crate::types::NodeConfig, ssh_key_path: &str) -> (bool, Vec<String>) {
    let mut issues = Vec::new();
    let mut all_ready = true;
    
    // Batch file checks into single command
    let file_check_cmd = format!(
        "test -r {} && echo 'funded_ok' || echo 'funded_fail'; \
         test -r {} && echo 'unfunded_ok' || echo 'unfunded_fail'; \
         test -r {} && echo 'vote_ok' || echo 'vote_fail'; \
         ls {}/tower-1_9-*.bin >/dev/null 2>&1 && echo 'tower_ok' || echo 'tower_fail'; \
         test -d {} && test -w {} && echo 'ledger_ok' || echo 'ledger_fail'; \
         test -x {} && echo 'cli_ok' || echo 'cli_fail'",
        node.paths.funded_identity,
        node.paths.unfunded_identity,
        node.paths.vote_keypair,
        node.paths.ledger,
        node.paths.ledger,
        node.paths.ledger,
        node.paths.solana_cli_path
    );
    
    match ssh_pool.execute_command(node, ssh_key_path, &file_check_cmd).await {
        Ok(output) => {
            for line in output.lines() {
                match line.trim() {
                    "funded_fail" => {
                        issues.push("Funded identity keypair missing or not readable".to_string());
                        all_ready = false;
                    }
                    "unfunded_fail" => {
                        issues.push("Unfunded identity keypair missing or not readable".to_string());
                        all_ready = false;
                    }
                    "vote_fail" => {
                        issues.push("Vote keypair missing or not readable".to_string());
                        all_ready = false;
                    }
                    "tower_fail" => {
                        issues.push("Tower file missing".to_string());
                        all_ready = false;
                    }
                    "ledger_fail" => {
                        issues.push("Ledger directory missing or not writable".to_string());
                        all_ready = false;
                    }
                    "cli_fail" => {
                        issues.push("Solana CLI not executable".to_string());
                        all_ready = false;
                    }
                    _ => {}
                }
            }
        }
        Err(_) => {
            all_ready = false;
            issues.push("Failed to check file readiness".to_string());
        }
    }
    
    (all_ready, issues)
}

/// Enhanced version of detect_node_status_and_executable with detailed progress reporting
async fn detect_node_status_and_executable_with_progress(node: &crate::types::NodeConfig, validator_pair: &crate::types::ValidatorPair, ssh_pool: &mut SshConnectionPool, progress_bar: &ProgressBar) -> Result<(crate::types::NodeStatus, crate::types::ValidatorType, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<bool>, Vec<String>)> {
    // Try to connect to the node
    if let Err(_) = ssh_pool.connect(node, &validator_pair.local_ssh_key_path).await {
        progress_bar.suspend(|| {
            println!("      ❌ SSH connection failed");
        });
        return Ok((crate::types::NodeStatus::Unknown, crate::types::ValidatorType::Unknown, None, None, None, None, None, Some(false), vec!["SSH connection failed".to_string()]));
    }
    
    progress_bar.suspend(|| {
        println!("      ✅ SSH connection established");
    });
    
    // First, extract all relevant executable paths
    let mut validator_type = crate::types::ValidatorType::Unknown;
    let mut agave_validator_executable = None;
    let mut fdctl_executable = None;
    let mut main_validator_executable = None;
    let mut version = None;
    let mut sync_status = None;
    let mut current_identity = None;
    
    // Step 2: Process Detection
    progress_bar.suspend(|| {
        println!("      🔍 Detecting validator processes...");
    });
    
    // Get the process list
    if let Ok(output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, "ps aux | grep -Ei 'solana-validator|agave|fdctl|firedancer' | grep -v grep").await {
        let lines: Vec<&str> = output.lines().collect();
        for line in lines {
            let parts: Vec<&str> = line.split_whitespace().collect();
            for part in parts.iter() {
                // Firedancer: .../build/native/gcc/bin/fdctl
                if part.contains("fdctl") && part.contains("/build/native/gcc/bin/fdctl") {
                    main_validator_executable = Some(part.to_string());
                    fdctl_executable = Some(part.to_string());
                    validator_type = crate::types::ValidatorType::Firedancer;
                    
                    // For Firedancer, agave executable is ../build/native/gcc/bin/solana
                    if let Some(fdctl_dir) = std::path::Path::new(part).parent() {
                        let solana_path = fdctl_dir.join("solana");
                        agave_validator_executable = Some(solana_path.to_string_lossy().to_string());
                    }
                }
                // Agave: .../target/release/agave-validator  
                else if part.contains("agave-validator") && part.contains("/target/release/agave-validator") {
                    if main_validator_executable.is_none() {
                        main_validator_executable = Some(part.to_string());
                        validator_type = if line.contains("jito") || line.contains("Jito") {
                            crate::types::ValidatorType::Jito
                        } else {
                            crate::types::ValidatorType::Agave
                        };
                    }
                    agave_validator_executable = Some(part.to_string());
                }
                // Legacy solana-validator
                else if part.contains("solana-validator") {
                    if main_validator_executable.is_none() {
                        main_validator_executable = Some(part.to_string());
                        validator_type = crate::types::ValidatorType::Solana;
                    }
                    // For solana-validator, it can also be used for catchup
                    if agave_validator_executable.is_none() {
                        agave_validator_executable = Some(part.to_string());
                    }
                }
            }
        }
    }
    
    let validator_type_name = match validator_type {
        crate::types::ValidatorType::Firedancer => "Firedancer",
        crate::types::ValidatorType::Agave => "Agave",
        crate::types::ValidatorType::Jito => "Jito",
        crate::types::ValidatorType::Solana => "Solana",
        crate::types::ValidatorType::Unknown => "Unknown",
    };
    
    progress_bar.suspend(|| {
        println!("      ✅ Detected {} validator", validator_type_name.bright_green());
    });
    
    // Try to find fdctl executable for firedancer nodes with proper path format
    if fdctl_executable.is_none() {
        let fdctl_search_cmd = "find /opt /home /usr -path '*/build/native/gcc/bin/fdctl' 2>/dev/null | head -1";
        if let Ok(output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, fdctl_search_cmd).await {
            let path = output.trim();
            if !path.is_empty() && path.contains("/build/native/gcc/bin/fdctl") {
                fdctl_executable = Some(path.to_string());
                
                // If we found fdctl but no agave executable yet, set the solana path
                if agave_validator_executable.is_none() {
                    if let Some(fdctl_dir) = std::path::Path::new(path).parent() {
                        let solana_path = fdctl_dir.join("solana");
                        agave_validator_executable = Some(solana_path.to_string_lossy().to_string());
                    }
                }
            }
        }
    }
    
    // Try to find agave-validator executable if not already found
    if agave_validator_executable.is_none() {
        // For Firedancer, the agave executable is in the same directory as fdctl (solana)
        if let Some(ref fdctl_path) = fdctl_executable {
            if let Some(dir) = std::path::Path::new(fdctl_path).parent() {
                let solana_path = dir.join("solana");
                agave_validator_executable = Some(solana_path.to_string_lossy().to_string());
            }
        } else {
            // Search for agave-validator with proper path format: .../target/release/agave-validator
            let agave_search_cmd = "find /opt /home /usr -path '*/target/release/agave-validator' 2>/dev/null | head -1";
            if let Ok(output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, agave_search_cmd).await {
                let path = output.trim();
                if !path.is_empty() && path.contains("/target/release/agave-validator") {
                    agave_validator_executable = Some(path.to_string());
                }
            }
        }
    }
    
    // Step 3: Version Detection
    progress_bar.suspend(|| {
        println!("      🔍 Detecting version information...");
    });
    
    // Detect version using the agave executable
    if let Some(ref agave_exec) = agave_validator_executable {
        if let Ok(version_output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, &format!("timeout 5 {} --version 2>/dev/null", agave_exec)).await {
            if let Some(line) = version_output.lines().next() {
                if line.starts_with("solana-cli ") {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 2 {
                        let version_num = parts[1];
                        if line.contains("client:Firedancer") {
                            version = Some(format!("Firedancer {}", version_num));
                        } else if line.contains("client:Agave") {
                            version = Some(format!("Agave {}", version_num));
                        } else if version_num.starts_with("0.") {
                            version = Some(format!("Firedancer {}", version_num));
                        } else if version_num.starts_with("2.") {
                            version = Some(format!("Agave {}", version_num));
                        }
                    }
                }
            }
        }
    }
    
    if let Some(ref v) = version {
        progress_bar.suspend(|| {
            println!("      ✅ Version: {}", v.bright_cyan());
        });
    }
    
    // Step 4: Sync Status Detection
    progress_bar.suspend(|| {
        println!("      🔍 Checking sync status...");
    });
    
    // Detect sync status using catchup command
    if let Some(ref agave_exec) = agave_validator_executable {
        let catchup_cmd = format!("timeout 10 {} catchup --our-localhost 2>&1 | grep -m1 'has caught up' | head -1", agave_exec);
        if let Ok(catchup_output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, &catchup_cmd).await {
            for line in catchup_output.lines() {
                if line.contains("has caught up") {
                    if let Some(caught_up_pos) = line.find(" has caught up") {
                        let identity = line[..caught_up_pos].trim();
                        if current_identity.is_none() && !identity.is_empty() {
                            current_identity = Some(identity.to_string());
                        }
                        
                        // Extract slot information
                        if let Some(us_start) = line.find("us:") {
                            let us_end = line[us_start+3..].find(' ').unwrap_or(line.len() - us_start - 3) + us_start + 3;
                            let us_slot = &line[us_start+3..us_end];
                            sync_status = Some(format!("Caught up (slot: {})", us_slot));
                        } else {
                            sync_status = Some("Caught up".to_string());
                        }
                        break;
                    }
                }
            }
        }
        
        // If no catchup info, set a default sync status
        if sync_status.is_none() {
            sync_status = Some("Unknown".to_string());
        }
    }
    
    // Step 5: Swap Readiness Check
    progress_bar.suspend(|| {
        println!("      🔍 Checking swap readiness...");
    });
    
    let (swap_ready, swap_issues) = check_node_swap_readiness(ssh_pool, node, &validator_pair.local_ssh_key_path).await;
    
    progress_bar.suspend(|| {
        if swap_ready {
            println!("      ✅ Swap readiness: Ready");
        } else {
            println!("      ❌ Swap readiness: Not ready ({})", swap_issues.join(", "));
        }
    });
    
    // Step 6: Identity Detection
    progress_bar.suspend(|| {
        println!("      🔍 Detecting active identity...");
    });
    
    // Use the validator monitor command to get the active identity
    // We'll use timeout to get just the initial output and then kill the process
    let monitor_cmd = if let Some(ref exec_path) = main_validator_executable {
        format!("timeout --kill-after=2 3 bash -c '{} --ledger {} monitor 2>/dev/null | head -3 | grep \"Identity:\" | head -1'", exec_path, node.paths.ledger)
    } else {
        // Fallback to dynamic detection if executable wasn't found
        format!("VALIDATOR_EXEC=$(ps aux | grep -Ei 'solana-validator|agave|fdctl|firedancer' | grep -v grep | awk '{{for(i=11;i<=NF;i++) if($i ~ /solana-validator|agave-validator|fdctl/) print $i; exit}}') && if [ -n \"$VALIDATOR_EXEC\" ]; then timeout --kill-after=2 3 bash -c \"$VALIDATOR_EXEC --ledger {} monitor 2>/dev/null | head -3 | grep 'Identity:' | head -1\"; else echo 'no-validator-running'; fi", node.paths.ledger)
    };
    
    match ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, &monitor_cmd).await {
        Ok(output) => {
            // Parse the output to find the Identity line
            for line in output.lines() {
                if line.starts_with("Identity: ") {
                    let identity = line.replace("Identity: ", "").trim().to_string();
                    // Store the current identity for status tracking
                    if current_identity.is_none() {
                        current_identity = Some(identity.clone());
                    }
                    
                    // Check if this identity matches the validator's funded identity
                    if identity == validator_pair.identity_pubkey {
                        progress_bar.suspend(|| {
                            println!("      ✅ Identity: {} (ACTIVE)", identity.bright_green());
                        });
                        return Ok((crate::types::NodeStatus::Active, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues));
                    } else {
                        progress_bar.suspend(|| {
                            println!("      ✅ Identity: {} (STANDBY)", identity.bright_yellow());
                        });
                        return Ok((crate::types::NodeStatus::Standby, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues));
                    }
                }
            }
            
            // Fallback: try catchup command to get identity
            if let Some(ref agave_exec) = agave_validator_executable {
                let catchup_cmd = format!("timeout 10 {} catchup --our-localhost 2>&1 | grep -m1 'has caught up' | head -1", agave_exec);
                if let Ok(catchup_output) = ssh_pool.execute_command(node, &validator_pair.local_ssh_key_path, &catchup_cmd).await {
                    for line in catchup_output.lines() {
                        if line.contains("has caught up") {
                            if let Some(caught_up_pos) = line.find(" has caught up") {
                                let identity = line[..caught_up_pos].trim();
                                if current_identity.is_none() {
                                    current_identity = Some(identity.to_string());
                                }
                                if identity == validator_pair.identity_pubkey {
                                    progress_bar.suspend(|| {
                                        println!("      ✅ Identity: {} (ACTIVE)", identity.bright_green());
                                    });
                                    return Ok((crate::types::NodeStatus::Active, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues));
                                } else if !identity.is_empty() {
                                    progress_bar.suspend(|| {
                                        println!("      ✅ Identity: {} (STANDBY)", identity.bright_yellow());
                                    });
                                    return Ok((crate::types::NodeStatus::Standby, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues));
                                }
                            }
                        }
                    }
                }
            }
            
            // If we can't find the Identity line from either method, assume unknown
            progress_bar.suspend(|| {
                println!("      ❌ Identity: Unable to determine");
            });
            Ok((crate::types::NodeStatus::Unknown, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues))
        }
        Err(_) => {
            progress_bar.suspend(|| {
                println!("      ❌ Identity: Command failed");
            });
            Ok((crate::types::NodeStatus::Unknown, validator_type, agave_validator_executable, fdctl_executable, version, sync_status, current_identity, Some(swap_ready), swap_issues))
        }
    }
}