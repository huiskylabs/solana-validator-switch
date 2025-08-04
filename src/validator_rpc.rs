use anyhow::{anyhow, Result};
use serde_json::{json, Value};

use crate::ssh::AsyncSshPool;
use crate::types::{NodeConfig, ValidatorType};

/// RPC response structure for standard JSON-RPC responses
#[derive(Debug)]
pub struct RpcResponse {
    pub result: Value,
    pub error: Option<Value>,
}

/// Get the appropriate RPC port for a validator type
pub fn get_rpc_port(validator_type: ValidatorType, command_line: Option<&str>) -> u16 {
    match validator_type {
        ValidatorType::Agave | ValidatorType::Jito => {
            // Check command line for custom RPC port
            let mut port = 8899; // default
            if let Some(cmd) = command_line {
                if let Some(pos) = cmd.find("--rpc-port") {
                    let rest = &cmd[pos..];
                    if let Some(port_str) = rest.split_whitespace().nth(1) {
                        if let Ok(p) = port_str.parse::<u16>() {
                            port = p;
                        }
                    }
                }
            }
            port
        }
        ValidatorType::Firedancer => 8899, // Firedancer always uses 8899
        ValidatorType::Unknown => 8899,    // Default to 8899
    }
}

/// Execute a JSON-RPC call over SSH
pub async fn execute_rpc_call(
    ssh_pool: &AsyncSshPool,
    node: &NodeConfig,
    ssh_key: &str,
    method: &str,
    params: Option<Value>,
    rpc_port: u16,
) -> Result<RpcResponse> {
    let request = json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": method,
        "params": params.unwrap_or(json!([]))
    });

    let curl_command = format!(
        r#"curl -s http://localhost:{} -X POST -H "Content-Type: application/json" -d '{}' 2>&1"#,
        rpc_port,
        request.to_string()
    );

    let output = ssh_pool
        .execute_command(node, ssh_key, &curl_command)
        .await
        .map_err(|e| anyhow!("Failed to execute RPC call: {}", e))?;

    // Parse JSON response
    let json_response: Value = serde_json::from_str(&output)
        .map_err(|e| anyhow!("Failed to parse RPC response: {}. Output: {}", e, output))?;

    Ok(RpcResponse {
        result: json_response.get("result").cloned().unwrap_or(json!(null)),
        error: json_response.get("error").cloned(),
    })
}

/// Get validator identity using getIdentity RPC call
pub async fn get_identity(
    ssh_pool: &AsyncSshPool,
    node: &NodeConfig,
    ssh_key: &str,
    rpc_port: u16,
) -> Result<String> {
    let response = execute_rpc_call(ssh_pool, node, ssh_key, "getIdentity", None, rpc_port).await?;

    if let Some(error) = response.error {
        return Err(anyhow!("RPC error: {:?}", error));
    }

    response
        .result
        .get("identity")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| anyhow!("Failed to extract identity from RPC response"))
}

/// Get validator health using getHealth RPC call
pub async fn get_health(
    ssh_pool: &AsyncSshPool,
    node: &NodeConfig,
    ssh_key: &str,
    rpc_port: u16,
) -> Result<bool> {
    let response = execute_rpc_call(ssh_pool, node, ssh_key, "getHealth", None, rpc_port).await?;

    if let Some(error) = response.error {
        return Err(anyhow!("RPC error: {:?}", error));
    }

    // Check if result is "ok"
    match response.result.as_str() {
        Some("ok") => Ok(true),
        _ => Ok(false),
    }
}

/// Check if a validator is caught up using getHealth RPC
pub async fn is_validator_caught_up(
    ssh_pool: &AsyncSshPool,
    node: &NodeConfig,
    ssh_key: &str,
    validator_type: ValidatorType,
    command_line: Option<&str>,
) -> Result<(bool, String)> {
    let rpc_port = get_rpc_port(validator_type, command_line);

    match get_health(ssh_pool, node, ssh_key, rpc_port).await {
        Ok(true) => Ok((true, "Caught up".to_string())),
        Ok(false) => Ok((false, "Not healthy".to_string())),
        Err(e) => Ok((false, format!("RPC error: {}", e))),
    }
}

/// Get validator identity and health status in one call
pub async fn get_identity_and_health(
    ssh_pool: &AsyncSshPool,
    node: &NodeConfig,
    ssh_key: &str,
    validator_type: ValidatorType,
    command_line: Option<&str>,
) -> Result<(String, bool)> {
    let rpc_port = get_rpc_port(validator_type, command_line);

    // Get identity
    let identity = get_identity(ssh_pool, node, ssh_key, rpc_port).await?;

    // Get health
    let is_healthy = get_health(ssh_pool, node, ssh_key, rpc_port)
        .await
        .unwrap_or(false);

    Ok((identity, is_healthy))
}
