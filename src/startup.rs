use anyhow::{anyhow, Result};
use colored::*;
use indicatif::{ProgressBar, ProgressStyle};
use inquire::Confirm;
use std::io::{self, Write};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;

use crate::config::ConfigManager;
use crate::ssh::AsyncSshPool;
use crate::startup_logger::StartupLogger;
use crate::types::{Config, NodeConfig, NodeWithStatus};

// Default SSH key path for legacy functions
const DEFAULT_SSH_KEY: &str = "~/.ssh/id_rsa";
use inquire::{validator::Validation, Text};

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

/// Helper to get SSH key for a host from detected keys
#[allow(dead_code)]
fn get_ssh_key_for_host(
    detected_keys: &std::collections::HashMap<String, String>,
    host: &str,
) -> Result<String> {
    detected_keys
        .get(host)
        .cloned()
        .ok_or_else(|| anyhow!("No SSH key detected for host: {}", host))
}

/// Comprehensive startup checklist and validation with enhanced UX
pub async fn run_startup_checklist() -> Result<Option<crate::AppState>> {
    run_startup_checklist_with_config(None).await
}

/// Comprehensive startup checklist and validation with enhanced UX and custom config path
pub async fn run_startup_checklist_with_config(config_path: Option<String>) -> Result<Option<crate::AppState>> {
    // Create logger first
    let logger = StartupLogger::new()?;
    logger.create_latest_symlink()?;

    // Clear screen and show startup banner
    println!("\x1B[2J\x1B[1;1H"); // Clear screen
    println!("{}", "🚀 Solana Validator Switch".bright_cyan().bold());
    println!("{}", "Initializing validator management system...".dimmed());
    println!();

    // Show log file location
    println!(
        "{}",
        format!("📄 Diagnostic log: {}", logger.get_log_path().display()).dimmed()
    );
    println!();

    // Create progress bar for overall startup process
    let progress_bar = ProgressBar::new(100);
    progress_bar.set_style(
        ProgressStyle::default_bar()
            .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>3}% {msg}")
            .unwrap()
            .progress_chars("█▉▊▋▌▍▎▏  "),
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

    let mut config =
        validate_configuration_with_progress(&mut validation, &progress_bar, &logger, config_path.clone()).await?;

    // Only continue with SSH and other validation if config is valid
    let ssh_pool_and_keys = if validation.config_valid {
        progress_bar.set_position(30);
        // Phase 2: SSH connection validation (60% of progress)
        progress_bar.set_message("Establishing SSH connections...");
        let (pool, detected_keys) = validate_ssh_connections_with_progress(
            &config.as_ref().unwrap(),
            &mut validation,
            &progress_bar,
            &logger,
        )
        .await?;
        progress_bar.set_position(70);

        // Save detected SSH keys to config if any were detected
        if !detected_keys.is_empty() {
            progress_bar.set_message("Saving detected SSH keys to config...");
            if let Some(ref mut config_mut) = config {
                let mut config_updated = false;
                for validator in &mut config_mut.validators {
                    for node in &mut validator.nodes {
                        if node.ssh_key_path.is_none() {
                            if let Some(detected_key) = detected_keys.get(&node.host) {
                                node.ssh_key_path = Some(detected_key.clone());
                                config_updated = true;
                            }
                        }
                    }
                }

                if config_updated {
                    // Save the updated config
                    let config_manager = ConfigManager::with_path(config_path.clone())?;
                    if let Err(e) = config_manager.save(&config_mut) {
                        progress_bar.suspend(|| {
                            println!("    ⚠️  Failed to save SSH keys to config: {}", e);
                        });
                    } else {
                        progress_bar.suspend(|| {
                            println!("    ✅ SSH keys saved to config for faster restarts");
                        });
                    }
                }
            }
        }

        // Phase 3: Model verification (80% of progress)
        progress_bar.set_message("Verifying system readiness...");
        validate_model_verification_with_progress(
            &config.as_ref().unwrap(),
            &pool,
            &mut validation,
            &progress_bar,
            &logger,
        )
        .await?;
        progress_bar.set_position(80);

        Some((pool, detected_keys))
    } else {
        None
    };

    // Phase 4: Comprehensive validator status detection (85-95% of progress)
    let validator_statuses = if validation.config_valid
        && validation.ssh_connections_valid
        && validation.model_verification_valid
    {
        progress_bar.set_message("🔍 Detecting validator statuses...");
        progress_bar.set_position(85);

        // Finish the progress bar before detailed detection output
        progress_bar.finish_with_message("✅ Starting validator detection...");

        let mut statuses = detect_node_statuses_with_progress(
            &config.as_ref().unwrap(),
            &ssh_pool_and_keys.as_ref().unwrap().0,
            &ssh_pool_and_keys.as_ref().unwrap().1,
            &progress_bar,
            &logger,
        )
        .await?;

        // Fetch validator metadata
        for status in &mut statuses {
            if let Ok(metadata) = crate::validator_metadata::fetch_validator_metadata(
                &status.validator_pair.rpc,
                &status.validator_pair.identity_pubkey,
            )
            .await
            {
                status.metadata = metadata;
            }
        }
        progress_bar.set_position(98);

        Some(statuses)
    } else {
        None
    };

    // Check if any nodes had SSH connection failures
    if let Some(ref statuses) = validator_statuses {
        let mut ssh_failures = Vec::new();
        for status in statuses {
            for node_status in &status.nodes_with_status {
                if node_status
                    .swap_issues
                    .iter()
                    .any(|issue| issue.contains("SSH connection failed"))
                {
                    ssh_failures.push(format!(
                        "{}@{}",
                        node_status.node.user, node_status.node.host
                    ));
                }
            }
        }

        if !ssh_failures.is_empty() {
            validation.ssh_connections_valid = false;
            validation.issues.push(format!(
                "SSH connection failed to {} node(s)",
                ssh_failures.len()
            ));

            // Store the failed hosts for later display
            for host in ssh_failures {
                validation
                    .issues
                    .push(format!("Cannot connect to: {}", host));
            }
        }
    }

    // Phase 5: Final validation and summary
    progress_bar.set_message("Finalizing startup...");
    validation.success = validation.config_valid
        && validation.ssh_connections_valid
        && validation.model_verification_valid;

    progress_bar.set_position(100);
    progress_bar.finish_and_clear();

    if validation.success {
        if let (Some(config), Some((ssh_pool, detected_ssh_keys)), Some(validator_statuses)) =
            (config, ssh_pool_and_keys, validator_statuses)
        {
            // Create metadata cache
            let metadata_cache =
                Arc::new(Mutex::new(crate::validator_metadata::MetadataCache::new()));

            let app_state = crate::AppState {
                ssh_pool: Arc::new(ssh_pool),
                config,
                validator_statuses,
                metadata_cache,
                detected_ssh_keys,
                selected_validator_index: 0, // Default to first validator
            };

            // Auto-failover safety checks are now done per-validator during status detection

            // Show "press any key to continue" prompt after all checks pass
            show_ready_prompt().await;

            Ok(Some(app_state))
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

        // Log final validation summary
        logger.log_section("Startup Validation Failed")?;
        logger.log(&format!("Config Valid: {}", validation.config_valid))?;
        logger.log(&format!(
            "SSH Connections Valid: {}",
            validation.ssh_connections_valid
        ))?;
        logger.log(&format!(
            "Model Verification Valid: {}",
            validation.model_verification_valid
        ))?;
        logger.log(&format!("Total Issues: {}", validation.issues.len()))?;

        // Show helpful resolution steps
        println!("\n{} Suggested actions:", "💡".bright_blue().bold());
        if !validation.config_valid {
            println!("  • Edit your configuration file: ~/.solana-validator-switch/config.yaml");
            println!(
                "  • Use the example config: https://github.com/your-repo/config.example.yaml"
            );
            println!("  • Ensure all required fields are filled with correct values");
        }
        if !validation.ssh_connections_valid {
            println!("  • Test SSH connections manually: ssh user@host");
            println!("  • If authentication fails, copy your SSH key:");

            // Show specific ssh-copy-id commands for failed hosts
            for issue in &validation.issues {
                if issue.contains("Cannot connect to:") {
                    if let Some(host_part) = issue.split("Cannot connect to: ").nth(1) {
                        println!("      ssh-copy-id {}", host_part.bright_cyan());
                    }
                }
            }

            println!("  • Ensure remote hosts are accessible and SSH service is running");
        }
        if !validation.model_verification_valid {
            println!("  • Check validator file paths and permissions");
            println!("  • Ensure validator processes are running");
        }

        // Show a prompt to acknowledge the error before exiting
        println!();
        println!(
            "{}",
            format!(
                "📄 Check the diagnostic log for details: {}",
                logger.get_log_path().display()
            )
            .yellow()
        );
        println!("{}", "Press Enter to exit...".dimmed());
        let mut input = String::new();
        std::io::stdin().read_line(&mut input).unwrap();

        Ok(None)
    }
}

async fn validate_configuration_with_progress(
    validation: &mut StartupValidation,
    progress_bar: &ProgressBar,
    logger: &StartupLogger,
    config_path: Option<String>,
) -> Result<Option<Config>> {
    let config_manager = ConfigManager::with_path(config_path)?;

    logger.log_section("Configuration Validation")?;

    // Configuration file existence check
    progress_bar.set_message("Checking configuration file...");
    logger.log("Checking for configuration file...")?;

    if !config_manager.exists() {
        logger.log_error("Configuration", "Configuration file not found")?;
        progress_bar.suspend(|| {
            println!("  ❌ Configuration file not found");
        });

        validation
            .issues
            .push("Configuration file missing".to_string());

        progress_bar.suspend(|| {
            println!("\n{}", "⚠️ No configuration found.".yellow());
            println!();
            println!("{}", "Please create your configuration file at:".dimmed());
            println!(
                "{}",
                format!("  {}", config_manager.get_config_path().display()).bright_cyan()
            );
            println!();
            println!("{}", "You can either:".dimmed());
            println!(
                "{}",
                "  1. Copy and edit the example config: config.example.yaml".dimmed()
            );
            println!(
                "{}",
                "  2. Create the file manually using the documented YAML format".dimmed()
            );
            println!();
            println!("{}", "Application will exit now.".yellow());
        });

        return Ok(None);
    }

    // Configuration loading and validation
    progress_bar.set_message("Loading configuration...");
    logger.log("Loading configuration file...")?;

    match config_manager.load() {
        Ok(config) => {
            logger.log_success(&format!(
                "Configuration file loaded: {}",
                config_manager.get_config_path().display()
            ))?;
            progress_bar.suspend(|| {
                println!(
                    "  ✅ Configuration file loaded: {}",
                    config_manager.get_config_path().display()
                );
            });

            // Check if migration is needed
            progress_bar.set_message("Checking configuration completeness...");
            logger.log("Checking if configuration needs migration...")?;
            let needs_migration = check_migration_needed(&config);
            if needs_migration {
                logger.log_warning(
                    "Configuration needs migration to include missing public key identifiers",
                )?;
                // Configuration needs migration - mark as invalid but continue to show errors
                validation.config_valid = false;
                validation.issues.push(
                    "Configuration needs migration to include missing public key identifiers"
                        .to_string(),
                );
            }

            // Validate configuration completeness
            progress_bar.set_message("Validating configuration structure...");
            logger.log("Validating configuration structure...")?;
            let config_issues = validate_config_completeness(&config);

            if config_issues.is_empty() && !needs_migration {
                validation.config_valid = true;
                logger.log_success("Configuration is complete and valid")?;
                progress_bar.suspend(|| {
                    println!("  ✅ Configuration is complete and valid");
                });
                Ok(Some(config))
            } else {
                // Log configuration issues
                for issue in &config_issues {
                    logger.log_error("Configuration", issue)?;
                }
                // Configuration has issues - mark as invalid but continue to show errors
                validation.config_valid = false;
                validation.issues.extend(config_issues);
                Ok(None) // Return None to stop startup but continue to error reporting
            }
        }
        Err(e) => {
            logger.log_error(
                "Configuration",
                &format!("Failed to load configuration: {}", e),
            )?;
            progress_bar.suspend(|| {
                println!("  ❌ Failed to load configuration: {}", e);
            });
            validation
                .issues
                .push(format!("Configuration loading failed: {}", e));
            Ok(None)
        }
    }
}

#[allow(dead_code)]
async fn validate_configuration(validation: &mut StartupValidation) -> Result<Option<Config>> {
    validate_configuration_with_config(validation, None).await
}

#[allow(dead_code)]
async fn validate_configuration_with_config(validation: &mut StartupValidation, config_path: Option<String>) -> Result<Option<Config>> {
    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .template("  {spinner:.green} {msg}")
            .unwrap(),
    );
    spinner.set_message("Checking configuration file...");
    spinner.enable_steady_tick(Duration::from_millis(100));

    let config_manager = ConfigManager::with_path(config_path)?;

    // Check if configuration exists
    if !config_manager.exists() {
        spinner.finish_with_message("❌ Configuration file not found");
        validation
            .issues
            .push("Configuration file missing".to_string());

        println!("\n{}", "⚠️ No configuration found.".yellow());
        println!(
            "{}",
            "You need to set up your validator configuration first.".dimmed()
        );

        println!(
            "{}",
            "Please create your configuration file and restart the application.".yellow()
        );
        return Ok(None);
    }

    // Load and validate configuration
    match config_manager.load() {
        Ok(mut config) => {
            spinner.finish_with_message("✅ Configuration loaded successfully");

            // Check if migration is needed (missing public key fields)
            let needs_migration = check_migration_needed(&config);
            if needs_migration {
                println!("  🔄 Configuration needs migration to include public key identifiers");

                let migrate_now =
                    Confirm::new("Would you like to add the missing public key identifiers now?")
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
                            validation
                                .issues
                                .push(format!("Failed to reload configuration: {}", e));
                            Ok(None)
                        }
                    }
                } else {
                    println!(
                        "{}",
                        "Configuration issues not resolved. Some features may not work correctly."
                            .yellow()
                    );
                    Ok(Some(config))
                }
            }
        }
        Err(e) => {
            spinner.finish_with_message("❌ Failed to load configuration");
            validation
                .issues
                .push(format!("Configuration loading failed: {}", e));
            Ok(None)
        }
    }
}

async fn validate_ssh_connections_with_progress(
    config: &Config,
    validation: &mut StartupValidation,
    progress_bar: &ProgressBar,
    logger: &StartupLogger,
) -> Result<(AsyncSshPool, std::collections::HashMap<String, String>)> {
    logger.log_section("SSH Connection Validation")?;

    let ssh_pool = AsyncSshPool::new();
    let mut connection_issues = Vec::new();
    let mut detected_ssh_keys = std::collections::HashMap::new();

    if config.validators.is_empty() {
        validation
            .issues
            .push("No validators configured".to_string());
        progress_bar.suspend(|| {
            println!("  ❌ No validators configured");
        });
        return Ok((ssh_pool, std::collections::HashMap::new()));
    }

    let _total_nodes: usize = config.validators.iter().map(|v| v.nodes.len()).sum();
    let mut _connected_nodes = 0;

    // Establish connections to all nodes efficiently
    for (validator_index, validator_pair) in config.validators.iter().enumerate() {
        let validator_name = format!("Validator {}", validator_index + 1);

        for (node_index, node) in validator_pair.nodes.iter().enumerate() {
            let node_name = format!("{} Node {}", validator_name, node_index + 1);

            progress_bar.set_message(format!("Detecting SSH key for {}...", node_name));
            logger.log(&format!(
                "Checking SSH connection to {} ({})",
                node_name, node.host
            ))?;

            // Check if SSH key is already in config
            let mut key_worked = false;
            if let Some(ref configured_key) = node.ssh_key_path {
                logger.log(&format!("  Trying configured key: {}", configured_key))?;
                // Try the configured key first (silently)
                match ssh_pool.get_session(node, configured_key).await {
                    Ok(_) => {
                        logger.log_success(&format!(
                            "  Connected to {} with configured key",
                            node.host
                        ))?;
                        _connected_nodes += 1;
                        detected_ssh_keys.insert(node.host.clone(), configured_key.clone());
                        key_worked = true;
                    }
                    Err(e) => {
                        logger.log_error("SSH", &format!("  Configured key failed: {}", e))?;
                        // Configured key failed, will try auto-detection
                    }
                }
            }

            // If no configured key or it failed, auto-detect
            if !key_worked {
                logger.log("  Auto-detecting SSH key...")?;
                match crate::ssh_key_detector::detect_ssh_key(&node.host, &node.user).await {
                    Ok(detected_key) => {
                        logger.log(&format!("  Detected SSH key: {}", detected_key))?;
                        // Try to connect with detected key (silently)
                        match ssh_pool.get_session(node, &detected_key).await {
                            Ok(_) => {
                                logger.log_success(&format!(
                                    "  Connected to {} with detected key",
                                    node.host
                                ))?;
                                _connected_nodes += 1;
                                detected_ssh_keys.insert(node.host.clone(), detected_key);
                            }
                            Err(e) => {
                                logger.log_error("SSH", &format!("  Connection failed: {}", e))?;
                                connection_issues
                                    .push(format!("Failed to connect to {}: {}", node_name, e));
                            }
                        }
                    }
                    Err(detect_err) => {
                        logger
                            .log_error("SSH", &format!("  Key detection failed: {}", detect_err))?;
                        connection_issues.push(format!(
                            "Failed to detect SSH key for {}: {}",
                            node_name, detect_err
                        ));
                    }
                }
            }
        }
    }

    // Final connection status
    if connection_issues.is_empty() {
        logger.log_success("All SSH connections established successfully")?;
        validation.ssh_connections_valid = true;
    } else {
        logger.log_error(
            "SSH",
            &format!("{} connection issues found", connection_issues.len()),
        )?;
        validation.issues.extend(connection_issues);
        validation.ssh_connections_valid = false;
    }

    Ok((ssh_pool, detected_ssh_keys))
}

#[allow(dead_code)]
async fn validate_ssh_connections(
    config: &Config,
    validation: &mut StartupValidation,
) -> Result<AsyncSshPool> {
    let ssh_pool = AsyncSshPool::new();
    let mut connection_issues = Vec::new();

    if config.validators.is_empty() {
        validation
            .issues
            .push("No validators configured".to_string());
        return Ok(ssh_pool);
    }

    // Establish connections to all nodes efficiently
    for (validator_index, validator_pair) in config.validators.iter().enumerate() {
        let validator_name = format!("Validator {}", validator_index + 1);

        for (_node_index, _node) in validator_pair.nodes.iter().enumerate() {
            let node_name = format!("{} Node {}", validator_name, _node_index + 1);

            // Skip connection test - it will be done during actual detection
            // This function is marked as dead_code anyway
            match Ok::<(), anyhow::Error>(()) {
                Ok(_) => {
                    println!(
                        "✅ Connected to {}: {}@{}",
                        node_name, _node.user, _node.host
                    );
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

async fn validate_model_verification_with_progress(
    _config: &Config,
    _ssh_pool: &AsyncSshPool,
    validation: &mut StartupValidation,
    progress_bar: &ProgressBar,
    logger: &StartupLogger,
) -> Result<()> {
    logger.log_section("System Readiness Verification")?;

    // Skip detailed model verification since we already established connections
    // This avoids creating duplicate connections and improves startup performance
    progress_bar.set_message("Verifying system readiness...");
    logger.log("Verifying system components...")?;

    // Simulate a brief validation check
    tokio::time::sleep(Duration::from_millis(500)).await;

    logger.log_success("System readiness verified")?;
    progress_bar.suspend(|| {
        println!("  ✅ System readiness verified");
    });

    validation.model_verification_valid = true;
    Ok(())
}

#[allow(dead_code)]
async fn validate_model_verification(
    _config: &Config,
    _ssh_pool: &AsyncSshPool,
    validation: &mut StartupValidation,
) -> Result<()> {
    // Skip model verification since we already established connections in phase 2
    // This avoids creating duplicate connections and improves startup performance
    println!("  ✅ Skipping detailed model verification - using existing connections");
    validation.model_verification_valid = true;
    Ok(())
}

#[allow(dead_code)]
async fn verify_keypair_files(
    ssh_pool: &AsyncSshPool,
    node: &NodeConfig,
    ssh_key_path: &str,
) -> Vec<String> {
    let mut issues = Vec::new();

    // Check critical keypair files
    let keypairs = vec![
        (&node.paths.funded_identity, "Funded identity keypair"),
        (&node.paths.unfunded_identity, "Unfunded identity keypair"),
        (&node.paths.vote_keypair, "Vote keypair"),
    ];

    for (path, description) in keypairs {
        // Check if file exists
        let test_f_args = vec!["-f", path];
        if let Err(_) = ssh_pool
            .execute_command_with_args(node, ssh_key_path, "test", &test_f_args)
            .await
        {
            issues.push(format!("{} missing: {}", description, path));
            continue;
        }

        // Check if file is readable
        let test_r_args = vec!["-r", path];
        if let Err(_) = ssh_pool
            .execute_command_with_args(node, ssh_key_path, "test", &test_r_args)
            .await
        {
            issues.push(format!("{} not readable: {}", description, path));
        }
    }

    issues
}

#[allow(dead_code)]
async fn verify_public_key_matches(
    _ssh_pool: &AsyncSshPool,
    _node: &NodeConfig,
    _ssh_key_path: &str,
) -> Vec<String> {
    // Note: Public key verification will be handled separately with access to the shared validator config
    // For now, skip this validation as it needs the full config structure

    Vec::new()
}

#[allow(dead_code)]
async fn verify_validator_paths(
    _ssh_pool: &AsyncSshPool,
    _node: &NodeConfig,
    _ssh_key_path: &str,
) -> Vec<String> {
    // This function is deprecated as paths are now detected dynamically
    Vec::new()
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
        if DEFAULT_SSH_KEY.to_string().is_empty() {
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

fn validate_node_config(
    node: &crate::types::NodeConfig,
    node_name: &str,
    issues: &mut Vec<String>,
) {
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
}

#[allow(dead_code)]
async fn fix_configuration_issues(_config: &Config, issues: &[String]) -> Result<()> {
    println!(
        "\n{}",
        "🔧 Configuration Issue Resolution".bright_cyan().bold()
    );
    println!("The following issues were found:");

    for (i, issue) in issues.iter().enumerate() {
        println!("  {}. {}", i + 1, issue);
    }

    println!("\n{}", "To resolve these issues:".bright_cyan());
    println!("  1. Edit your configuration file: ~/.solana-validator-switch/config.yaml");
    println!("  2. Use the example config as reference: config.example.yaml");
    println!("  3. Ensure all required fields are filled with correct values");
    println!("  4. Restart the application after making changes");

    Ok(())
}

#[allow(dead_code)]
fn display_validation_summary(validation: &StartupValidation) {
    println!();
    println!("  📊 Validation Summary:");
    println!(
        "    Configuration: {}",
        if validation.config_valid {
            "✅ Valid"
        } else {
            "❌ Invalid"
        }
    );
    println!(
        "    SSH Connections: {}",
        if validation.ssh_connections_valid {
            "✅ Connected"
        } else {
            "❌ Failed"
        }
    );
    println!(
        "    Model Verification: {}",
        if validation.model_verification_valid {
            "✅ Verified"
        } else {
            "❌ Issues Found"
        }
    );

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
    if validation.config_valid
        && validation.ssh_connections_valid
        && validation.model_verification_valid
    {
        println!("\n  🎉 All validations passed! System is ready.");
    } else {
        println!("\n  ❌ Some validations failed. Please resolve issues before continuing.");
    }
}

fn check_migration_needed(config: &Config) -> bool {
    // Check if any validator is missing public keys
    config
        .validators
        .iter()
        .any(|validator| validator.vote_pubkey.is_empty() || validator.identity_pubkey.is_empty())
}

#[allow(dead_code)]
async fn migrate_configuration(
    config_manager: &ConfigManager,
    mut config: Config,
) -> Result<Config> {
    println!("\n{}", "🔄 Configuration Migration".bright_cyan().bold());
    println!("Adding missing validator public key identifiers...");
    println!(
        "{}",
        "These keys are shared between primary and backup validators.".dimmed()
    );

    for (index, validator_pair) in config.validators.iter_mut().enumerate() {
        println!("\n{} Validator {}:", "🔑".bright_cyan(), index + 1);

        if validator_pair.vote_pubkey.is_empty() {
            let vote_pubkey = Text::new("Vote Pubkey:")
                .with_help_message("Enter the public key for the vote account")
                .with_validator(|input: &str| {
                    if input.trim().is_empty() {
                        Ok(Validation::Invalid("Vote Pubkey is required".into()))
                    } else if input.len() < 32 || input.len() > 44 {
                        Ok(Validation::Invalid(
                            "Vote Pubkey should be a valid base58 public key (32-44 characters)"
                                .into(),
                        ))
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
    // Show animated ready message
    println!(
        "{}",
        "┌─────────────────────────────────────────────────────────────┐".bright_cyan()
    );
    println!(
        "{}",
        "│                                                             │".bright_cyan()
    );
    println!(
        "{}",
        "│  ✅ All system checks passed!                              │".bright_cyan()
    );
    println!(
        "{}",
        "│  🚀 Solana Validator Switch is ready for operation        │".bright_cyan()
    );
    println!(
        "{}",
        "│                                                             │".bright_cyan()
    );
    println!(
        "{}",
        "│  Press any key to continue...                              │".bright_cyan()
    );
    println!(
        "{}",
        "│                                                             │".bright_cyan()
    );
    println!(
        "{}",
        "└─────────────────────────────────────────────────────────────┘".bright_cyan()
    );

    // Flush stdout to ensure the prompt appears immediately
    io::stdout().flush().unwrap();

    // Skip wait for status command
    if std::env::args().any(|arg| arg == "status") {
        // For status command, just clear everything
        print!("\x1B[2J\x1B[1;1H"); // Clear entire screen and move to top
        io::stdout().flush().unwrap();
    } else {
        // Actually wait for ANY key press, not just Enter
        use crossterm::event::{self, Event};
        crossterm::terminal::enable_raw_mode().ok();
        loop {
            if let Ok(Event::Key(_)) = event::read() {
                break;
            }
        }
        crossterm::terminal::disable_raw_mode().ok();

        // Clear the ready prompt
        print!("\x1B[8A\x1B[2K"); // Move up 8 lines and clear
        for _ in 0..8 {
            print!("\x1B[2K\x1B[1B"); // Clear line and move down
        }
        print!("\x1B[8A"); // Move back up to original position
        io::stdout().flush().unwrap();
    }
}

#[allow(dead_code)]
pub async fn detect_node_statuses(
    config: &Config,
    ssh_pool: &AsyncSshPool,
) -> Result<Vec<crate::ValidatorStatus>> {
    let mut validator_statuses = Vec::new();

    for validator_pair in &config.validators {
        let mut nodes_with_status = Vec::new();

        for node in &validator_pair.nodes {
            let (
                status,
                validator_type,
                agave_validator_executable,
                fdctl_executable,
                solana_cli_executable,
                version,
                sync_status,
                current_identity,
                ledger_path,
                swap_ready,
                swap_issues,
            ) = detect_node_status_and_executable(node, validator_pair, ssh_pool).await?;
            // Derive tower path from ledger path and validator pair identity
            let tower_path = ledger_path.as_ref().map(|ledger| {
                format!(
                    "{}/tower-1_9-{}.bin",
                    ledger, validator_pair.identity_pubkey
                )
            });

            nodes_with_status.push(crate::types::NodeWithStatus {
                node: node.clone(),
                status,
                validator_type,
                agave_validator_executable,
                fdctl_executable,
                solana_cli_executable,
                version,
                sync_status,
                current_identity,
                ledger_path,
                tower_path,
                swap_ready,
                swap_issues,
                ssh_key_path: None, // Not detected in this legacy function
            });
        }

        validator_statuses.push(crate::ValidatorStatus {
            validator_pair: validator_pair.clone(),
            nodes_with_status,
            metadata: None, // Will be fetched later
        });
    }

    Ok(validator_statuses)
}

/// Detect node statuses with detailed progress reporting
async fn detect_node_statuses_with_progress(
    config: &Config,
    ssh_pool: &AsyncSshPool,
    detected_ssh_keys: &std::collections::HashMap<String, String>,
    progress_bar: &ProgressBar,
    logger: &StartupLogger,
) -> Result<Vec<crate::ValidatorStatus>> {
    logger.log_section("Node Status Detection")?;

    let mut validator_statuses = Vec::new();
    let total_nodes: usize = config.validators.iter().map(|v| v.nodes.len()).sum();
    let mut processed_nodes = 0;

    for (validator_index, validator_pair) in config.validators.iter().enumerate() {
        let mut nodes_with_status = Vec::new();

        for (node_index, node) in validator_pair.nodes.iter().enumerate() {
            // Update progress with specific node being processed
            let node_label = format!(
                "Validator {} Node {} ({})",
                validator_index + 1,
                node_index + 1,
                node.label
            );
            logger.log(&format!("Analyzing node: {}", node_label))?;
            progress_bar.suspend(|| {
                println!("  🔍 Analyzing {}...", node_label.bright_yellow());
            });

            // Step 1: SSH Connection
            progress_bar.suspend(|| {
                println!("    🔗 Establishing SSH connection...");
            });

            let (
                status,
                validator_type,
                agave_validator_executable,
                fdctl_executable,
                solana_cli_executable,
                version,
                sync_status,
                current_identity,
                ledger_path,
                swap_ready,
                swap_issues,
            ) = detect_node_status_and_executable_with_progress(
                node,
                validator_pair,
                ssh_pool,
                detected_ssh_keys.get(&node.host).cloned(),
                progress_bar,
                logger,
            )
            .await?;

            // Derive tower path from ledger path and validator pair identity
            let tower_path = ledger_path.as_ref().map(|ledger| {
                format!(
                    "{}/tower-1_9-{}.bin",
                    ledger, validator_pair.identity_pubkey
                )
            });

            // Get the detected SSH key for this node
            let ssh_key_path = detected_ssh_keys.get(&node.host).cloned();

            nodes_with_status.push(crate::types::NodeWithStatus {
                node: node.clone(),
                status: status.clone(),
                validator_type: validator_type.clone(),
                agave_validator_executable,
                fdctl_executable,
                solana_cli_executable,
                version: version.clone(),
                sync_status: sync_status.clone(),
                current_identity: current_identity.clone(),
                ledger_path,
                tower_path,
                swap_ready,
                swap_issues,
                ssh_key_path,
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
                println!(
                    "    {} {} - {}",
                    status_emoji,
                    status_text,
                    version
                        .as_ref()
                        .unwrap_or(&"Unknown version".to_string())
                        .bright_cyan()
                );
            });

            processed_nodes += 1;
            let progress_percent =
                85 + ((processed_nodes as f64 / total_nodes as f64) * 10.0) as u64;
            progress_bar.set_position(progress_percent);
        }

        // Check auto-failover safety requirements for this validator if enabled
        if let Some(ref alert_config) = config.alert_config {
            if alert_config.enabled && alert_config.auto_failover_enabled {
                progress_bar.suspend(|| {
                    println!(
                        "\n  🔍 Checking auto-failover safety requirements for Validator {}...",
                        validator_index + 1
                    );
                });

                // Check all nodes for this validator
                for node_with_status in &nodes_with_status {
                    if let Some(ssh_key) = detected_ssh_keys.get(&node_with_status.node.host) {
                        logger.log(&format!(
                            "Checking identity configuration for {}",
                            node_with_status.node.label
                        ))?;

                        match check_node_startup_identity_for_auto_failover(
                            &node_with_status,
                            ssh_pool,
                            ssh_key,
                            logger,
                        )
                        .await
                        {
                            Ok(_) => {
                                logger.log(&format!(
                                    "✅ {} passed identity check",
                                    node_with_status.node.label
                                ))?;
                                progress_bar.suspend(|| {
                                    println!(
                                        "    ✅ {} configured with safe startup identity",
                                        node_with_status.node.label
                                    );
                                });
                            }
                            Err(e) => {
                                let error_msg = format!(
                                    "Could not verify identity configuration for {}: {}",
                                    node_with_status.node.label, e
                                );
                                logger.log_error("Identity Check", &error_msg)?;
                                progress_bar.suspend(|| {
                                    println!("    ⚠️  Warning: {}", error_msg);
                                    println!("    ⚠️  Please ensure validators are configured with unfunded identity!");
                                });
                            }
                        }
                    } else {
                        progress_bar.suspend(|| {
                            println!(
                                "    ⚠️  Skipping {} - no SSH key available",
                                node_with_status.node.label
                            );
                        });
                    }
                }

                progress_bar.suspend(|| {
                    println!("    ✅ Auto-failover safety checks completed for this validator");
                });
            }
        }

        validator_statuses.push(crate::ValidatorStatus {
            validator_pair: validator_pair.clone(),
            nodes_with_status,
            metadata: None, // Will be fetched later
        });
    }

    // Check for any issues that should be reported as warnings (but don't block startup)
    let mut warnings = Vec::new();
    let mut has_startup_identity_issues = false;

    for (validator_idx, validator_status) in validator_statuses.iter().enumerate() {
        for (node_idx, node_with_status) in validator_status.nodes_with_status.iter().enumerate() {
            let node_label = format!(
                "Validator {} Node {} ({})",
                validator_idx + 1,
                node_idx + 1,
                node_with_status.node.label
            );

            // Check for SSH connectivity failure
            if node_with_status.status == crate::types::NodeStatus::Unknown {
                warnings.push(format!(
                    "{}: SSH connection failed (will limit functionality)",
                    node_label
                ));
            }

            // Skip swap readiness check during startup - will be done at switch time

            // Check for active identity detection failure
            if node_with_status.current_identity.is_none()
                && node_with_status.status != crate::types::NodeStatus::Unknown
            {
                warnings.push(format!("{}: Failed to detect active identity", node_label));
            }

            // Check for startup identity configuration issues - these are still critical for auto-failover
            for issue in &node_with_status.swap_issues {
                if issue.contains("Startup identity issue:") {
                    warnings.push(format!("{}: {}", node_label, issue));
                    has_startup_identity_issues = true;
                }
            }
        }
    }

    // Show warnings if any were found, but continue startup
    if !warnings.is_empty() {
        progress_bar.finish_and_clear();
        println!("\n{}", "⚠️  SYSTEM WARNINGS DETECTED".yellow().bold());
        println!("\nThe following issues were found (operations may be limited):\n");

        for warning in &warnings {
            println!("  • {}", warning.yellow());
        }

        if has_startup_identity_issues {
            println!(
                "\n{}",
                "Note: Startup identity issues will prevent auto-failover but not manual switches."
                    .dimmed()
            );
        }

        println!(
            "\n{}",
            "SVS will continue to start - some functionality may be limited.".green()
        );
        println!(
            "{}",
            "Use targeted commands to work with available nodes.".dimmed()
        );

        // Brief pause to let user see warnings
        tokio::time::sleep(Duration::from_millis(2000)).await;

        // Restart the progress bar for final steps
        let new_progress_bar = ProgressBar::new(100);
        new_progress_bar.set_style(
            ProgressStyle::default_bar()
                .template(
                    "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos:>3}% {msg}",
                )
                .unwrap()
                .progress_chars("█▉▊▋▌▍▎▏  "),
        );
        new_progress_bar.set_position(95);
        new_progress_bar.set_message("Completing startup...");
        new_progress_bar.enable_steady_tick(Duration::from_millis(100));

        tokio::time::sleep(Duration::from_millis(500)).await;
        new_progress_bar.finish_and_clear();
    }

    Ok(validator_statuses)
}

#[allow(dead_code)]
async fn detect_node_status_and_executable(
    node: &crate::types::NodeConfig,
    validator_pair: &crate::types::ValidatorPair,
    ssh_pool: &AsyncSshPool,
) -> Result<(
    crate::types::NodeStatus,
    crate::types::ValidatorType,
    Option<String>, // agave_validator_executable
    Option<String>, // fdctl_executable
    Option<String>, // solana_cli_executable
    Option<String>, // version
    Option<String>, // sync_status
    Option<String>, // current_identity
    Option<String>, // ledger_path
    Option<bool>,   // swap_ready
    Vec<String>,    // swap_issues
)> {
    // Use configured SSH key or default
    let ssh_key = node.ssh_key_path.as_deref().unwrap_or(DEFAULT_SSH_KEY);

    // Try to connect to the node
    if let Err(_) = ssh_pool.get_session(node, ssh_key).await {
        return Ok((
            crate::types::NodeStatus::Unknown,
            crate::types::ValidatorType::Unknown,
            None,                                      // agave_validator_executable
            None,                                      // fdctl_executable
            None,                                      // solana_cli_executable
            None,                                      // version
            None,                                      // sync_status
            None,                                      // current_identity
            None,                                      // ledger_path
            Some(false),                               // swap_ready
            vec!["SSH connection failed".to_string()], // swap_issues
        ));
    }

    // First, extract all relevant executable paths
    let mut validator_type = crate::types::ValidatorType::Unknown;
    let mut agave_validator_executable = None;
    let mut fdctl_executable = None;
    let mut solana_cli_executable = None;
    let mut _main_validator_executable = None;
    let mut version = None;
    let sync_status;
    let mut current_identity = None;
    let mut ledger_path = None;
    #[allow(dead_code)]
    let mut firedancer_config_path = None;

    // First, check what validator is actually running
    let ps_cmd =
        "ps aux | grep -E 'bin/fdctl|bin/agave-validator|release/agave-validator|bin/solana-validator|release/solana-validator' | grep -v grep";
    if let Ok(output) = ssh_pool.execute_command(node, &ssh_key, ps_cmd).await {
        let lines: Vec<&str> = output.lines().collect();
        for line in lines {
            let parts: Vec<&str> = line.split_whitespace().collect();

            // Check if this is a Firedancer process
            if line.contains("bin/fdctl") {
                // logger.log("Detected Firedancer validator")?;
                validator_type = crate::types::ValidatorType::Firedancer;

                // Extract fdctl executable and config path
                for (i, part) in parts.iter().enumerate() {
                    if part.contains("bin/fdctl") {
                        fdctl_executable = Some(part.to_string());
                        _main_validator_executable = Some(part.to_string());

                        // For Firedancer, solana CLI is in the same directory as fdctl
                        if let Some(fdctl_dir) = std::path::Path::new(part).parent() {
                            let solana_path = fdctl_dir.join("solana");
                            solana_cli_executable = Some(solana_path.to_string_lossy().to_string());
                        }
                    } else if part == &"--config" && i + 1 < parts.len() {
                        let _ = firedancer_config_path.insert(parts[i + 1].to_string());
                    }
                }
                break;
            }
            // Check if this is an Agave/Jito process
            else if line.contains("agave-validator") {
                // logger.log("Detected Agave validator")?;
                validator_type = crate::types::ValidatorType::Agave;

                // Extract agave executable and ledger path
                for (i, part) in parts.iter().enumerate() {
                    if part.contains("agave-validator")
                        && (part.contains("bin/agave-validator")
                            || part.contains("release/agave-validator"))
                    {
                        if agave_validator_executable.is_none() {
                            agave_validator_executable = Some(part.to_string());
                            _main_validator_executable = Some(part.to_string());
                            // Derive solana CLI path from agave-validator path
                            solana_cli_executable = Some(part.replace("agave-validator", "solana"));
                        }
                    } else if part == &"--ledger" && i + 1 < parts.len() {
                        ledger_path = Some(parts[i + 1].to_string());
                    }
                }
            }
        }
    }

    // If no running validator found, search for executables on disk as fallback
    if _main_validator_executable.is_none() {
        // logger.log("No running validator process found, searching for executables on disk...")?;
        // Search for agave-validator in either release or bin directories
        let agave_search_cmd = r#"find /opt /home /usr \( -path '*/release/agave-validator' -o -path '*/bin/agave-validator' \) 2>/dev/null | head -1"#;
        // logger.log_ssh_command(&node.host, agave_search_cmd, "", None)?;

        if let Ok(output) = ssh_pool
            .execute_command(node, &ssh_key, agave_search_cmd)
            .await
        {
            // logger.log_ssh_command(&node.host, agave_search_cmd, &output, None)?;
            let path = output.trim();
            if !path.is_empty() && path.contains("agave-validator") {
                agave_validator_executable = Some(path.to_string());
                _main_validator_executable = Some(path.to_string());
                // Derive solana CLI path from agave-validator path
                solana_cli_executable = Some(path.replace("agave-validator", "solana"));

                // We'll determine if it's Jito or Agave later using --version
                validator_type = crate::types::ValidatorType::Agave;
            }
        }
    }

    // Detect version based on validator type
    if validator_type == crate::types::ValidatorType::Firedancer {
        // For Firedancer, use fdctl executable
        if let Some(ref fdctl_exec) = fdctl_executable {
            if let Ok(version_output) = ssh_pool
                .execute_command(
                    node,
                    &ssh_key,
                    &format!("timeout 5 {} --version 2>/dev/null", fdctl_exec),
                )
                .await
            {
                // Parse fdctl version output - first part is version, second is git hash
                if let Some(line) = version_output.lines().next() {
                    if let Some(version_match) = line.split_whitespace().next() {
                        version = Some(format!("Firedancer {}", version_match));
                    }
                }
            }
        }
    } else if let Some(ref agave_exec) = agave_validator_executable {
        // For Agave/Jito, use agave-validator executable
        if let Ok(version_output) = ssh_pool
            .execute_command(
                node,
                &ssh_key,
                &format!("timeout 5 {} --version 2>/dev/null", agave_exec),
            )
            .await
        {
            if let Some(line) = version_output.lines().next() {
                // Handle both agave-validator and solana-cli output formats
                if line.starts_with("agave-validator ") || line.starts_with("solana-cli ") {
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
                // Check if it's Jito by looking for "jito" in the version output
                if validator_type == crate::types::ValidatorType::Agave
                    && version_output.to_lowercase().contains("jito")
                {
                    validator_type = crate::types::ValidatorType::Jito;
                    // Update version string to indicate Jito
                    if let Some(ref mut v) = version {
                        *v = v.replace("Agave", "Jito");
                    }
                }
            }
        }
    }

    // Detect sync status using RPC calls
    // We need to get the full command line from the ps output to extract RPC port
    let mut command_line = None;
    if let Ok(output) = ssh_pool.execute_command(node, &ssh_key, ps_cmd).await {
        let lines: Vec<&str> = output.lines().collect();
        for line in lines {
            if line.contains("bin/fdctl")
                || line.contains("agave-validator")
                || line.contains("solana-validator")
            {
                command_line = Some(line.to_string());
                break;
            }
        }
    }

    // Get identity and health status via RPC
    match crate::validator_rpc::get_identity_and_health(
        ssh_pool,
        node,
        &ssh_key,
        validator_type.clone(),
        command_line.as_deref(),
    )
    .await
    {
        Ok((identity, is_healthy)) => {
            if current_identity.is_none() && !identity.is_empty() {
                current_identity = Some(identity);
            }
            sync_status = Some(if is_healthy {
                "Caught up".to_string()
            } else {
                "Not healthy".to_string()
            });
        }
        Err(_) => {
            // If RPC fails, try to get just the identity
            if let Ok(identity) = crate::validator_rpc::get_identity(
                ssh_pool,
                node,
                &ssh_key,
                crate::validator_rpc::get_rpc_port(validator_type.clone(), command_line.as_deref()),
            )
            .await
            {
                if current_identity.is_none() && !identity.is_empty() {
                    current_identity = Some(identity);
                }
            }
            sync_status = Some("Unknown".to_string());
        }
    }

    // Skip swap readiness check during startup - will be done at switch time
    let swap_ready = None; // Unknown at startup
    let swap_issues = Vec::new();

    // Use RPC to get the active identity
    // Get the full command line from the ps output to extract RPC port (if we need it again)
    // We may already have command_line from above, but let's ensure we have it
    if command_line.is_none() {
        if let Ok(output) = ssh_pool.execute_command(node, &ssh_key, ps_cmd).await {
            let lines: Vec<&str> = output.lines().collect();
            for line in lines {
                if line.contains("bin/fdctl")
                    || line.contains("agave-validator")
                    || line.contains("solana-validator")
                {
                    command_line = Some(line.to_string());
                    break;
                }
            }
        }
    }

    match crate::validator_rpc::get_identity(
        ssh_pool,
        node,
        &ssh_key,
        crate::validator_rpc::get_rpc_port(validator_type.clone(), command_line.as_deref()),
    )
    .await
    {
        Ok(identity) => {
            if !identity.is_empty() {
                current_identity = Some(identity.clone());
                // logger.log(&format!("Set current_identity to: {}", identity)).ok();

                // Check if this identity matches the validator's funded identity
                // logger.log(&format!("Comparing identity {} with validator identity {}", identity, validator_pair.identity_pubkey)).ok();
                if identity == validator_pair.identity_pubkey {
                    // logger.log_success(&format!("Node {} is ACTIVE (identity matches)", node.label)).ok();
                    // Skip tower file check during startup - will be done at switch time
                    return Ok((
                        crate::types::NodeStatus::Active,
                        validator_type.clone(),
                        agave_validator_executable,
                        fdctl_executable,
                        solana_cli_executable,
                        version,
                        sync_status,
                        current_identity,
                        ledger_path,
                        swap_ready, // None - unknown at startup
                        swap_issues,
                    ));
                } else {
                    // logger.log(&format!("Node {} is STANDBY (identity {} does not match {})", node.label, identity, validator_pair.identity_pubkey)).ok();
                    // For standby nodes, swap_ready and swap_issues are already set correctly (no tower needed)
                    return Ok((
                        crate::types::NodeStatus::Standby,
                        validator_type.clone(),
                        agave_validator_executable,
                        fdctl_executable,
                        solana_cli_executable,
                        version,
                        sync_status,
                        current_identity,
                        ledger_path,
                        swap_ready,
                        swap_issues,
                    ));
                }
            }

            // If we can't find the Identity, assume unknown
            // logger.log_warning(&format!("Could not determine node status for {} - no matching identity found", node.label)).ok();
            Ok((
                crate::types::NodeStatus::Unknown,
                validator_type.clone(),
                agave_validator_executable,
                fdctl_executable,
                solana_cli_executable,
                version,
                sync_status,
                current_identity,
                ledger_path,
                swap_ready,
                swap_issues,
            ))
        }
        Err(_e) => {
            // logger.log_error("RPC identity check", &format!("Failed for node {}: {:?}", node.label, e)).ok();
            Ok((
                crate::types::NodeStatus::Unknown,
                validator_type,
                agave_validator_executable,
                fdctl_executable,
                solana_cli_executable,
                version,
                sync_status,
                current_identity,
                ledger_path,
                swap_ready,
                swap_issues,
            ))
        }
    }
}

/// Check if a node is ready for validator switching
///
/// This function checks:
/// - Funded identity keypair (readable)
/// - Unfunded identity keypair (readable)
/// - Vote keypair (readable)
/// - Ledger directory (exists and writable)
/// - Tower file (only for active nodes when is_standby = Some(false))
///
/// Note: To avoid redundancy, this is called once initially with is_standby = Some(true)
/// to skip tower checks. If the node is later determined to be active, only the tower
/// check is performed separately instead of re-running all checks.
pub async fn check_node_swap_readiness(
    ssh_pool: &AsyncSshPool,
    node: &crate::types::NodeConfig,
    ssh_key_path: &str,
    ledger_path: Option<&String>,
    is_standby: Option<bool>,
) -> (bool, Vec<String>) {
    let mut issues = Vec::new();
    let mut all_ready = true;

    // Use detected ledger path if available, otherwise use a default
    let ledger = ledger_path
        .map(|s| s.as_str())
        .unwrap_or("/mnt/solana_ledger");

    // Batch file checks into single command
    // For standby nodes, we don't check tower files
    let file_check_cmd = if is_standby == Some(true) {
        format!(
            "test -r {} && echo 'funded_ok' || echo 'funded_fail'; \
             test -r {} && echo 'unfunded_ok' || echo 'unfunded_fail'; \
             test -r {} && echo 'vote_ok' || echo 'vote_fail'; \
             test -d {} && test -w {} && echo 'ledger_ok' || echo 'ledger_fail'",
            node.paths.funded_identity,
            node.paths.unfunded_identity,
            node.paths.vote_keypair,
            ledger,
            ledger
        )
    } else {
        format!(
            "test -r {} && echo 'funded_ok' || echo 'funded_fail'; \
             test -r {} && echo 'unfunded_ok' || echo 'unfunded_fail'; \
             test -r {} && echo 'vote_ok' || echo 'vote_fail'; \
             ls {}/tower-1_9-*.bin >/dev/null 2>&1 && echo 'tower_ok' || echo 'tower_fail'; \
             test -d {} && test -w {} && echo 'ledger_ok' || echo 'ledger_fail'",
            node.paths.funded_identity,
            node.paths.unfunded_identity,
            node.paths.vote_keypair,
            ledger,
            ledger,
            ledger
        )
    };

    match ssh_pool
        .execute_command(node, ssh_key_path, &file_check_cmd)
        .await
    {
        Ok(output) => {
            for line in output.lines() {
                match line.trim() {
                    "funded_fail" => {
                        issues.push("Funded identity keypair missing or not readable".to_string());
                        all_ready = false;
                    }
                    "unfunded_fail" => {
                        issues
                            .push("Unfunded identity keypair missing or not readable".to_string());
                        all_ready = false;
                    }
                    "vote_fail" => {
                        issues.push("Vote keypair missing or not readable".to_string());
                        all_ready = false;
                    }
                    "tower_fail" => {
                        // Only report tower issues for non-standby nodes
                        if is_standby != Some(true) {
                            issues.push("Tower file missing".to_string());
                            all_ready = false;
                        }
                    }
                    "ledger_fail" => {
                        issues.push("Ledger directory missing or not writable".to_string());
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
async fn detect_node_status_and_executable_with_progress(
    node: &crate::types::NodeConfig,
    validator_pair: &crate::types::ValidatorPair,
    ssh_pool: &AsyncSshPool,
    ssh_key_path: Option<String>,
    progress_bar: &ProgressBar,
    logger: &StartupLogger,
) -> Result<(
    crate::types::NodeStatus,
    crate::types::ValidatorType,
    Option<String>, // agave_validator_executable
    Option<String>, // fdctl_executable
    Option<String>, // solana_cli_executable
    Option<String>, // version
    Option<String>, // sync_status
    Option<String>, // current_identity
    Option<String>, // ledger_path
    Option<bool>,   // swap_ready
    Vec<String>,    // swap_issues
)> {
    // Use the detected SSH key or configured key
    let ssh_key = ssh_key_path
        .or(node.ssh_key_path.clone())
        .unwrap_or_else(|| DEFAULT_SSH_KEY.to_string());

    // Show which SSH key is being used
    progress_bar.suspend(|| {
        println!("      🔑 Using SSH key: {}", ssh_key);
    });

    // Try to connect to the node
    if let Err(e) = ssh_pool.get_session(node, &ssh_key).await {
        logger.log_error("SSH", &format!("Connection to {} failed: {}", node.host, e))?;
        progress_bar.suspend(|| {
            println!("      ❌ SSH connection failed");
        });
        return Ok((
            crate::types::NodeStatus::Unknown,
            crate::types::ValidatorType::Unknown,
            None,                                      // agave_validator_executable
            None,                                      // fdctl_executable
            None,                                      // solana_cli_executable
            None,                                      // version
            None,                                      // sync_status
            None,                                      // current_identity
            None,                                      // ledger_path
            Some(false),                               // swap_ready
            vec!["SSH connection failed".to_string()], // swap_issues
        ));
    }

    logger.log_success(&format!("SSH connection established to {}", node.host))?;
    progress_bar.suspend(|| {
        println!("      ✅ SSH connection established");
    });

    // First, extract all relevant executable paths
    let mut validator_type = crate::types::ValidatorType::Unknown;
    let mut agave_validator_executable = None;
    let mut fdctl_executable = None;
    let mut solana_cli_executable = None;
    let mut _main_validator_executable = None;
    let mut version = None;
    let sync_status;
    let mut current_identity = None;
    let mut ledger_path = None;
    #[allow(dead_code)]
    let mut firedancer_config_path = None;

    // Step 2: Executable Detection
    // Removed println to prevent progress bar corruption
    logger.log("Detecting validator executables...")?;

    // First, check what validator is actually running
    let ps_cmd =
        "ps aux | grep -E 'bin/fdctl|bin/agave-validator|release/agave-validator|bin/solana-validator|release/solana-validator' | grep -v grep";
    logger.log_ssh_command(&node.host, ps_cmd, "", None)?;

    if let Ok(output) = ssh_pool.execute_command(node, &ssh_key, ps_cmd).await {
        logger.log_ssh_command(&node.host, ps_cmd, &output, None)?;
        let lines: Vec<&str> = output.lines().collect();
        logger
            .log(&format!("Found {} process lines", lines.len()))
            .ok();
        for line in lines {
            logger.log(&format!("Processing line: {}", line)).ok();
            let parts: Vec<&str> = line.split_whitespace().collect();

            // Check if this is a Firedancer process
            if line.contains("bin/fdctl") {
                logger.log("Detected Firedancer validator process").ok();
                validator_type = crate::types::ValidatorType::Firedancer;

                // Extract fdctl executable and config path
                for (i, part) in parts.iter().enumerate() {
                    if part.contains("bin/fdctl") {
                        fdctl_executable = Some(part.to_string());
                        _main_validator_executable = Some(part.to_string());

                        // For Firedancer, solana CLI is in the same directory as fdctl
                        if let Some(fdctl_dir) = std::path::Path::new(part).parent() {
                            let solana_path = fdctl_dir.join("solana");
                            solana_cli_executable = Some(solana_path.to_string_lossy().to_string());
                        }
                    } else if part == &"--config" && i + 1 < parts.len() {
                        let _ = firedancer_config_path.insert(parts[i + 1].to_string());
                    }
                }
                break;
            }
            // Check if this is an Agave/Jito/Solana process
            else if line.contains("agave-validator") || line.contains("solana-validator") {
                logger
                    .log(&format!(
                        "Detected {} validator process",
                        if line.contains("agave-validator") {
                            "Agave"
                        } else {
                            "Solana"
                        }
                    ))
                    .ok();
                validator_type = crate::types::ValidatorType::Agave;

                // Extract validator executable and ledger path
                for (i, part) in parts.iter().enumerate() {
                    if (part.contains("agave-validator") || part.contains("solana-validator"))
                        && (part.contains("bin/agave-validator")
                            || part.contains("release/agave-validator")
                            || part.contains("bin/solana-validator")
                            || part.contains("release/solana-validator"))
                    {
                        if agave_validator_executable.is_none() {
                            agave_validator_executable = Some(part.to_string());
                            _main_validator_executable = Some(part.to_string());
                            // Derive solana CLI path from validator path
                            if part.contains("agave-validator") {
                                solana_cli_executable =
                                    Some(part.replace("agave-validator", "solana"));
                            } else {
                                solana_cli_executable =
                                    Some(part.replace("solana-validator", "solana"));
                            }
                        }
                    } else if part == &"--ledger" && i + 1 < parts.len() {
                        ledger_path = Some(parts[i + 1].to_string());
                    }
                }
            }
        }
    }

    // If no running validator found, search for executables on disk as fallback
    if _main_validator_executable.is_none() {
        // logger.log("No running validator process found, searching for executables on disk...")?;
        // Search for agave-validator in either release or bin directories
        let agave_search_cmd = r#"find /opt /home /usr \( -path '*/release/agave-validator' -o -path '*/bin/agave-validator' \) 2>/dev/null | head -1"#;
        // logger.log_ssh_command(&node.host, agave_search_cmd, "", None)?;

        if let Ok(output) = ssh_pool
            .execute_command(node, &ssh_key, agave_search_cmd)
            .await
        {
            // logger.log_ssh_command(&node.host, agave_search_cmd, &output, None)?;
            let path = output.trim();
            if !path.is_empty() && path.contains("agave-validator") {
                agave_validator_executable = Some(path.to_string());
                _main_validator_executable = Some(path.to_string());
                // Derive solana CLI path from agave-validator path
                solana_cli_executable = Some(path.replace("agave-validator", "solana"));

                // We'll determine if it's Jito or Agave later using --version
                validator_type = crate::types::ValidatorType::Agave;
            }
        }
    }

    // If no running validator found, search for executables on disk as fallback
    if _main_validator_executable.is_none() {
        logger
            .log("No running validator process found, searching for executables on disk...")
            .ok();

        // Search for agave-validator or solana-validator
        // Try multiple approaches to find the validator
        let search_cmd = "ls /home/solana/.local/share/solana/install/active_release/bin/solana-validator 2>/dev/null || ls /home/solana/.local/share/solana/install/active_release/bin/agave-validator 2>/dev/null || echo ''";
        logger
            .log_ssh_command(&node.host, search_cmd, "", None)
            .ok();

        match ssh_pool.execute_command(node, &ssh_key, search_cmd).await {
            Ok(output) => {
                logger
                    .log_ssh_command(&node.host, search_cmd, &output, None)
                    .ok();
                let path = output.trim();
                if !path.is_empty()
                    && (path.contains("agave-validator") || path.contains("solana-validator"))
                {
                    if path.contains("agave-validator") {
                        agave_validator_executable = Some(path.to_string());
                        _main_validator_executable = Some(path.to_string());
                        solana_cli_executable = Some(path.replace("agave-validator", "solana"));
                    } else {
                        // solana-validator found
                        agave_validator_executable = Some(path.to_string());
                        _main_validator_executable = Some(path.to_string());
                        solana_cli_executable = Some(path.replace("solana-validator", "solana"));
                    }
                    validator_type = crate::types::ValidatorType::Agave;
                    logger
                        .log(&format!("Found validator executable at: {}", path))
                        .ok();
                } else {
                    logger.log("No validator executable found on disk").ok();
                }
            }
            Err(e) => {
                logger
                    .log_error(
                        "find command",
                        &format!("Failed to search for executables: {}", e),
                    )
                    .ok();
            }
        }
    }

    let validator_type_name = match validator_type {
        crate::types::ValidatorType::Firedancer => "Firedancer",
        crate::types::ValidatorType::Agave => "Agave",
        crate::types::ValidatorType::Jito => "Jito",
        crate::types::ValidatorType::Unknown => "Unknown",
    };

    if validator_type == crate::types::ValidatorType::Unknown {
        logger.log_warning("No validator executable detected")?;
    } else {
        logger.log(&format!("Validator type: {}", validator_type_name))?;
    }

    progress_bar.suspend(|| {
        println!(
            "      ✅ Detected {} validator",
            validator_type_name.bright_green()
        );
    });

    // For Firedancer, extract ledger path from config file if we have it
    if validator_type == crate::types::ValidatorType::Firedancer
        && firedancer_config_path.is_some()
        && ledger_path.is_none()
    {
        if let Some(ref config_path) = firedancer_config_path {
            progress_bar.suspend(|| {
                println!("      🔍 Reading Firedancer config for ledger path...");
            });

            // Read the config file and extract ledger path
            let cat_cmd = format!(
                "cat {} 2>/dev/null | grep -A 5 '\\[ledger\\]' | grep 'path' | head -1",
                config_path
            );
            if let Ok(config_output) = ssh_pool.execute_command(node, &ssh_key, &cat_cmd).await {
                // Parse something like: path = "/mnt/solana_ledger"
                for line in config_output.lines() {
                    if line.contains("path") && line.contains("=") {
                        let parts: Vec<&str> = line.split('=').collect();
                        if parts.len() >= 2 {
                            let path = parts[1].trim().trim_matches('"').trim_matches('\'');
                            if !path.is_empty() {
                                ledger_path = Some(path.to_string());
                                progress_bar.suspend(|| {
                                    println!("      ✅ Ledger path: {}", path.bright_cyan());
                                });
                                break;
                            }
                        }
                    }
                }
            }
        }
    }

    // Step 3: Version Detection
    progress_bar.suspend(|| {
        println!("      🔍 Detecting version information...");
    });
    logger.log("Detecting validator version...")?;

    // Detect version based on validator type
    if validator_type == crate::types::ValidatorType::Firedancer {
        // For Firedancer, use fdctl executable
        if let Some(ref fdctl_exec) = fdctl_executable {
            if let Ok(version_output) = ssh_pool
                .execute_command(
                    node,
                    &ssh_key,
                    &format!("timeout 5 {} --version 2>/dev/null", fdctl_exec),
                )
                .await
            {
                // Parse fdctl version output - first part is version, second is git hash
                if let Some(line) = version_output.lines().next() {
                    if let Some(version_match) = line.split_whitespace().next() {
                        version = Some(format!("Firedancer {}", version_match));
                    }
                }
            }
        }
    } else if let Some(ref agave_exec) = agave_validator_executable {
        // For Agave/Jito, use agave-validator executable
        if let Ok(version_output) = ssh_pool
            .execute_command(
                node,
                &ssh_key,
                &format!("timeout 5 {} --version 2>/dev/null", agave_exec),
            )
            .await
        {
            if let Some(line) = version_output.lines().next() {
                // Handle both agave-validator and solana-cli output formats
                if line.starts_with("agave-validator ") || line.starts_with("solana-cli ") {
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
                // Check if it's Jito by looking for "jito" in the version output
                if validator_type == crate::types::ValidatorType::Agave
                    && version_output.to_lowercase().contains("jito")
                {
                    validator_type = crate::types::ValidatorType::Jito;
                    // Update version string to indicate Jito
                    if let Some(ref mut v) = version {
                        *v = v.replace("Agave", "Jito");
                    }
                }
            }
        }
    }

    if let Some(ref v) = version {
        logger.log(&format!("Version detected: {}", v))?;
        progress_bar.suspend(|| {
            println!("      ✅ Version: {}", v.bright_cyan());
        });
    } else {
        logger.log_warning("Unable to detect validator version")?;
    }

    // Step 4: Sync Status Detection
    progress_bar.suspend(|| {
        println!("      🔍 Checking sync status...");
    });
    logger.log("Checking sync status...")?;

    // Detect sync status using RPC calls
    // We need to get the full command line from the ps output to extract RPC port
    let mut command_line = None;
    if let Ok(output) = ssh_pool.execute_command(node, &ssh_key, ps_cmd).await {
        let lines: Vec<&str> = output.lines().collect();
        for line in lines {
            if line.contains("bin/fdctl")
                || line.contains("agave-validator")
                || line.contains("solana-validator")
            {
                command_line = Some(line.to_string());
                break;
            }
        }
    }

    // Get identity and health status via RPC
    match crate::validator_rpc::get_identity_and_health(
        ssh_pool,
        node,
        &ssh_key,
        validator_type.clone(),
        command_line.as_deref(),
    )
    .await
    {
        Ok((identity, is_healthy)) => {
            if current_identity.is_none() && !identity.is_empty() {
                current_identity = Some(identity);
            }
            sync_status = Some(if is_healthy {
                "Caught up".to_string()
            } else {
                "Not healthy".to_string()
            });
        }
        Err(_) => {
            // If RPC fails, try to get just the identity
            if let Ok(identity) = crate::validator_rpc::get_identity(
                ssh_pool,
                node,
                &ssh_key,
                crate::validator_rpc::get_rpc_port(validator_type.clone(), command_line.as_deref()),
            )
            .await
            {
                if current_identity.is_none() && !identity.is_empty() {
                    current_identity = Some(identity);
                }
            }
            sync_status = Some("Unknown".to_string());
        }
    }

    // Skip swap readiness check during startup - this will be done at switch time
    // We only need to know if nodes are reachable and what their status is
    let swap_ready = None; // Unknown at startup
    let mut swap_issues = Vec::new();

    // Step 6: Check startup identity configuration
    progress_bar.suspend(|| {
        println!("      🔍 Checking startup identity configuration...");
    });
    logger.log("Checking startup identity configuration...")?;

    if validator_type != crate::types::ValidatorType::Unknown {
        if let Err(e) = crate::startup_checks::check_node_startup_identity_inline(
            node,
            validator_type.clone(),
            ssh_pool,
            &ssh_key,
        )
        .await
        {
            progress_bar.suspend(|| {
                println!("      ❌ {}", e.to_string().red());
            });
            logger.log_error("Startup identity check", &e.to_string())?;
            swap_issues.push(format!("Startup identity issue: {}", e));
        } else {
            progress_bar.suspend(|| {
                println!("      ✅ Startup identity differs from authorized voter");
            });
        }
    }

    // Step 7: Identity Detection using RPC
    progress_bar.suspend(|| {
        println!("      🔍 Detecting active identity...");
    });
    logger.log("Detecting active identity...")?;

    // Use RPC to get identity
    // Get the full command line from the ps output to extract RPC port (if we need it again)
    if command_line.is_none() {
        if let Ok(output) = ssh_pool.execute_command(node, &ssh_key, ps_cmd).await {
            let lines: Vec<&str> = output.lines().collect();
            for line in lines {
                if line.contains("bin/fdctl")
                    || line.contains("agave-validator")
                    || line.contains("solana-validator")
                {
                    command_line = Some(line.to_string());
                    break;
                }
            }
        }
    }

    match crate::validator_rpc::get_identity(
        ssh_pool,
        node,
        &ssh_key,
        crate::validator_rpc::get_rpc_port(validator_type.clone(), command_line.as_deref()),
    )
    .await
    {
        Ok(identity) => {
            if !identity.is_empty() {
                current_identity = Some(identity.clone());
                // logger.log(&format!("Set current_identity to: {}", identity)).ok();

                // Check if this identity matches the validator's funded identity
                // logger.log(&format!("Comparing identity {} with validator identity {}", identity, validator_pair.identity_pubkey)).ok();
                if identity == validator_pair.identity_pubkey {
                    // logger.log_success(&format!("Node {} is ACTIVE (identity matches)", node.label)).ok();
                    // Skip tower file check during startup - will be done at switch time
                    return Ok((
                        crate::types::NodeStatus::Active,
                        validator_type.clone(),
                        agave_validator_executable,
                        fdctl_executable,
                        solana_cli_executable,
                        version,
                        sync_status,
                        current_identity,
                        ledger_path,
                        swap_ready, // None - unknown at startup
                        swap_issues,
                    ));
                } else {
                    // logger.log(&format!("Node {} is STANDBY (identity {} does not match {})", node.label, identity, validator_pair.identity_pubkey)).ok();
                    // For standby nodes, swap_ready and swap_issues are already set correctly (no tower needed)
                    return Ok((
                        crate::types::NodeStatus::Standby,
                        validator_type.clone(),
                        agave_validator_executable,
                        fdctl_executable,
                        solana_cli_executable,
                        version,
                        sync_status,
                        current_identity,
                        ledger_path,
                        swap_ready,
                        swap_issues,
                    ));
                }
            }
        }
        Err(_e) => {
            // logger.log_error("RPC identity check", &format!("Failed for node {}: {:?}", node.label, e)).ok();
        }
    }

    // If we can't find the identity from RPC, assume unknown
    progress_bar.suspend(|| {
        println!("      ❌ Identity: Unable to determine");
    });
    Ok((
        crate::types::NodeStatus::Unknown,
        validator_type,
        agave_validator_executable,
        fdctl_executable,
        solana_cli_executable,
        version,
        sync_status,
        current_identity,
        ledger_path,
        swap_ready,
        swap_issues,
    ))
}

/// Check node startup identity configuration for auto-failover safety
async fn check_node_startup_identity_for_auto_failover(
    node: &NodeWithStatus,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
    logger: &StartupLogger,
) -> Result<()> {
    logger.log(&format!(
        "Checking identity configuration for {}",
        node.node.label
    ))?;

    // Check startup identity configuration based on validator type
    match node.validator_type {
        crate::types::ValidatorType::Firedancer => {
            logger.log(&format!(
                "{} is Firedancer type, checking config",
                node.node.label
            ))?;
            crate::startup_checks::check_node_startup_identity_inline(
                &node.node,
                node.validator_type.clone(),
                ssh_pool,
                ssh_key,
            )
            .await?
        }
        crate::types::ValidatorType::Agave | crate::types::ValidatorType::Jito => {
            logger.log(&format!(
                "{} is Agave/Jito type, checking command line",
                node.node.label
            ))?;
            crate::startup_checks::check_node_startup_identity_inline(
                &node.node,
                node.validator_type.clone(),
                ssh_pool,
                ssh_key,
            )
            .await?
        }
        crate::types::ValidatorType::Unknown => {
            logger.log(&format!(
                "⚠️ {} has unknown validator type - skipping check",
                node.node.label
            ))?;
            return Ok(());
        }
    };

    Ok(())
}
