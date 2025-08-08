use anyhow::{anyhow, Result};

/// Extract and save fdctl executable path from process info
pub fn extract_and_save_fdctl_path(
    node_with_status: &mut crate::types::NodeWithStatus,
    process_info: &str,
) -> Result<String> {
    // First check if we already have it
    if let Some(ref path) = node_with_status.fdctl_executable {
        return Ok(path.clone());
    }

    // Fallback: extract fdctl path from running process
    let fdctl_path = process_info
        .lines()
        .find(|line| line.contains("fdctl"))
        .and_then(|line| {
            line.split_whitespace()
                .find(|part| part.contains("fdctl") && (part.ends_with("fdctl") || part.contains("/fdctl")))
                .map(|s| s.to_string())
        })
        .ok_or_else(|| anyhow!("Firedancer fdctl executable path not found in node status or running process"))?;
    
    // Save it back to node status for future use
    node_with_status.fdctl_executable = Some(fdctl_path.clone());
    
    Ok(fdctl_path)
}

/// Extract and save agave validator executable path from process info
pub fn extract_and_save_agave_path(
    node_with_status: &mut crate::types::NodeWithStatus,
    process_info: &str,
) -> Result<String> {
    // First check if we already have it
    if let Some(ref path) = node_with_status.agave_validator_executable {
        return Ok(path.clone());
    }

    // Fallback: extract agave-validator path from running process
    let agave_path = process_info
        .lines()
        .find(|line| line.contains("agave-validator"))
        .and_then(|line| {
            line.split_whitespace()
                .find(|part| part.contains("agave-validator") && (part.ends_with("agave-validator") || part.contains("/agave-validator")))
                .map(|s| s.to_string())
        })
        .ok_or_else(|| anyhow!("Agave validator executable path not found in node status or running process"))?;
    
    // Save it back to node status for future use
    node_with_status.agave_validator_executable = Some(agave_path.clone());
    
    // Also update solana CLI path
    node_with_status.solana_cli_executable = Some(agave_path.replace("agave-validator", "solana"));
    
    Ok(agave_path)
}

/// Extract ledger path from process info
pub fn extract_ledger_path(process_info: &str) -> Option<String> {
    process_info
        .lines()
        .find(|line| line.contains("--ledger"))
        .and_then(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts
                .windows(2)
                .find(|w| w[0] == "--ledger")
                .map(|w| w[1].to_string())
        })
}

/// Extract config path for Firedancer from process info
pub fn extract_firedancer_config_path(process_info: &str) -> Result<String> {
    process_info
        .lines()
        .find(|line| line.contains("fdctl") && line.contains("--config"))
        .and_then(|line| {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts
                .windows(2)
                .find(|w| w[0] == "--config")
                .map(|w| w[1].to_string())
        })
        .ok_or_else(|| anyhow!("Firedancer config path not found in running process. Please ensure fdctl is running with --config parameter"))
}