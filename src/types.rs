use serde::{Deserialize, Serialize};
use std::time::Instant;

// Default functions for serde
fn default_enabled() -> bool {
    true
}

fn default_delinquency_threshold() -> u64 {
    30
}

fn default_ssh_failure_threshold() -> u64 {
    1800 // 30 minutes of SSH failures before alert
}

fn default_rpc_failure_threshold() -> u64 {
    1800 // 30 minutes of RPC failures before alert
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
    #[serde(default = "default_ssh_failure_threshold")]
    pub ssh_failure_threshold_seconds: u64,
    #[serde(default = "default_rpc_failure_threshold")]
    pub rpc_failure_threshold_seconds: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub telegram: Option<TelegramConfig>,
    #[serde(default)]
    pub auto_failover_enabled: bool,
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
    #[serde(rename = "solanaCliPath")]
    pub solana_cli: String,
    #[serde(rename = "agaveValidatorPath", skip_serializing_if = "Option::is_none")]
    pub agave_validator: Option<String>,
    #[serde(rename = "fdctlPath", skip_serializing_if = "Option::is_none")]
    pub fdctl: Option<String>,
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

#[derive(Debug, Clone, PartialEq)]
pub enum RemoteShellType {
    Bash,
    PowerShell,     // Windows PowerShell
    PowerShellCore, // pwsh on Linux/macOS
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

// Failure tracking structures
#[derive(Debug, Clone)]
pub struct FailureTracker {
    pub consecutive_failures: u32,
    pub first_failure_time: Option<Instant>,
    pub last_success_time: Option<Instant>,
    pub last_failure_time: Option<Instant>,
    pub last_error: Option<String>,
}

impl FailureTracker {
    pub fn new() -> Self {
        Self {
            consecutive_failures: 0,
            first_failure_time: None,
            last_success_time: None,
            last_failure_time: None,
            last_error: None,
        }
    }

    pub fn record_success(&mut self) {
        self.consecutive_failures = 0;
        self.first_failure_time = None;
        self.last_success_time = Some(Instant::now());
        self.last_error = None;
    }

    pub fn record_failure(&mut self, error: String) {
        self.consecutive_failures += 1;
        if self.first_failure_time.is_none() {
            self.first_failure_time = Some(Instant::now());
        }
        self.last_failure_time = Some(Instant::now());
        self.last_error = Some(error);
    }

    #[allow(dead_code)]
    pub fn seconds_since_first_failure(&self) -> Option<u64> {
        self.first_failure_time.map(|t| t.elapsed().as_secs())
    }

    #[allow(dead_code)]
    pub fn seconds_since_last_success(&self) -> Option<u64> {
        self.last_success_time.map(|t| t.elapsed().as_secs())
    }
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct NodeHealthStatus {
    pub ssh_status: FailureTracker,
    pub rpc_status: FailureTracker,
    #[allow(dead_code)]
    pub is_voting: bool,
    #[allow(dead_code)]
    pub last_vote_slot: Option<u64>,
    #[allow(dead_code)]
    pub last_vote_time: Option<Instant>,
}
