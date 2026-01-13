use anyhow::{anyhow, Result};

/// Get fdctl executable path from config (required, no auto-detection)
pub fn get_fdctl_path(node_with_status: &crate::types::NodeWithStatus) -> Result<String> {
    node_with_status.fdctl_executable.clone()
        .ok_or_else(|| anyhow!("Firedancer fdctl executable path not configured. Please set 'fdctlPath' in node paths config"))
}

/// Get agave validator executable path from config (required, no auto-detection)
#[allow(dead_code)]
pub fn get_agave_path(node_with_status: &crate::types::NodeWithStatus) -> Result<String> {
    node_with_status.agave_validator_executable.clone()
        .ok_or_else(|| anyhow!("Agave validator executable path not configured. Please set 'agaveValidatorPath' in node paths config"))
}

/// Get solana CLI executable path from config (required, no auto-detection)
#[allow(dead_code)]
pub fn get_solana_cli_path(node_with_status: &crate::types::NodeWithStatus) -> Result<String> {
    node_with_status.solana_cli_executable.clone()
        .ok_or_else(|| anyhow!("Solana CLI executable path not configured. Please set 'solanaCliPath' in node paths config"))
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
