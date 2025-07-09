use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub version: String,
    pub ssh: SshConfig,
    pub nodes: HashMap<String, NodeConfig>,
    pub rpc: RpcConfig,
    pub monitoring: MonitoringConfig,
    pub security: SecurityConfig,
    pub display: DisplayConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SshConfig {
    #[serde(rename = "keyPath")]
    pub key_path: String,
    pub timeout: u32,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
    pub endpoint: String,
    pub timeout: u32,
    pub retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MonitoringConfig {
    pub interval: u32,
    #[serde(rename = "healthThreshold")]
    pub health_threshold: u32,
    #[serde(rename = "readinessThreshold")]
    pub readiness_threshold: u32,
    #[serde(rename = "enableMetrics")]
    pub enable_metrics: bool,
    #[serde(rename = "metricsRetention")]
    pub metrics_retention: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecurityConfig {
    #[serde(rename = "confirmSwitches")]
    pub confirm_switches: bool,
    #[serde(rename = "maxRetries")]
    pub max_retries: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DisplayConfig {
    pub theme: String,
    pub compact: bool,
    #[serde(rename = "showTechnicalDetails")]
    pub show_technical_details: bool,
}

#[derive(Debug)]
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