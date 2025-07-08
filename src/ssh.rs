use anyhow::{Result, anyhow};
use ssh2::Session;
use std::net::TcpStream;
use std::path::Path;
use std::time::{Duration, Instant};
use std::io::Read;

use crate::types::{NodeConfig, ConnectionStatus, ValidationResult};

pub struct SshManager {
    session: Option<Session>,
}

impl SshManager {
    pub fn new() -> Self {
        SshManager { session: None }
    }
    
    pub async fn connect(&mut self, node: &NodeConfig, ssh_key_path: &str) -> Result<ConnectionStatus> {
        let start_time = Instant::now();
        
        // Connect to TCP stream
        let tcp = TcpStream::connect(format!("{}:{}", node.host, node.port))?;
        tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
        tcp.set_write_timeout(Some(Duration::from_secs(30)))?;
        
        // Create SSH session
        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;
        
        // Authenticate with private key
        let private_key_path = Path::new(ssh_key_path);
        if !private_key_path.exists() {
            return Err(anyhow!("SSH private key not found: {}", ssh_key_path));
        }
        
        // Try with and without passphrase
        let result = session.userauth_pubkey_file(
            &node.user,
            None,
            private_key_path,
            None,
        );
        
        if let Err(_) = result {
            return Err(anyhow!("SSH authentication failed for {}@{}", node.user, node.host));
        }
        
        if !session.authenticated() {
            return Err(anyhow!("SSH authentication failed"));
        }
        
        let latency = start_time.elapsed();
        self.session = Some(session);
        
        Ok(ConnectionStatus {
            connected: true,
            latency_ms: Some(latency.as_millis() as u64),
            error: None,
        })
    }
    
    pub fn execute_command(&self, command: &str) -> Result<String> {
        let session = self.session.as_ref()
            .ok_or_else(|| anyhow!("No active SSH session"))?;
            
        let mut channel = session.channel_session()?;
        channel.exec(command)?;
        
        let mut output = String::new();
        channel.read_to_string(&mut output)?;
        
        channel.wait_close()?;
        let exit_status = channel.exit_status()?;
        
        if exit_status != 0 {
            return Err(anyhow!("Command failed with exit code {}: {}", exit_status, command));
        }
        
        Ok(output.trim().to_string())
    }
    
    pub fn disconnect(&mut self) {
        if let Some(session) = self.session.take() {
            let _ = session.disconnect(None, "", None);
        }
    }
    
    pub fn is_connected(&self) -> bool {
        self.session.is_some()
    }
}

impl Drop for SshManager {
    fn drop(&mut self) {
        self.disconnect();
    }
}

pub async fn validate_node_files(ssh: &SshManager, node: &NodeConfig) -> Result<ValidationResult> {
    let mut valid_files = 0;
    let total_files = 6;
    let mut issues = Vec::new();
    
    // Check ledger directory
    match ssh.execute_command(&format!("test -d \"{}\"", node.paths.ledger)) {
        Ok(_) => valid_files += 1,
        Err(_) => issues.push(format!("Ledger directory missing: {}", node.paths.ledger)),
    }
    
    // Check accounts folder
    match ssh.execute_command(&format!("test -d \"{}/accounts\"", node.paths.ledger)) {
        Ok(_) => valid_files += 1,
        Err(_) => issues.push("Accounts folder missing in ledger directory".to_string()),
    }
    
    // Check tower file
    match ssh.execute_command(&format!("ls {}/tower-1_9-*.bin 2>/dev/null | head -1", node.paths.ledger)) {
        Ok(output) => {
            if !output.is_empty() {
                valid_files += 1;
            } else {
                issues.push("Tower file not found in ledger directory (pattern: tower-1_9-*.bin)".to_string());
            }
        },
        Err(_) => issues.push("Tower file not found in ledger directory (pattern: tower-1_9-*.bin)".to_string()),
    }
    
    // Check keypairs
    for (name, path) in [
        ("Funded identity keypair", &node.paths.funded_identity),
        ("Unfunded identity keypair", &node.paths.unfunded_identity),
        ("Vote account keypair", &node.paths.vote_keypair),
    ] {
        match ssh.execute_command(&format!("test -f \"{}\"", path)) {
            Ok(_) => valid_files += 1,
            Err(_) => issues.push(format!("{} missing: {}", name, path)),
        }
    }
    
    Ok(ValidationResult {
        valid_files,
        total_files,
        issues,
    })
}