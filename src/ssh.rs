use anyhow::{anyhow, Result};
use ssh2::Session;
use std::collections::HashMap;
use std::io::Read;
use std::net::TcpStream;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use crate::types::{ConnectionStatus, NodeConfig};

/// Individual SSH connection with health tracking
pub struct SshConnection {
    session: Session,
    #[allow(dead_code)]
    node_id: String,
    connected_at: Instant,
    last_used: Instant,
    is_healthy: bool,
}

impl SshConnection {
    pub fn new(session: Session, node_id: String) -> Self {
        let now = Instant::now();
        SshConnection {
            session,
            node_id,
            connected_at: now,
            last_used: now,
            is_healthy: true,
        }
    }

    pub fn is_connected(&self) -> bool {
        self.is_healthy && self.session.authenticated()
    }

    pub fn update_last_used(&mut self) {
        self.last_used = Instant::now();
    }

    pub fn mark_unhealthy(&mut self) {
        self.is_healthy = false;
    }

    pub fn execute_command(&mut self, command: &str) -> Result<String> {
        self.execute_command_with_input(command, None)
    }

    pub fn execute_command_with_input(
        &mut self,
        command: &str,
        input: Option<&str>,
    ) -> Result<String> {
        self.update_last_used();

        let mut channel = self.session.channel_session()?;
        channel.exec(command)?;

        // Write input if provided
        if let Some(input_data) = input {
            use std::io::Write;
            channel.write_all(input_data.as_bytes())?;
            channel.flush()?;
            // Close the input stream to signal end of data
            channel.send_eof()?;
        }

        let mut output = String::new();
        channel.read_to_string(&mut output)?;
        channel.wait_close()?;

        let exit_status = channel.exit_status()?;
        if exit_status != 0 {
            return Err(anyhow!(
                "Command failed with exit status {}: {}",
                exit_status,
                output
            ));
        }

        Ok(output)
    }

    pub fn health_check(&mut self) -> bool {
        // Simple health check - try to execute a lightweight command
        match self.execute_command("echo 'health_check'") {
            Ok(output) if output.trim() == "health_check" => {
                self.is_healthy = true;
                true
            }
            _ => {
                self.mark_unhealthy();
                false
            }
        }
    }
}

/// Persistent SSH connection pool manager
pub struct SshConnectionPool {
    connections: HashMap<String, SshConnection>,
    config: PoolConfig,
}

pub struct PoolConfig {
    pub max_idle_time: Duration,
    pub health_check_interval: Duration,
    pub connect_timeout: Duration,
}

impl Default for PoolConfig {
    fn default() -> Self {
        PoolConfig {
            max_idle_time: Duration::from_secs(300),        // 5 minutes
            health_check_interval: Duration::from_secs(60), // 1 minute
            connect_timeout: Duration::from_secs(10),
        }
    }
}

impl SshConnectionPool {
    pub fn new() -> Self {
        SshConnectionPool {
            connections: HashMap::new(),
            config: PoolConfig::default(),
        }
    }

    pub fn with_config(config: PoolConfig) -> Self {
        SshConnectionPool {
            connections: HashMap::new(),
            config,
        }
    }

    /// Get connection ID for a node
    fn get_connection_id(node: &NodeConfig) -> String {
        format!("{}@{}:{}", node.user, node.host, node.port)
    }

    /// Get or create a connection to a node
    pub async fn get_connection(
        &mut self,
        node: &NodeConfig,
        ssh_key_path: &str,
    ) -> Result<&mut SshConnection> {
        let connection_id = Self::get_connection_id(node);

        // Check if we need to reconnect
        let needs_reconnect = if let Some(conn) = self.connections.get_mut(&connection_id) {
            if conn.is_connected() {
                // Quick health check if it's been a while
                if conn.last_used.elapsed() > self.config.health_check_interval {
                    !conn.health_check()
                } else {
                    false
                }
            } else {
                true
            }
        } else {
            true
        };

        if needs_reconnect {
            // Remove any existing unhealthy connection
            self.connections.remove(&connection_id);

            // Create new connection (only print for new connections)
            let session = self.create_session(node, ssh_key_path).await?;
            let connection = SshConnection::new(session, connection_id.clone());

            self.connections.insert(connection_id.clone(), connection);
        } else {
            // Update last used time for existing connection
            if let Some(conn) = self.connections.get_mut(&connection_id) {
                conn.last_used = Instant::now();
            }
        }

        Ok(self.connections.get_mut(&connection_id).unwrap())
    }

    /// Create a new SSH session
    async fn create_session(&self, node: &NodeConfig, ssh_key_path: &str) -> Result<Session> {
        // Expand the SSH key path (handle ~ for home directory)
        let expanded_path = if ssh_key_path.starts_with("~") {
            let home = dirs::home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
            home.join(&ssh_key_path[2..]) // Skip "~/"
        } else {
            PathBuf::from(ssh_key_path)
        };

        // Check if the SSH key file exists
        if !expanded_path.exists() {
            return Err(anyhow!(
                "SSH key file not found: {} (expanded from: {})",
                expanded_path.display(),
                ssh_key_path
            ));
        }

        // Check if we can read the file
        if let Err(e) = std::fs::metadata(&expanded_path) {
            return Err(anyhow!(
                "Cannot access SSH key file {}: {}",
                expanded_path.display(),
                e
            ));
        }

        // Connect to TCP stream with timeout
        let tcp = TcpStream::connect_timeout(
            &format!("{}:{}", node.host, node.port).parse()?,
            self.config.connect_timeout,
        )?;

        tcp.set_read_timeout(Some(self.config.connect_timeout))?;
        tcp.set_write_timeout(Some(self.config.connect_timeout))?;
        tcp.set_nodelay(true)?; // Disable Nagle's algorithm for lower latency

        // Create SSH session
        let mut session = Session::new()?;
        session.set_tcp_stream(tcp);
        session.handshake()?;

        // Authenticate with SSH key
        match session.userauth_pubkey_file(&node.user, None, &expanded_path, None) {
            Ok(_) => {}
            Err(e) => {
                return Err(anyhow!(
                    "SSH authentication failed for user '{}' with key '{}': {}. \
                    Make sure: 1) The private key file exists and is readable, \
                    2) The corresponding public key is in ~/.ssh/authorized_keys on the remote host, \
                    3) The key format is supported (RSA, ED25519, etc.)",
                    node.user, expanded_path.display(), e
                ));
            }
        }

        if !session.authenticated() {
            return Err(anyhow!(
                "SSH authentication failed for {} - key was rejected by server",
                node.user
            ));
        }

        Ok(session)
    }

    /// Execute command on a node with automatic connection management
    pub async fn execute_command_with_input(
        &mut self,
        node: &NodeConfig,
        command: &str,
        input: &str,
    ) -> Result<String> {
        let _node_key = format!("{}@{}:{}", node.user, node.host, node.port);

        // Get connection and execute command
        let connection = self.get_connection(node, "").await?;
        connection.execute_command_with_input(command, Some(input))
    }

    pub async fn connect(&mut self, node: &NodeConfig, ssh_key_path: &str) -> Result<()> {
        let connection_id = Self::get_connection_id(node);

        // Check if connection already exists
        if let Some(conn) = self.connections.get(&connection_id) {
            if conn.is_connected() {
                // Connection already exists and is healthy, no need to reconnect
                return Ok(());
            }
        }

        // Create new connection
        let _connection = self.get_connection(node, ssh_key_path).await?;
        Ok(())
    }

    pub async fn execute_command(
        &mut self,
        node: &NodeConfig,
        ssh_key_path: &str,
        command: &str,
    ) -> Result<String> {
        let connection = self.get_connection(node, ssh_key_path).await?;
        connection.execute_command(command)
    }

    /// Get connection status for a node
    pub fn get_connection_status(&self, node: &NodeConfig) -> Option<ConnectionStatus> {
        let connection_id = Self::get_connection_id(node);

        if let Some(conn) = self.connections.get(&connection_id) {
            let latency = if conn.is_connected() {
                // Calculate average latency based on connection time
                Some(conn.connected_at.elapsed().as_millis() as u64)
            } else {
                None
            };

            Some(ConnectionStatus {
                connected: conn.is_connected(),
                latency_ms: latency,
                error: if conn.is_connected() {
                    None
                } else {
                    Some("Connection unhealthy".to_string())
                },
            })
        } else {
            None
        }
    }

    /// Perform health checks on all connections
    pub fn health_check_all(&mut self) {
        let mut unhealthy_connections = Vec::new();

        for (id, conn) in &mut self.connections {
            if !conn.health_check() {
                unhealthy_connections.push(id.clone());
            }
        }

        // Remove unhealthy connections
        for id in unhealthy_connections {
            println!("🔌 Removing unhealthy connection to {}", id);
            self.connections.remove(&id);
        }
    }

    /// Clean up idle connections
    pub fn cleanup_idle_connections(&mut self) {
        let now = Instant::now();
        let mut idle_connections = Vec::new();

        for (id, conn) in &self.connections {
            if now.duration_since(conn.last_used) > self.config.max_idle_time {
                idle_connections.push(id.clone());
            }
        }

        for id in idle_connections {
            println!("🧹 Cleaning up idle connection to {}", id);
            self.connections.remove(&id);
        }
    }

    /// Get statistics about the connection pool
    pub fn get_pool_stats(&self) -> PoolStats {
        let total_connections = self.connections.len();
        let healthy_connections = self
            .connections
            .values()
            .filter(|conn| conn.is_connected())
            .count();

        PoolStats {
            total_connections,
            healthy_connections,
            unhealthy_connections: total_connections - healthy_connections,
        }
    }

    /// Gracefully disconnect all connections
    pub fn disconnect_all(&mut self) {
        let count = self.connections.len();
        if count > 0 {
            println!("🔌 Disconnecting {} SSH connection(s)...", count);
            self.connections.clear();
        }
    }
}

#[derive(Debug)]
pub struct PoolStats {
    pub total_connections: usize,
    pub healthy_connections: usize,
    pub unhealthy_connections: usize,
}

impl Drop for SshConnectionPool {
    fn drop(&mut self) {
        // Don't automatically disconnect - let the app manage connection lifecycle
    }
}

/// Legacy SshManager for backwards compatibility
#[allow(dead_code)]
pub struct SshManager {
    pool: SshConnectionPool,
    current_node: Option<NodeConfig>,
    current_ssh_key: Option<String>,
}

#[allow(dead_code)]
impl SshManager {
    pub fn new() -> Self {
        SshManager {
            pool: SshConnectionPool::new(),
            current_node: None,
            current_ssh_key: None,
        }
    }

    pub async fn connect(
        &mut self,
        node: &NodeConfig,
        ssh_key_path: &str,
    ) -> Result<ConnectionStatus> {
        self.current_node = Some(node.clone());
        self.current_ssh_key = Some(ssh_key_path.to_string());

        let start_time = Instant::now();
        let _connection = self.pool.get_connection(node, ssh_key_path).await?;
        let latency = start_time.elapsed();

        Ok(ConnectionStatus {
            connected: true,
            latency_ms: Some(latency.as_millis() as u64),
            error: None,
        })
    }

    pub async fn execute_command_with_input(
        &mut self,
        command: &str,
        input: &str,
    ) -> Result<String> {
        self.pool
            .execute_command_with_input(&self.current_node.as_ref().unwrap(), command, input)
            .await
    }

    pub async fn execute_command(&mut self, command: &str) -> Result<String> {
        if let (Some(node), Some(ssh_key)) = (&self.current_node, &self.current_ssh_key) {
            self.pool.execute_command(node, ssh_key, command).await
        } else {
            Err(anyhow!("No active connection. Call connect() first."))
        }
    }

    pub fn is_connected(&self) -> bool {
        if let Some(node) = &self.current_node {
            self.pool
                .get_connection_status(node)
                .map(|status| status.connected)
                .unwrap_or(false)
        } else {
            false
        }
    }

    pub fn disconnect(&mut self) {
        // For backwards compatibility, don't actually disconnect
        // Just clear current node reference
        self.current_node = None;
        self.current_ssh_key = None;
    }

    /// Get access to the connection pool for advanced operations
    pub fn get_pool(&mut self) -> &mut SshConnectionPool {
        &mut self.pool
    }
}

// NOTE: These validation functions are commented out as ledger path is now dynamically detected
// and would require refactoring to accept ledger path as a parameter

/*
/// Validate validator files on a remote node using SSH pool
#[allow(dead_code)]
pub async fn validate_node_files_with_pool(
    ssh_pool: &mut SshConnectionPool,
    node: &NodeConfig,
    ssh_key_path: &str,
) -> Result<ValidationResult> {
    // Implementation commented out - would need ledger path parameter
    unimplemented!("This function needs to be updated to accept ledger path as parameter")
}
*/

/*
/// Validate validator files on a remote node (legacy version using SshManager)
#[allow(dead_code)]
pub async fn validate_node_files(
    ssh_manager: &mut SshManager,
    node: &NodeConfig,
) -> Result<ValidationResult> {
    // Implementation commented out - would need ledger path parameter
    unimplemented!("This function needs to be updated to accept ledger path as parameter")
}
*/
