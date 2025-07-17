use anyhow::Result;
use chrono::Local;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

/// Thread-safe logger for startup diagnostics
#[derive(Clone)]
pub struct StartupLogger {
    log_path: PathBuf,
    file: Arc<Mutex<std::fs::File>>,
}

impl StartupLogger {
    /// Create a new startup logger with timestamp-based filename
    pub fn new() -> Result<Self> {
        // Create logs directory in config directory
        let config_dir = dirs::home_dir()
            .expect("Failed to get home directory")
            .join(".solana-validator-switch")
            .join("logs");

        fs::create_dir_all(&config_dir)?;

        // Create log file with timestamp
        let timestamp = Local::now().format("%Y%m%d_%H%M%S");
        let log_filename = format!("startup_{}.log", timestamp);
        let log_path = config_dir.join(log_filename);

        let file = OpenOptions::new()
            .create(true)
            .write(true)
            .append(true)
            .open(&log_path)?;

        let logger = StartupLogger {
            log_path,
            file: Arc::new(Mutex::new(file)),
        };

        // Write header
        logger.log_section("Solana Validator Switch - Startup Diagnostics")?;
        logger.log(&format!(
            "Started at: {}",
            Local::now().format("%Y-%m-%d %H:%M:%S")
        ))?;
        logger.log(&format!("Version: {}", env!("CARGO_PKG_VERSION")))?;
        logger.log_separator()?;

        Ok(logger)
    }

    /// Get the path to the log file
    pub fn get_log_path(&self) -> &Path {
        &self.log_path
    }

    /// Log a message with timestamp
    pub fn log(&self, message: &str) -> Result<()> {
        let timestamp = Local::now().format("%H:%M:%S%.3f");
        let formatted = format!("[{}] {}\n", timestamp, message);

        if let Ok(mut file) = self.file.lock() {
            file.write_all(formatted.as_bytes())?;
            file.flush()?;
        }

        Ok(())
    }

    /// Log a section header
    pub fn log_section(&self, title: &str) -> Result<()> {
        self.log("")?;
        self.log(&format!("=== {} ===", title))?;
        Ok(())
    }

    /// Log a separator line
    pub fn log_separator(&self) -> Result<()> {
        self.log(&"-".repeat(80))?;
        Ok(())
    }

    /// Log an error with context
    pub fn log_error(&self, context: &str, error: &str) -> Result<()> {
        self.log(&format!("ERROR [{}]: {}", context, error))?;
        Ok(())
    }

    /// Log a warning
    pub fn log_warning(&self, warning: &str) -> Result<()> {
        self.log(&format!("WARNING: {}", warning))?;
        Ok(())
    }

    /// Log a success message
    pub fn log_success(&self, message: &str) -> Result<()> {
        self.log(&format!("SUCCESS: {}", message))?;
        Ok(())
    }

    /// Log SSH command and output
    pub fn log_ssh_command(
        &self,
        host: &str,
        command: &str,
        output: &str,
        error: Option<&str>,
    ) -> Result<()> {
        self.log(&format!("SSH [{}] Command: {}", host, command))?;
        if !output.is_empty() {
            self.log(&format!("SSH [{}] Output:", host))?;
            for line in output.lines() {
                self.log(&format!("  {}", line))?;
            }
        }
        if let Some(err) = error {
            self.log(&format!("SSH [{}] Error: {}", host, err))?;
        }
        Ok(())
    }

    /// Log node status details
    pub fn log_node_status(&self, node_label: &str, details: Vec<(&str, &str)>) -> Result<()> {
        self.log(&format!("Node Status: {}", node_label))?;
        for (key, value) in details {
            self.log(&format!("  {}: {}", key, value))?;
        }
        Ok(())
    }

    /// Create a symlink to the latest log
    pub fn create_latest_symlink(&self) -> Result<()> {
        let latest_path = self.log_path.parent().unwrap().join("latest.log");

        // Remove existing symlink if it exists
        let _ = fs::remove_file(&latest_path);

        // Create new symlink
        #[cfg(unix)]
        {
            std::os::unix::fs::symlink(&self.log_path, &latest_path)?;
        }

        Ok(())
    }
}
