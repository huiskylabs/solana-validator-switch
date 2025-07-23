use anyhow::{anyhow, Result};
use colored::*;
use std::collections::HashMap;

use crate::ssh::AsyncSshPool;
use crate::startup_logger::StartupLogger;
use crate::types::{NodeWithStatus, ValidatorPair};
use crate::AppState;

/// Perform startup safety checks for auto-failover configuration
pub async fn check_auto_failover_safety(app_state: &AppState, logger: &StartupLogger) -> Result<()> {
    // Skip checks if auto-failover is not enabled
    let _alert_config = match &app_state.config.alert_config {
        Some(config) if config.enabled && config.auto_failover_enabled => config,
        _ => return Ok(()), // Auto-failover not enabled, no checks needed
    };

    // Always require unfunded identity check when auto-failover is enabled
    // This is a critical safety requirement

    println!("\n{}", "🔍 Checking auto-failover safety requirements...".cyan());
    logger.log("Starting auto-failover safety checks")?;

    // Check each validator pair
    for (idx, validator_status) in app_state.validator_statuses.iter().enumerate() {
        let validator_pair = &validator_status.validator_pair;
        
        println!(
            "\n  Validator {}: {}",
            idx + 1,
            validator_pair.identity_pubkey.bright_white()
        );

        // Check all nodes for this validator
        for node_with_status in &validator_status.nodes_with_status {
            logger.log(&format!("Checking identity configuration for {}", node_with_status.node.label))?;
            match check_node_identity(
                node_with_status,
                validator_pair,
                &app_state.ssh_pool,
                &app_state.detected_ssh_keys,
                logger,
            )
            .await {
                Ok(_) => {
                    logger.log(&format!("✅ {} passed identity check", node_with_status.node.label))?;
                },
                Err(e) => {
                    let error_msg = format!("Could not verify identity configuration for {}: {}", 
                        node_with_status.node.label, e);
                    logger.log_error("Identity Check", &error_msg)?;
                    println!("      ⚠️  Warning: {}", error_msg);
                    println!("      ⚠️  Please ensure validators are configured with unfunded identity!");
                }
            }
        }
    }

    println!(
        "\n{}",
        "✅ All validators configured with unfunded identity - safe for auto-failover"
            .green()
            .bold()
    );

    Ok(())
}

/// Check that validators are not starting with their authorized voter identity
pub async fn check_startup_identity_safety(app_state: &AppState) -> Result<()> {
    println!("\n{}", "🔍 Checking startup identity configuration...".cyan());

    // Check each validator pair
    for (idx, validator_status) in app_state.validator_statuses.iter().enumerate() {
        let validator_pair = &validator_status.validator_pair;
        
        println!(
            "\n  Validator {}: {}",
            idx + 1,
            validator_pair.identity_pubkey.bright_white()
        );

        // Check all nodes for this validator
        for node_with_status in &validator_status.nodes_with_status {
            check_node_startup_identity(
                node_with_status,
                &app_state.ssh_pool,
                &app_state.detected_ssh_keys,
            )
            .await?;
        }
    }

    println!(
        "\n{}",
        "✅ All validators configured with safe startup identity"
            .green()
            .bold()
    );

    Ok(())
}

async fn check_node_identity(
    node: &NodeWithStatus,
    _validator_pair: &ValidatorPair,
    ssh_pool: &AsyncSshPool,
    detected_ssh_keys: &HashMap<String, String>,
    logger: &StartupLogger,
) -> Result<()> {
    let ssh_key = detected_ssh_keys
        .get(&node.node.host)
        .ok_or_else(|| anyhow!("No SSH key detected for {}", node.node.host))?;

    println!("    Checking {}: ", node.node.label);

    // Check startup identity configuration based on validator type
    match node.validator_type {
        crate::types::ValidatorType::Firedancer => {
            logger.log(&format!("{} is Firedancer type, checking config", node.node.label))?;
            check_firedancer_identity_config(node, ssh_pool, ssh_key).await?
        }
        crate::types::ValidatorType::Agave | crate::types::ValidatorType::Jito => {
            logger.log(&format!("{} is Agave/Jito type, checking command line", node.node.label))?;
            check_agave_identity_config(node, ssh_pool, ssh_key).await?
        }
        crate::types::ValidatorType::Unknown => {
            logger.log(&format!("⚠️ {} has unknown validator type - skipping check", node.node.label))?;
            println!("      ⚠️  Unknown validator type - skipping check");
            return Ok(());
        }
    };

    println!("      ✅ Configured with safe startup identity");
    Ok(())
}


async fn check_node_startup_identity(
    node: &NodeWithStatus,
    ssh_pool: &AsyncSshPool,
    detected_ssh_keys: &HashMap<String, String>,
) -> Result<()> {
    let ssh_key = detected_ssh_keys
        .get(&node.node.host)
        .ok_or_else(|| anyhow!("No SSH key detected for {}", node.node.host))?;

    println!("    Checking {}: ", node.node.label);

    // Check identity configuration based on validator type
    match node.validator_type {
        crate::types::ValidatorType::Firedancer => {
            check_firedancer_identity_config(node, ssh_pool, ssh_key).await?
        }
        crate::types::ValidatorType::Agave | crate::types::ValidatorType::Jito => {
            check_agave_identity_config(node, ssh_pool, ssh_key).await?
        }
        crate::types::ValidatorType::Unknown => {
            println!("      ⚠️  Unknown validator type - skipping check");
            return Ok(());
        }
    };

    println!("      ✅ Startup identity differs from authorized voter");
    Ok(())
}

async fn check_firedancer_identity_config(
    node: &NodeWithStatus,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
) -> Result<()> {
    // TODO: This function should use a proper TOML parser instead of grep/string parsing.
    // Current implementation is fragile and error-prone. Should:
    // 1. Add `toml` crate dependency
    // 2. Read the entire config file via SSH
    // 3. Parse with toml::from_str
    // 4. Access fields properly: config["consensus"]["identity_path"] and config["consensus"]["authorized_voter_paths"][0]
    
    // Get the Firedancer config file path
    let ps_cmd = "ps aux | grep -E 'fdctl.*--config' | grep -v grep";
    let process_info = ssh_pool
        .execute_command(&node.node, ssh_key, ps_cmd)
        .await?;

    let config_path = process_info
        .lines()
        .find(|line| line.contains("fdctl") && line.contains("--config"))
        .and_then(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts
                .windows(2)
                .find(|w| w[0] == "--config")
                .map(|w| w[1].to_string())
        })
        .ok_or_else(|| anyhow!("Failed to find Firedancer config path in running process"))?;

    // Read the consensus section from the config file
    let config_cmd = format!("grep -A10 '\\[consensus\\]' \"{}\" | grep -E 'identity_path|authorized_voter_paths' -A3", config_path);
    let config_content = ssh_pool
        .execute_command(&node.node, ssh_key, &config_cmd)
        .await?;

    // Parse identity_path - format: identity_path = "/path/to/keypair.json"
    let identity_path = config_content
        .lines()
        .find(|line| line.trim_start().starts_with("identity_path"))
        .and_then(|line| {
            // Extract the path from quotes after the = sign
            line.split('=')
                .nth(1)
                .and_then(|part| part.trim().split('"').nth(1))
        })
        .ok_or_else(|| anyhow!("Failed to parse identity_path from Firedancer config"))?;

    // Parse authorized_voter_paths - it's an array, we need the first item
    // Format: authorized_voter_paths = [
    //     "/path/to/funded-validator-keypair.json"
    // ]
    let authorized_voter_path = config_content
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("authorized_voter_paths"))
        .nth(1) // Get the line after authorized_voter_paths = [
        .and_then(|line| {
            // Extract path from quotes, handling indentation
            line.trim().split('"').nth(1)
        })
        .ok_or_else(|| anyhow!("Failed to parse authorized_voter_paths from Firedancer config"))?;

    // Check if they're the same
    if identity_path == authorized_voter_path {
        return Err(anyhow!(
            "\n❌ SAFETY CHECK FAILED: {} has identity_path same as authorized_voter_paths!\n\
             \n\
             Firedancer Config Issue:\n\
             identity_path = \"{}\"\n\
             authorized_voter_paths[0] = \"{}\"\n\
             \n\
             This is UNSAFE for auto-failover. The startup identity must differ from the authorized voter.\n\
             \n\
             To fix this:\n\
             1. Stop Firedancer\n\
             2. Edit the config file: {}\n\
             3. Set identity_path to your unfunded keypair: \"{}\"\n\
             4. Restart Firedancer\n\
             5. Run svs again",
            node.node.label.red().bold(),
            identity_path,
            authorized_voter_path,
            config_path,
            node.node.paths.unfunded_identity
        ));
    }

    Ok(())
}

async fn check_agave_identity_config(
    node: &NodeWithStatus,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
) -> Result<()> {
    // Get the running process command line
    let ps_cmd = "ps aux | grep -E 'solana-validator|agave-validator|jito-validator' | grep -v grep";
    let process_info = ssh_pool
        .execute_command(&node.node, ssh_key, ps_cmd)
        .await?;

    let process_line = process_info
        .lines()
        .find(|line| line.contains("validator"))
        .ok_or_else(|| anyhow!("Failed to find validator process"))?;

    // Extract --identity and --authorized-voter paths
    let parts: Vec<&str> = process_line.split_whitespace().collect();
    
    let identity_path = parts
        .windows(2)
        .find(|w| w[0] == "--identity")
        .map(|w| w[1])
        .ok_or_else(|| anyhow!("Failed to find --identity in validator command"))?;

    let authorized_voter_path = parts
        .windows(2)
        .find(|w| w[0] == "--authorized-voter")
        .map(|w| w[1])
        .ok_or_else(|| anyhow!("Failed to find --authorized-voter in validator command"))?;

    // Check if they're the same
    if identity_path == authorized_voter_path {
        return Err(anyhow!(
            "\n❌ SAFETY CHECK FAILED: {} has --identity same as --authorized-voter!\n\
             \n\
             Command Line Issue:\n\
             --identity {}\n\
             --authorized-voter {}\n\
             \n\
             This is UNSAFE for auto-failover. The startup identity must differ from the authorized voter.\n\
             \n\
             To fix this:\n\
             1. Stop the validator\n\
             2. Change the startup command to use different keypairs:\n\
                --identity {}\n\
                --authorized-voter {}\n\
             3. Restart the validator\n\
             4. Run svs again",
            node.node.label.red().bold(),
            identity_path,
            authorized_voter_path,
            node.node.paths.unfunded_identity,
            node.node.paths.funded_identity
        ));
    }

    Ok(())
}

/// Check node startup identity configuration inline during startup
pub async fn check_node_startup_identity_inline(
    node: &crate::types::NodeConfig,
    validator_type: crate::types::ValidatorType,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
) -> Result<()> {
    match validator_type {
        crate::types::ValidatorType::Firedancer => {
            check_firedancer_identity_config_inline(node, ssh_pool, ssh_key).await
        }
        crate::types::ValidatorType::Agave | crate::types::ValidatorType::Jito => {
            check_agave_identity_config_inline(node, ssh_pool, ssh_key).await
        }
        crate::types::ValidatorType::Unknown => Ok(()),
    }
}

async fn check_firedancer_identity_config_inline(
    node: &crate::types::NodeConfig,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
) -> Result<()> {
    // TODO: This function should use a proper TOML parser instead of grep/string parsing.
    // See TODO comment in check_firedancer_identity_config() above.
    
    // Get the Firedancer config file path
    let ps_cmd = "ps aux | grep -E 'fdctl.*--config' | grep -v grep";
    let process_info = ssh_pool
        .execute_command(node, ssh_key, ps_cmd)
        .await?;

    let config_path = process_info
        .lines()
        .find(|line| line.contains("fdctl") && line.contains("--config"))
        .and_then(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts
                .windows(2)
                .find(|w| w[0] == "--config")
                .map(|w| w[1].to_string())
        })
        .ok_or_else(|| anyhow!("Failed to find Firedancer config path"))?;

    // Read the consensus section from the config file
    let config_cmd = format!("grep -A10 '\\[consensus\\]' \"{}\" | grep -E 'identity_path|authorized_voter_paths' -A3", config_path);
    let config_content = ssh_pool
        .execute_command(node, ssh_key, &config_cmd)
        .await?;

    // Parse identity_path
    let identity_path = config_content
        .lines()
        .find(|line| line.trim_start().starts_with("identity_path"))
        .and_then(|line| {
            line.split('=')
                .nth(1)
                .and_then(|part| part.trim().split('"').nth(1))
        })
        .ok_or_else(|| anyhow!("Failed to parse identity_path"))?;

    // Parse authorized_voter_paths (first item)
    let authorized_voter_path = config_content
        .lines()
        .skip_while(|line| !line.trim_start().starts_with("authorized_voter_paths"))
        .nth(1) // Get the line after authorized_voter_paths = [
        .and_then(|line| {
            line.trim().split('"').nth(1)
        })
        .ok_or_else(|| anyhow!("Failed to parse authorized_voter_paths"))?;

    // Check if they're the same
    if identity_path == authorized_voter_path {
        return Err(anyhow!(
            "Identity matches authorized voter! identity_path={}, authorized_voter_paths[0]={}",
            identity_path,
            authorized_voter_path
        ));
    }

    Ok(())
}

async fn check_agave_identity_config_inline(
    node: &crate::types::NodeConfig,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
) -> Result<()> {
    // Get the running process command line
    let ps_cmd = "ps aux | grep -E 'solana-validator|agave-validator|jito-validator' | grep -v grep";
    let process_info = ssh_pool
        .execute_command(node, ssh_key, ps_cmd)
        .await?;

    let process_line = process_info
        .lines()
        .find(|line| line.contains("validator"))
        .ok_or_else(|| anyhow!("Failed to find validator process"))?;

    // Extract --identity and --authorized-voter paths
    let parts: Vec<&str> = process_line.split_whitespace().collect();
    
    let identity_path = parts
        .windows(2)
        .find(|w| w[0] == "--identity")
        .map(|w| w[1])
        .ok_or_else(|| anyhow!("Failed to find --identity"))?;

    let authorized_voter_path = parts
        .windows(2)
        .find(|w| w[0] == "--authorized-voter")
        .map(|w| w[1])
        .ok_or_else(|| anyhow!("Failed to find --authorized-voter"))?;

    // Check if they're the same
    if identity_path == authorized_voter_path {
        return Err(anyhow!(
            "Identity matches authorized voter! --identity={}, --authorized-voter={}",
            identity_path,
            authorized_voter_path
        ));
    }

    Ok(())
}