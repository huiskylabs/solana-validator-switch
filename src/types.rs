use serde::{Deserialize, Serialize};

// Default functions for serde
fn default_enabled() -> bool {
    true
}

fn default_delinquency_threshold() -> u64 {
    30
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: String,
    pub validators: Vec<ValidatorPair>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub alert_config: Option<AlertConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AlertConfig {
    #[serde(default = "default_enabled")]
    pub enabled: bool,
    #[serde(default = "default_delinquency_threshold")]
    pub delinquency_threshold_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram: Option<TelegramConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelegramConfig {
    pub bot_token: String,
    pub chat_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorPair {
    #[serde(rename = "votePubkey")]
    pub vote_pubkey: String,
    #[serde(rename = "identityPubkey")]
    pub identity_pubkey: String,
    pub rpc: String,
    pub nodes: Vec<NodeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub paths: NodePaths,
    #[serde(rename = "sshKeyPath", skip_serializing_if = "Option::is_none")]
    pub ssh_key_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePaths {
    #[serde(rename = "fundedIdentity")]
    pub funded_identity: String,
    #[serde(rename = "unfundedIdentity")]
    pub unfunded_identity: String,
    #[serde(rename = "voteKeypair")]
    pub vote_keypair: String,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    Active,
    Standby,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidatorType {
    Agave,
    Jito,
    Firedancer,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct NodeWithStatus {
    pub node: NodeConfig,
    pub status: NodeStatus,
    pub validator_type: ValidatorType, // Type of validator (Firedancer, Agave, Jito, etc.)
    pub agave_validator_executable: Option<String>, // Path to agave-validator executable (for catchup check)
    pub fdctl_executable: Option<String>, // Path to fdctl executable (for firedancer identity set)
    pub solana_cli_executable: Option<String>, // Path to solana CLI executable
    pub version: Option<String>,          // Version information (e.g., "Firedancer 0.505.20216")
    pub sync_status: Option<String>,      // Sync status (e.g., "Caught up (slot: 344297365)")
    pub current_identity: Option<String>, // Current validator identity (from catchup command)
    pub ledger_path: Option<String>,      // Ledger path extracted from running process or config
    pub tower_path: Option<String>,       // Tower file path derived from ledger path and identity
    pub swap_ready: Option<bool>,         // Whether the node is ready for validator switching
    pub swap_issues: Vec<String>,         // Issues preventing swap readiness
    pub ssh_key_path: Option<String>,     // Detected SSH key path for this node
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ValidationResult {
    pub valid_files: u32,
    pub total_files: u32,
    pub issues: Vec<String>,
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}
