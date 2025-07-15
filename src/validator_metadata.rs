use anyhow::{anyhow, Result};
use base64::{engine::general_purpose, Engine as _};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;

const CONFIG_PROGRAM_ID: &str = "Config1111111111111111111111111111111111111";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorMetadata {
    pub name: Option<String>,
    pub website: Option<String>,
    pub details: Option<String>,
    #[serde(rename = "iconUrl")]
    pub icon_url: Option<String>,
}

#[derive(Debug, Serialize)]
struct RpcRequest {
    jsonrpc: String,
    id: u64,
    method: String,
    params: Vec<Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RpcResponse {
    jsonrpc: String,
    result: Option<Vec<AccountInfo>>,
    error: Option<Value>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct AccountInfo {
    account: Account,
    pubkey: String,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct Account {
    data: (String, String), // (base64_data, encoding)
    executable: bool,
    lamports: u64,
    owner: String,
}

pub async fn fetch_validator_metadata(
    rpc_url: &str,
    validator_identity: &str,
) -> Result<Option<ValidatorMetadata>> {
    let client = reqwest::Client::new();

    // Prepare the RPC request
    let payload = RpcRequest {
        jsonrpc: "2.0".to_string(),
        id: 1,
        method: "getProgramAccounts".to_string(),
        params: vec![
            serde_json::json!(CONFIG_PROGRAM_ID),
            serde_json::json!({
                "filters": [
                    {
                        "memcmp": {
                            "offset": 37,
                            "bytes": validator_identity
                        }
                    }
                ],
                "encoding": "base64"
            }),
        ],
    };

    // Make the RPC request
    let response = client
        .post(rpc_url)
        .json(&payload)
        .send()
        .await
        .map_err(|e| anyhow!("Failed to send RPC request: {}", e))?;

    let rpc_response: RpcResponse = response
        .json()
        .await
        .map_err(|e| anyhow!("Failed to parse RPC response: {}", e))?;

    if let Some(error) = rpc_response.error {
        return Err(anyhow!("RPC error: {:?}", error));
    }

    let result = rpc_response
        .result
        .ok_or_else(|| anyhow!("No result in RPC response"))?;

    if result.is_empty() {
        return Ok(None); // No metadata found
    }

    // Take the first (usually only) account
    let account_data_b64 = &result[0].account.data.0;
    let account_data = general_purpose::STANDARD
        .decode(account_data_b64)
        .map_err(|e| anyhow!("Failed to decode base64: {}", e))?;

    // Parse the ConfigKeys prefix dynamically
    if account_data.len() < 4 {
        return Err(anyhow!("Account data too short"));
    }

    // Read number of keys (u32 little-endian)
    let num_keys = u32::from_le_bytes([
        account_data[0],
        account_data[1],
        account_data[2],
        account_data[3],
    ]);

    let mut prefix_size = 4; // u32 size

    // Skip over each key (32 bytes pubkey + 1 byte signer flag)
    for _ in 0..num_keys {
        prefix_size += 33; // 32 (pubkey) + 1 (bool)
    }

    if account_data.len() <= prefix_size {
        return Err(anyhow!("Account data too short for metadata"));
    }

    let metadata_bytes = &account_data[prefix_size..];

    // Parse JSON metadata
    let metadata: ValidatorMetadata = serde_json::from_slice(metadata_bytes)
        .map_err(|e| anyhow!("Failed to parse metadata JSON: {}", e))?;

    Ok(Some(metadata))
}

// Cache for validator metadata to avoid repeated RPC calls
pub struct MetadataCache {
    cache: HashMap<String, Option<ValidatorMetadata>>,
}

impl MetadataCache {
    pub fn new() -> Self {
        Self {
            cache: HashMap::new(),
        }
    }

    pub async fn get_or_fetch(
        &mut self,
        rpc_url: &str,
        validator_identity: &str,
    ) -> Result<Option<ValidatorMetadata>> {
        if let Some(cached) = self.cache.get(validator_identity) {
            return Ok(cached.clone());
        }

        let metadata = fetch_validator_metadata(rpc_url, validator_identity).await?;
        self.cache
            .insert(validator_identity.to_string(), metadata.clone());

        Ok(metadata)
    }
}
