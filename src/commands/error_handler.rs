use anyhow::anyhow;
use colored::*;
use std::io;

/// Enhanced error types for better UX messaging
#[derive(Debug)]
pub enum SwitchError {
    SshConnectionFailed { host: String, details: String },
    TowerFileNotFound { path: String },
    ExecutableNotFound { name: String, validator_type: String },
    PermissionDenied { operation: String, path: String },
    NetworkTimeout { operation: String, elapsed_secs: u64 },
    PartialSwitch { active_status: String, standby_status: String },
    ConfigurationError { message: String },
    ValidationFailed { issues: Vec<String> },
}

impl SwitchError {
    /// Convert to user-friendly error message with recovery suggestions
    pub fn to_user_message(&self) -> String {
        match self {
            SwitchError::SshConnectionFailed { host, details } => {
                format!(
                    "{}\n{}\n{}\n{}",
                    format!("❌ Failed to connect to validator node {}", host).red().bold(),
                    format!("   Details: {}", details).dimmed(),
                    "💡 Troubleshooting suggestions:".yellow(),
                    "   • Check network connectivity to the host\n   • Verify SSH keys and permissions\n   • Ensure the host is accessible on port 22"
                )
            },
            
            SwitchError::TowerFileNotFound { path } => {
                format!(
                    "{}\n{}\n{}\n{}",
                    "❌ Cannot find tower file for transfer".red().bold(),
                    format!("   Looking in: {}", path).dimmed(),
                    "💡 Troubleshooting suggestions:".yellow(),
                    "   • Verify the validator has been running and producing blocks\n   • Check the ledger path is correct\n   • Ensure the validator has write permissions to the ledger directory"
                )
            },
            
            SwitchError::ExecutableNotFound { name, validator_type } => {
                format!(
                    "{}\n{}\n{}\n{}",
                    format!("❌ Required {} executable '{}' not found", validator_type, name).red().bold(),
                    "   The validator software may not be properly installed".dimmed(),
                    "💡 Troubleshooting suggestions:".yellow(),
                    match validator_type.as_str() {
                        "Firedancer" => "   • Check fdctl is installed and in PATH\n   • Verify firedancer config path in configuration\n   • Run 'which fdctl' on the validator node",
                        _ => "   • Verify validator software is installed\n   • Check PATH environment variable\n   • Ensure binary has execute permissions"
                    }
                )
            },
            
            SwitchError::PermissionDenied { operation, path } => {
                format!(
                    "{}\n{}\n{}\n{}",
                    format!("❌ Permission denied while {}", operation).red().bold(),
                    format!("   Path: {}", path).dimmed(),
                    "💡 Troubleshooting suggestions:".yellow(),
                    "   • Check file ownership and permissions\n   • Ensure user has sudo privileges if required\n   • Verify SELinux/AppArmor policies if applicable"
                )
            },
            
            SwitchError::NetworkTimeout { operation, elapsed_secs } => {
                format!(
                    "{}\n{}\n{}\n{}",
                    format!("❌ Network timeout during {}", operation).red().bold(),
                    format!("   Operation timed out after {} seconds", elapsed_secs).dimmed(),
                    "💡 Troubleshooting suggestions:".yellow(),
                    "   • Check network latency between nodes\n   • Verify firewall rules aren't blocking connections\n   • Consider increasing timeout values for high-latency connections"
                )
            },
            
            SwitchError::PartialSwitch { active_status, standby_status } => {
                format!(
                    "{}\n{}\n{}\n{}\n{}\n{}",
                    "⚠️  Partial switch detected - manual intervention required".yellow().bold(),
                    format!("   Active node: {}", active_status).dimmed(),
                    format!("   Standby node: {}", standby_status).dimmed(),
                    "💡 Recovery steps:".yellow(),
                    "   1. Run 'svs status' to verify current validator states",
                    "   2. Check validator logs on both nodes for errors\n   3. Manually complete the switch or roll back as needed\n   4. Contact support if the issue persists"
                )
            },
            
            SwitchError::ConfigurationError { message } => {
                format!(
                    "{}\n{}\n{}\n{}",
                    "❌ Configuration error detected".red().bold(),
                    format!("   {}", message).dimmed(),
                    "💡 Troubleshooting suggestions:".yellow(),
                    "   • Review your config.yaml file\n   • Ensure all paths are absolute and exist\n   • Verify validator public keys are correct"
                )
            },
            
            SwitchError::ValidationFailed { issues } => {
                let issues_formatted = issues.iter()
                    .map(|issue| format!("   • {}", issue))
                    .collect::<Vec<_>>()
                    .join("\n");
                
                format!(
                    "{}\n{}\n{}\n{}",
                    "❌ Validation failed".red().bold(),
                    "   The following issues were detected:".dimmed(),
                    issues_formatted.red(),
                    "\n💡 Please fix these issues before attempting to switch validators".yellow()
                )
            },
        }
    }
    
    /// Get exit code for this error type
    pub fn exit_code(&self) -> i32 {
        match self {
            SwitchError::SshConnectionFailed { .. } => 10,
            SwitchError::TowerFileNotFound { .. } => 11,
            SwitchError::ExecutableNotFound { .. } => 12,
            SwitchError::PermissionDenied { .. } => 13,
            SwitchError::NetworkTimeout { .. } => 14,
            SwitchError::PartialSwitch { .. } => 15,
            SwitchError::ConfigurationError { .. } => 16,
            SwitchError::ValidationFailed { .. } => 17,
        }
    }
}

/// Wrap anyhow errors with better context
pub fn enhance_error_context(error: anyhow::Error) -> anyhow::Error {
    let error_string = error.to_string();
    
    // Map common error patterns to enhanced errors
    if error_string.contains("Connection refused") || error_string.contains("Connection timeout") {
        let host = extract_host_from_error(&error_string).unwrap_or_else(|| "unknown".to_string());
        return anyhow!(SwitchError::SshConnectionFailed {
            host,
            details: error_string.clone(),
        }.to_user_message());
    }
    
    if error_string.contains("Permission denied") {
        let path = extract_path_from_error(&error_string).unwrap_or_else(|| "unknown".to_string());
        return anyhow!(SwitchError::PermissionDenied {
            operation: "accessing file".to_string(),
            path,
        }.to_user_message());
    }
    
    if error_string.contains("No tower file found") {
        return anyhow!(SwitchError::TowerFileNotFound {
            path: "/mnt/solana_ledger".to_string(),
        }.to_user_message());
    }
    
    if error_string.contains("executable path not found") {
        let executable = if error_string.contains("fdctl") {
            ("fdctl".to_string(), "Firedancer".to_string())
        } else if error_string.contains("agave") {
            ("agave-validator".to_string(), "Agave".to_string())
        } else {
            ("validator".to_string(), "Unknown".to_string())
        };
        
        return anyhow!(SwitchError::ExecutableNotFound {
            name: executable.0,
            validator_type: executable.1,
        }.to_user_message());
    }
    
    // Return original error if no enhancement applies
    error
}

// Helper functions
fn extract_host_from_error(error: &str) -> Option<String> {
    // Simple extraction - could be enhanced with regex
    if let Some(pos) = error.find('@') {
        let rest = &error[pos+1..];
        if let Some(end) = rest.find(' ') {
            return Some(rest[..end].to_string());
        }
    }
    None
}

fn extract_path_from_error(error: &str) -> Option<String> {
    // Simple extraction - could be enhanced with regex
    if let Some(pos) = error.find('/') {
        let rest = &error[pos..];
        if let Some(end) = rest.find(' ') {
            return Some(rest[..end].to_string());
        }
        return Some(rest.to_string());
    }
    None
}

/// Display a spinner with a message during long operations
pub struct ProgressSpinner {
    message: String,
    handle: Option<std::thread::JoinHandle<()>>,
    running: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl ProgressSpinner {
    pub fn new(message: &str) -> Self {
        let running = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true));
        let running_clone = running.clone();
        let message_clone = message.to_string();
        
        let handle = std::thread::spawn(move || {
            let spinner_chars = vec!['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut i = 0;
            
            while running_clone.load(std::sync::atomic::Ordering::Relaxed) {
                print!("\r{} {} ", spinner_chars[i], message_clone);
                io::Write::flush(&mut io::stdout()).unwrap();
                i = (i + 1) % spinner_chars.len();
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            print!("\r"); // Clear the line
            io::Write::flush(&mut io::stdout()).unwrap();
        });
        
        Self {
            message: message.to_string(),
            handle: Some(handle),
            running,
        }
    }
    
    pub fn stop_with_message(mut self, message: &str) {
        self.running.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
        println!("{}", message);
    }
}

impl Drop for ProgressSpinner {
    fn drop(&mut self) {
        self.running.store(false, std::sync::atomic::Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}