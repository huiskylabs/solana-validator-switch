use anyhow::{Result, anyhow};
use serde_json;
use std::fs;
use std::path::PathBuf;
use dirs;

use crate::types::Config;

pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    pub fn new() -> Result<Self> {
        let config_dir = dirs::home_dir()
            .ok_or_else(|| anyhow!("Could not find home directory"))?
            .join(".solana-validator-switch");
            
        // Create config directory if it doesn't exist
        if !config_dir.exists() {
            fs::create_dir_all(&config_dir)?;
        }
        
        let config_path = config_dir.join("config.json");
        
        Ok(ConfigManager { config_path })
    }
    
    pub fn get_config_path(&self) -> &PathBuf {
        &self.config_path
    }
    
    pub fn load(&self) -> Result<Config> {
        if !self.config_path.exists() {
            return Err(anyhow!("Configuration file not found. Run 'svs setup' first."));
        }
        
        let content = fs::read_to_string(&self.config_path)?;
        let config: Config = serde_json::from_str(&content)?;
        Ok(config)
    }
    
    pub fn save(&self, config: &Config) -> Result<()> {
        let content = serde_json::to_string_pretty(config)?;
        fs::write(&self.config_path, content)?;
        Ok(())
    }
    
    pub fn exists(&self) -> bool {
        self.config_path.exists()
    }
    
    pub fn create_default() -> Config {
        use std::collections::HashMap;
        use crate::types::*;
        
        Config {
            version: "1.0.0".to_string(),
            ssh: SshConfig {
                key_path: format!("{}/.ssh/id_rsa", dirs::home_dir().unwrap().display()),
                timeout: 30,
            },
            nodes: HashMap::new(),
            rpc: RpcConfig {
                endpoint: "https://api.mainnet-beta.solana.com".to_string(),
                timeout: 30000,
                retries: 3,
            },
            monitoring: MonitoringConfig {
                interval: 5000,
                health_threshold: 100,
                readiness_threshold: 50,
                enable_metrics: true,
                metrics_retention: 7,
            },
            security: SecurityConfig {
                confirm_switches: true,
                max_retries: 3,
            },
            display: DisplayConfig {
                theme: "dark".to_string(),
                compact: true,
                show_technical_details: false,
            },
        }
    }
}