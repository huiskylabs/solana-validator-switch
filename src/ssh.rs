use crate::types::NodeConfig;
use anyhow::{anyhow, Result};
use openssh::{Session, SessionBuilder, Stdio};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::RwLock;

/// SSH session pool with async support and connection reuse
pub struct AsyncSshPool {
    sessions: Arc<RwLock<HashMap<String, Arc<Session>>>>,
    config: PoolConfig,
}

#[derive(Clone)]
pub struct PoolConfig {
    pub connect_timeout: Duration,
    pub max_idle_time: Duration,
    pub multiplex: bool,
}

impl Default for PoolConfig {
    fn default() -> Self {
        PoolConfig {
            connect_timeout: Duration::from_secs(10),
            max_idle_time: Duration::from_secs(300),
            multiplex: true, // Enable connection multiplexing by default
        }
    }
}

impl AsyncSshPool {
    pub fn new() -> Self {
        Self::with_config(PoolConfig::default())
    }

    pub fn with_config(config: PoolConfig) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
        }
    }

    fn get_connection_key(node: &NodeConfig, ssh_key_path: &str) -> String {
        format!("{}@{}:{}:{}", node.user, node.host, node.port, ssh_key_path)
    }

    /// Check if a command requires shell features (pipes, redirections, etc.)
    fn needs_shell_execution(command: &str) -> bool {
        command.contains('|')
            || command.contains('>')
            || command.contains('<')
            || command.contains('&')
            || command.contains(';')
            || command.contains('$')
            || command.contains('`')
            || command.contains("||")
            || command.contains("&&")
            || command.contains("2>&1")
    }

    /// Get or create an SSH session for a node
    pub async fn get_session(&self, node: &NodeConfig, ssh_key_path: &str) -> Result<Arc<Session>> {
        let key = Self::get_connection_key(node, ssh_key_path);

        // Try to get existing session
        {
            let sessions = self.sessions.read().await;
            if let Some(session) = sessions.get(&key) {
                // Check if session is still alive
                if self.is_session_alive(session).await {
                    return Ok(Arc::clone(session));
                }
            }
        }

        // Create new session
        let session = self.create_session(node, ssh_key_path).await?;
        let session_arc = Arc::new(session);

        // Store session
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(key, Arc::clone(&session_arc));
        }

        Ok(session_arc)
    }

    async fn create_session(&self, node: &NodeConfig, ssh_key_path: &str) -> Result<Session> {
        // Expand the SSH key path
        let expanded_path = if ssh_key_path.starts_with("~") {
            let home = dirs::home_dir().ok_or_else(|| anyhow!("Could not find home directory"))?;
            home.join(&ssh_key_path[2..])
        } else {
            std::path::PathBuf::from(ssh_key_path)
        };

        if !expanded_path.exists() {
            return Err(anyhow!(
                "SSH key file not found: {} (expanded from: {})",
                expanded_path.display(),
                ssh_key_path
            ));
        }

        let mut builder = SessionBuilder::default();
        builder
            .user(node.user.clone())
            .port(node.port)
            .keyfile(&expanded_path)
            .connect_timeout(self.config.connect_timeout);

        // Enable multiplexing if configured
        if self.config.multiplex {
            // Convert Duration to seconds for control persist
            let persist_secs = self.config.max_idle_time.as_secs();
            use std::num::NonZeroUsize;
            if let Some(persist_time) = NonZeroUsize::new(persist_secs as usize) {
                builder.control_persist(openssh::ControlPersist::IdleFor(persist_time));
            } else {
                builder.control_persist(openssh::ControlPersist::Forever);
            }
        }

        let session = builder
            .connect(&node.host)
            .await
            .map_err(|e| anyhow!("Failed to connect to {}@{}: {}", node.user, node.host, e))?;

        Ok(session)
    }

    async fn is_session_alive(&self, session: &Session) -> bool {
        // Simple check by running a lightweight command
        match session.command("true").output().await {
            Ok(output) => output.status.success(),
            Err(_) => false,
        }
    }

    /// Execute a command with arguments and return the output
    pub async fn execute_command_with_args(
        &self,
        node: &NodeConfig,
        ssh_key_path: &str,
        command: &str,
        args: &[&str],
    ) -> Result<String> {
        let session = self.get_session(node, ssh_key_path).await?;

        let mut cmd = session.command(command);
        for arg in args {
            cmd.arg(arg);
        }

        let output = cmd
            .output()
            .await
            .map_err(|e| anyhow!("Failed to execute command: {}", e))?;

        // For commands with args, always return stdout content if available
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // If we have stdout content, return it even if the command "failed"
        if !stdout.is_empty() {
            return Ok(stdout);
        }

        // If no stdout but there's stderr, and command failed, return error
        if !output.status.success() && !stderr.is_empty() {
            return Err(anyhow!("Command failed: {}", stderr));
        }

        // Otherwise return empty string
        Ok(String::new())
    }

    /// Execute a command and return the output
    pub async fn execute_command(
        &self,
        node: &NodeConfig,
        ssh_key_path: &str,
        command: &str,
    ) -> Result<String> {
        let session = self.get_session(node, ssh_key_path).await?;

        let needs_shell = Self::needs_shell_execution(command);

        let output = if needs_shell {
            // Use bash -c with proper argument handling for shell features
            session
                .command("bash")
                .arg("-c")
                .arg(command)
                .output()
                .await
                .map_err(|e| anyhow!("Failed to execute command: {}", e))?
        } else {
            // Execute directly for better performance
            session
                .command(command)
                .output()
                .await
                .map_err(|e| anyhow!("Failed to execute command: {}", e))?
        };

        // For commands with 2>&1, stderr is redirected to stdout, so we should always return stdout
        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // If we have stdout content, return it even if the command "failed"
        // This is important for commands like catchup that might return non-zero exit codes
        if !stdout.is_empty() {
            return Ok(stdout);
        }

        // If no stdout but there's stderr, and command failed, return error
        if !output.status.success() && !stderr.is_empty() {
            return Err(anyhow!("Command failed: {}", stderr));
        }

        // Otherwise return empty string
        Ok(String::new())
    }

    /// Execute a command with early exit based on output
    pub async fn execute_command_with_early_exit<F>(
        &self,
        node: &NodeConfig,
        ssh_key_path: &str,
        command: &str,
        check_fn: F,
    ) -> Result<String>
    where
        F: Fn(&str) -> bool + Send + 'static,
    {
        let session = self.get_session(node, ssh_key_path).await?;

        let needs_shell = Self::needs_shell_execution(command);

        let mut child = if needs_shell {
            session
                .command("bash")
                .arg("-c")
                .arg(command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .await
                .map_err(|e| anyhow!("Failed to spawn command: {}", e))?
        } else {
            session
                .command(command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .await
                .map_err(|e| anyhow!("Failed to spawn command: {}", e))?
        };

        let stdout = child
            .stdout()
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdout"))?;
        let mut reader = BufReader::new(stdout);
        let mut output = String::new();
        let mut line = String::new();

        // Read output line by line
        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => break, // EOF
                Ok(_) => {
                    output.push_str(&line);

                    // Check if we should exit early
                    if check_fn(&output) {
                        // Try to terminate the process
                        // Note: openssh-rs Child doesn't have kill(), just drop the child
                        drop(child);
                        break;
                    }
                }
                Err(e) => return Err(anyhow!("Failed to read output: {}", e)),
            }
        }

        Ok(output)
    }

    /// Execute a command and stream output via channel
    pub async fn execute_command_streaming(
        &self,
        node: &NodeConfig,
        ssh_key_path: &str,
        command: &str,
        tx: tokio::sync::mpsc::Sender<String>,
    ) -> Result<()> {
        let session = self.get_session(node, ssh_key_path).await?;

        let needs_shell = Self::needs_shell_execution(command);

        let mut child = if needs_shell {
            session
                .command("bash")
                .arg("-c")
                .arg(command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .await
                .map_err(|e| anyhow!("Failed to spawn command: {}", e))?
        } else {
            session
                .command(command)
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .await
                .map_err(|e| anyhow!("Failed to spawn command: {}", e))?
        };

        let stdout = child
            .stdout()
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdout"))?;
        let stderr = child
            .stderr()
            .take()
            .ok_or_else(|| anyhow!("Failed to get stderr"))?;

        // Spawn tasks to read stdout and stderr concurrently
        let tx_stdout = tx.clone();
        let stdout_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stdout);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        if tx_stdout.send(line.clone()).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        let tx_stderr = tx;
        let stderr_task = tokio::spawn(async move {
            let mut reader = BufReader::new(stderr);
            let mut line = String::new();

            loop {
                line.clear();
                match reader.read_line(&mut line).await {
                    Ok(0) => break,
                    Ok(_) => {
                        if tx_stderr.send(format!("[ERROR] {}", line)).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        // Wait for both tasks and the command to complete
        let _ = tokio::join!(stdout_task, stderr_task);
        let status = child.wait().await?;

        if !status.success() {
            return Err(anyhow!(
                "Command failed with exit code: {:?}",
                status.code()
            ));
        }

        Ok(())
    }

    /// Execute a command with input
    pub async fn execute_command_with_input(
        &self,
        node: &NodeConfig,
        ssh_key_path: &str,
        command: &str,
        input: &str,
    ) -> Result<String> {
        let session = self.get_session(node, ssh_key_path).await?;

        let needs_shell = Self::needs_shell_execution(command);

        let shell_command = if needs_shell {
            format!("bash -c '{}'", command.replace("'", "'\\'''"))
        } else {
            command.to_string()
        };

        // Create command with input via pipe
        let mut child = session
            .command(&shell_command)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .await
            .map_err(|e| anyhow!("Failed to spawn command: {}", e))?;

        // Write input to stdin
        if let Some(mut stdin) = child.stdin().take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(input.as_bytes()).await?;
            stdin.flush().await?;
            drop(stdin);
        }

        let output = child
            .wait_with_output()
            .await
            .map_err(|e| anyhow!("Failed to get command output: {}", e))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(anyhow!("Command failed: {}", stderr));
        }

        Ok(String::from_utf8_lossy(&output.stdout).to_string())
    }

    /// Optimized tower transfer using base64 -d streaming + dd
    pub async fn transfer_base64_to_file(
        &self,
        node: &NodeConfig,
        ssh_key_path: &str,
        remote_path: &str,
        base64_data: &str,
    ) -> Result<()> {
        let session = self.get_session(node, ssh_key_path).await?;

        // Start base64 -d on remote, writing to stdout
        let mut base64_child = session
            .command("base64")
            .arg("-d")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .await
            .map_err(|e| anyhow!("Failed to spawn base64 command: {}", e))?;

        // Pipe input data
        if let Some(mut stdin) = base64_child.stdin().take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(base64_data.as_bytes()).await?;
            stdin.flush().await?;
            drop(stdin); // Close pipe
        }

        // Read decoded output
        let mut stdout = base64_child
            .stdout()
            .take()
            .ok_or_else(|| anyhow!("Failed to get stdout"))?;
        let mut decoded = Vec::new();
        tokio::io::copy(&mut stdout, &mut decoded).await?;

        // Wait for base64 command to complete
        let status = base64_child.wait().await?;
        if !status.success() {
            return Err(anyhow!("base64 -d command failed"));
        }

        // Now send this decoded content to a remote file using dd
        let mut dd_child = session
            .command("dd")
            .arg(format!("of={}", remote_path))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .await
            .map_err(|e| anyhow!("Failed to spawn dd command: {}", e))?;

        // Write decoded content to dd stdin
        if let Some(mut dd_stdin) = dd_child.stdin().take() {
            use tokio::io::AsyncWriteExt;
            dd_stdin.write_all(&decoded).await?;
            dd_stdin.flush().await?;
            drop(dd_stdin);
        }

        // Wait for dd command to complete
        let dd_status = dd_child.wait().await?;
        if !dd_status.success() {
            return Err(anyhow!("dd command failed"));
        }

        Ok(())
    }

    /// Copy a file to remote host
    pub async fn copy_file_to_remote(
        &self,
        node: &NodeConfig,
        ssh_key_path: &str,
        local_path: &str,
        remote_path: &str,
    ) -> Result<()> {
        let session = self.get_session(node, ssh_key_path).await?;

        // Read file content
        let content = std::fs::read(local_path)?;

        // Use cat command to write to remote file
        let mut child = session
            .command("cat")
            .arg(format!("> {}", remote_path))
            .stdin(Stdio::piped())
            .spawn()
            .await?;

        if let Some(mut stdin) = child.stdin().take() {
            use tokio::io::AsyncWriteExt;
            stdin.write_all(&content).await?;
            stdin.flush().await?;
            drop(stdin);
        }

        let status = child.wait().await?;

        if !status.success() {
            return Err(anyhow!("Failed to copy file: {:?}", status));
        }

        Ok(())
    }

    /// Clear all cached sessions
    pub async fn clear_all_sessions(&self) {
        let mut sessions = self.sessions.write().await;
        sessions.clear();
    }

    /// Get pool statistics
    pub async fn get_stats(&self) -> PoolStats {
        let sessions = self.sessions.read().await;
        let total = sessions.len();

        // Count alive sessions
        let mut alive = 0;
        for session in sessions.values() {
            if self.is_session_alive(session).await {
                alive += 1;
            }
        }

        PoolStats {
            total_sessions: total,
            alive_sessions: alive,
            dead_sessions: total - alive,
        }
    }
}

#[derive(Debug)]
pub struct PoolStats {
    pub total_sessions: usize,
    pub alive_sessions: usize,
    pub dead_sessions: usize,
}

/// SSH command builder for complex commands
#[allow(dead_code)]
pub struct CommandBuilder {
    command: String,
    args: Vec<String>,
    env_vars: HashMap<String, String>,
    working_dir: Option<String>,
}

#[allow(dead_code)]
impl CommandBuilder {
    pub fn new(command: &str) -> Self {
        Self {
            command: command.to_string(),
            args: Vec::new(),
            env_vars: HashMap::new(),
            working_dir: None,
        }
    }

    pub fn arg(mut self, arg: &str) -> Self {
        self.args.push(arg.to_string());
        self
    }

    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        self.args
            .extend(args.into_iter().map(|s| s.as_ref().to_string()));
        self
    }

    pub fn env(mut self, key: &str, value: &str) -> Self {
        self.env_vars.insert(key.to_string(), value.to_string());
        self
    }

    pub fn current_dir(mut self, dir: &str) -> Self {
        self.working_dir = Some(dir.to_string());
        self
    }

    pub fn build(self) -> String {
        let mut cmd = String::new();

        // Add working directory if specified
        if let Some(dir) = self.working_dir {
            cmd.push_str(&format!("cd {} && ", dir));
        }

        // Add environment variables
        for (key, value) in self.env_vars {
            cmd.push_str(&format!("{}={} ", key, value));
        }

        // Add command and arguments
        cmd.push_str(&self.command);
        for arg in self.args {
            cmd.push(' ');
            // Quote arguments if they contain spaces
            if arg.contains(' ') {
                cmd.push_str(&format!("\"{}\"", arg));
            } else {
                cmd.push_str(&arg);
            }
        }

        cmd
    }
}
