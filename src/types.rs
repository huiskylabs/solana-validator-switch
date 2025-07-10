use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: String,
    pub validators: Vec<ValidatorPair>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidatorPair {
    #[serde(rename = "votePubkey")]
    pub vote_pubkey: String,
    #[serde(rename = "identityPubkey")]
    pub identity_pubkey: String,
    pub rpc: String,
    #[serde(rename = "localSshKeyPath")]
    pub local_ssh_key_path: String,
    pub nodes: Vec<NodeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeConfig {
    pub label: String,
    pub host: String,
    pub port: u16,
    pub user: String,
    pub paths: NodePaths,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodePaths {
    #[serde(rename = "fundedIdentity")]
    pub funded_identity: String,
    #[serde(rename = "unfundedIdentity")]
    pub unfunded_identity: String,
    #[serde(rename = "voteKeypair")]
    pub vote_keypair: String,
    pub ledger: String,
    pub tower: String,
    #[serde(rename = "solanaCliPath")]
    pub solana_cli_path: String,
    #[serde(rename = "firedancerConfig")]
    pub firedancer_config: Option<String>,
    #[serde(rename = "fdctlPath")]
    pub fdctl_path: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum NodeStatus {
    Active,
    Standby,
    Unknown,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ValidatorType {
    Solana,
    Agave,
    Jito,
    Firedancer,
    Unknown,
}

#[derive(Debug, Clone)]
pub struct NodeWithStatus {
    pub node: NodeConfig,
    pub status: NodeStatus,
    pub validator_type: ValidatorType, // Type of validator (Firedancer, Agave, Solana, etc.)
    pub agave_validator_executable: Option<String>, // Path to agave-validator executable (for catchup check)
    pub fdctl_executable: Option<String>, // Path to fdctl executable (for firedancer identity set)
    pub version: Option<String>,          // Version information (e.g., "Firedancer 0.505.20216")
    pub sync_status: Option<String>,      // Sync status (e.g., "Caught up (slot: 344297365)")
    pub current_identity: Option<String>, // Current validator identity
    pub swap_ready: Option<bool>,         // Whether the node is ready for validator switching
    pub swap_issues: Vec<String>,         // Issues preventing swap readiness
}

#[derive(Debug)]
#[allow(dead_code)]
pub struct ValidationResult {
    pub valid_files: u32,
    pub total_files: u32,
    pub issues: Vec<String>,
}

#[derive(Debug)]
pub struct ConnectionStatus {
    pub connected: bool,
    pub latency_ms: Option<u64>,
    pub error: Option<String>,
}
