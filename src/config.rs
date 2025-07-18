use anyhow::{anyhow, Result};
use std::fs;
use std::path::PathBuf;

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

        let config_path = config_dir.join("config.yaml");

        Ok(ConfigManager { config_path })
    }

    pub fn get_config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub fn load(&self) -> Result<Config> {
        if !self.config_path.exists() {
            return Err(anyhow!(
                "Configuration file not found. Run 'svs setup' first."
            ));
        }

        let content = fs::read_to_string(&self.config_path)?;
        let config: Config = serde_yaml::from_str(&content)?;
        Ok(config)
    }

    #[allow(dead_code)]
    pub fn save(&self, config: &Config) -> Result<()> {
        let content = serde_yaml::to_string(config)?;
        fs::write(&self.config_path, content)?;
        Ok(())
    }

    pub fn exists(&self) -> bool {
        self.config_path.exists()
    }

    #[allow(dead_code)]
    pub fn create_default() -> Config {
        use crate::types::*;

        Config {
            version: "1.0.0".to_string(),
            validators: Vec::new(),
            alert_config: None,
        }
    }
}
