# Solana Validator Switch CLI - Technical Specification

## Architecture Overview

### **Pure CLI Architecture**
- **Technology Stack**: Rust, Tokio, Clap, Ratatui
- **SSH Library**: openssh-rs v0.10 (with native multiplexing and connection pooling)
- **UI Framework**: Ratatui for interactive dashboard
- **Configuration**: YAML-based (~/.solana-validator-switch/config.yaml)
- **Async Runtime**: Tokio for high-performance async operations
- **No Browser**: Runs entirely in terminal, no web components

### **Persistent SSH Connection Pool**
```rust
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
```

### **Connection Management**
- **Connection Pooling**: Reusable SSH sessions with Arc<Session> for thread safety
- **Multiplexing**: OpenSSH native multiplexing with ControlPersist
- **Session Validation**: Automatic session health checks with lightweight commands
- **Auto-reconnect**: Automatic reconnection on session failure
- **Connection Caching**: Sessions cached by host, user, port, and SSH key path

---

## Switch Execution Flow

### **Realistic Timing Sequence:**
```
1. Pre-flight checks        (2-3 seconds)
2. Stop primary validator   (3-5 seconds)
3. Transfer tower file      (1-2 seconds) - Optimized with streaming
4. Start backup validator   (5-10 seconds)
5. Verify voting           (15-20 seconds)

Total switch time: 25-40 seconds
Voting gap: 15-25 seconds
```

### **Tower File Transfer Implementation:**
```rust
// Optimized tower transfer using streaming base64 decode + dd
pub async fn transfer_base64_to_file(
    &self,
    node: &NodeConfig,
    ssh_key_path: &str,
    remote_path: &str,
    base64_data: &str,
) -> Result<()> {
    let session = self.get_session(node, ssh_key_path).await?;
    
    // Step 1: Start base64 -d on remote, writing to stdout
    let mut base64_child = session
        .command("base64")
        .arg("-d")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .await?;

    // Step 2: Pipe base64 data to stdin and read decoded output
    if let Some(mut stdin) = base64_child.stdin().take() {
        stdin.write_all(base64_data.as_bytes()).await?;
        stdin.flush().await?;
        drop(stdin);
    }

    let mut stdout = base64_child.stdout().take().unwrap();
    let mut decoded = Vec::new();
    tokio::io::copy(&mut stdout, &mut decoded).await?;

    // Step 3: Wait for base64 command to complete
    let status = base64_child.wait().await?;
    if !status.success() {
        return Err(anyhow!("base64 -d command failed"));
    }

    // Step 4: Use dd to write decoded content (avoids shell redirection)
    let mut dd_child = session
        .command("dd")
        .arg(format!("of={}", remote_path))
        .stdin(Stdio::piped())
        .spawn()
        .await?;

    if let Some(mut dd_stdin) = dd_child.stdin().take() {
        dd_stdin.write_all(&decoded).await?;
        dd_stdin.flush().await?;
        drop(dd_stdin);
    }

    let dd_status = dd_child.wait().await?;
    if !dd_status.success() {
        return Err(anyhow!("dd command failed"));
    }

    Ok(())
}
```

---

## Performance Characteristics

### **Realistic Performance Metrics:**

#### **Connection Operations:**
- **Initial SSH connection**: 1-2 seconds
- **Persistent connection command**: 50-100ms
- **Cross-datacenter latency**: 20-50ms
- **Command execution overhead**: 10-20ms

#### **File Operations:**
- **Tower file size**: 2-5KB typically
- **Read time**: 50-100ms (base64 encoding on source)
- **Transfer time**: 100-300ms (optimized streaming with base64 -d + dd)
- **Write time**: 50-100ms (direct dd write, no shell redirection)

#### **Validator Operations:**
- **Stop validator**: 3-5 seconds (graceful shutdown)
- **Start validator**: 5-10 seconds (initialization)
- **Vote verification**: 15-20 seconds (wait for consensus)

### **Resource Usage:**
- **Memory**: ~30MB base + 5MB per SSH session (Arc<Session> efficiency)
- **CPU**: <3% during monitoring, 8-15% during switch
- **Network**: Minimal (1KB/s monitoring, 50KB during switch)

---

## Error Handling & Recovery

### **Error Classification:**
```typescript
enum ErrorSeverity {
  CRITICAL = 'critical',   // Cannot continue
  WARNING = 'warning',     // Can continue with limitations
  INFO = 'info'           // Informational only
}

interface SwitchError {
  code: string;
  message: string;
  severity: ErrorSeverity;
  recoverable: boolean;
  suggestions: string[];
}
```

### **Recovery Strategies:**

#### **Connection Recovery:**
```typescript
class ConnectionRecovery {
  async recover(node: NodeConfig): Promise<SSHConnection> {
    const strategies = [
      this.reconnectExisting,      // Try existing connection
      this.createNewConnection,    // Create fresh connection
      this.tryAlternatePort,      // Try alternate SSH port
      this.diagnosticMode         // Run diagnostics
    ];
    
    for (const strategy of strategies) {
      try {
        return await strategy(node);
      } catch (error) {
        continue;
      }
    }
    
    throw new UnrecoverableError('All recovery strategies failed');
  }
}
```

#### **Switch Recovery:**
```typescript
interface RecoveryPlan {
  strategy: 'rollback' | 'retry' | 'manual';
  steps: RecoveryStep[];
  estimatedTime: number;
  riskLevel: 'low' | 'medium' | 'high';
}

class SwitchRecovery {
  async createRecoveryPlan(
    error: SwitchError,
    state: SwitchState
  ): Promise<RecoveryPlan> {
    switch (state.phase) {
      case 'pre-flight':
        return this.createPreFlightRecovery(error);
      case 'stopping-primary':
        return this.createStopRecovery(error);
      case 'transferring-tower':
        return this.createTransferRecovery(error);
      case 'starting-backup':
        return this.createStartRecovery(error);
      case 'verification':
        return this.createVerificationRecovery(error);
    }
  }
}
```

---

## Real-Time Monitoring System

### **Data Collection Pipeline:**
```typescript
interface MonitoringData {
  slot: number;
  voteDistance: number;
  lastVoteTime: number;
  health: HealthStatus;
  identity: string;
  version: string;
  client: ValidatorClient;
  sshConnectivity: boolean;
  rpcHealth: boolean;
}

class Monitor {
  private async collectNodeData(ssh: SSHConnection): Promise<MonitoringData> {
    // Parallel data collection for efficiency
    const [slot, voteInfo, identity, rpcHealth] = await Promise.all([
      this.getSlot(ssh),
      this.getVoteInfo(ssh),
      this.getIdentity(ssh),
      this.getRpcHealth(ssh)
    ]);
    
    return {
      slot,
      voteDistance: voteInfo.distance,
      lastVoteTime: voteInfo.lastVote,
      health: this.calculateHealth(voteInfo),
      identity: identity.identity,
      version: identity.version,
      client: this.detectClient(identity.version),
      sshConnectivity: true,  // If we got here, SSH is connected
      rpcHealth
    };
  }
}
```

### **Health Scoring Algorithm:**
```typescript
calculateHealth(data: MonitoringData): HealthScore {
  let score = 100;
  
  // Vote distance impact
  if (data.voteDistance <= 3) score -= 0;
  else if (data.voteDistance <= 10) score -= 10;
  else if (data.voteDistance <= 50) score -= 30;
  else score -= 50;
  
  // SSH connectivity impact
  if (!data.sshConnectivity) score -= 40;
  
  // RPC health impact
  if (!data.rpcHealth) score -= 30;
  
  // Last vote time impact
  const secondsSinceVote = Date.now() / 1000 - data.lastVoteTime;
  if (secondsSinceVote > 30) score -= 20;
  if (secondsSinceVote > 60) score -= 30;
  
  return {
    score,
    status: score >= 90 ? 'excellent' : 
            score >= 70 ? 'good' : 
            score >= 50 ? 'fair' : 'poor'
  };
}
```

---

## Configuration Management

### **Configuration Schema:**
```typescript
interface Config {
  version: string;
  nodes: {
    primary: NodeConfig;
    backup: NodeConfig;
  };
  rpc: {
    endpoint: string;
    timeout: number;
  };
  monitoring: {
    interval: number;        // milliseconds
    healthThreshold: number; // vote distance
    readinessThreshold: number;
  };
  security: {
    confirmSwitches: boolean;
    sessionTimeout: number;  // minutes
    maxRetries: number;
  };
  display: {
    theme: 'dark' | 'light';
    compact: boolean;
    showTechnicalDetails: boolean;
  };
}
```

### **File-Based Storage:**
```typescript
class ConfigManager {
  private configPath = path.join(
    os.homedir(), 
    '.solana-validator-switch', 
    'config.json'
  );
  
  async save(config: Config): Promise<void> {
    // Ensure directory exists
    await fs.mkdir(path.dirname(this.configPath), { recursive: true });
    
    // Write with atomic operation
    const tempFile = `${this.configPath}.tmp`;
    await fs.writeFile(tempFile, JSON.stringify(config, null, 2));
    await fs.rename(tempFile, this.configPath);
    
    // Set secure permissions (owner read/write only)
    await fs.chmod(this.configPath, 0o600);
  }
}
```

---

## Terminal UI Architecture

### **Interactive Dashboard Stack:**
```rust
// Using Ratatui for terminal UI with auto-refresh countdown
pub struct DashboardState {
    pub validators: Vec<ValidatorInfo>,
    pub last_refresh: Instant,
    pub auto_refresh_interval: Duration,
    pub ssh_connectivity: HashMap<String, bool>,
    pub rpc_health: HashMap<String, bool>,
}

impl Dashboard {
    pub async fn run(&mut self) -> Result<()> {
        // Setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        
        // Main event loop
        loop {
            // Event polling with 10ms timeout for responsive keypresses
            if event::poll(Duration::from_millis(10))? {
                match event::read()? {
                    Event::Key(key) if key.code == KeyCode::Char('q') => break,
                    Event::Key(key) if key.code == KeyCode::Char('r') => {
                        self.refresh_all_fields().await?;
                    },
                    Event::Key(key) if key.code == KeyCode::Char('s') => {
                        self.initiate_switch().await?;
                    },
                    _ => {}
                }
            }
            
            // Auto-refresh check
            if self.last_refresh.elapsed() >= Duration::from_secs(10) {
                self.refresh_all_fields().await?;
            }
            
            // Render UI with countdown
            self.render_ui()?;
        }
        
        Ok(())
    }
    
    fn render_footer(&self, area: Rect, buf: &mut Buffer) {
        let time_until_refresh = Duration::from_secs(10) - self.last_refresh.elapsed();
        let footer_text = format!(
            "(Q)uit | (R)efresh (in {}s) | (S)witch",
            time_until_refresh.as_secs()
        );
        
        Paragraph::new(footer_text)
            .style(Style::default().fg(Color::Gray))
            .alignment(Alignment::Center)
            .render(area, buf);
    }
}
```

---

## Security Considerations

### **SSH Key Management:**
```typescript
class SSHKeyManager {
  async detectKeys(): Promise<SSHKey[]> {
    const sshDir = path.join(os.homedir(), '.ssh');
    const keyPatterns = ['id_rsa', 'id_ed25519', 'id_ecdsa'];
    
    const keys: SSHKey[] = [];
    for (const pattern of keyPatterns) {
      const keyPath = path.join(sshDir, pattern);
      if (await this.isValidKey(keyPath)) {
        keys.push({
          path: keyPath,
          type: this.detectKeyType(keyPath),
          fingerprint: await this.getFingerprint(keyPath)
        });
      }
    }
    
    return keys;
  }
}
```

### **No Credential Storage:**
- SSH keys referenced by path only
- No passwords stored anywhere
- No private key material in config
- Session data in memory only

---

## Performance Optimizations

### **Connection Pooling:**
```typescript
class SSHConnectionPool {
  private pools: Map<string, SSHConnection[]> = new Map();
  
  async getConnection(node: NodeConfig): Promise<SSHConnection> {
    const key = `${node.host}:${node.port}`;
    const pool = this.pools.get(key) || [];
    
    // Find available connection
    const available = pool.find(conn => !conn.busy);
    if (available) {
      available.busy = true;
      return available;
    }
    
    // Create new connection if under limit
    if (pool.length < 2) {
      const conn = await this.createConnection(node);
      pool.push(conn);
      this.pools.set(key, pool);
      return conn;
    }
    
    // Wait for available connection
    return this.waitForConnection(key);
  }
}
```

### **Command Batching:**
```typescript
class CommandBatcher {
  private queue: Command[] = [];
  private timer: NodeJS.Timer;
  
  async execute(cmd: Command): Promise<Result> {
    return new Promise((resolve, reject) => {
      this.queue.push({ ...cmd, resolve, reject });
      
      if (!this.timer) {
        this.timer = setTimeout(() => this.flush(), 10);
      }
    });
  }
  
  private async flush(): Promise<void> {
    const batch = this.queue.splice(0);
    const results = await this.executeBatch(batch);
    
    batch.forEach((cmd, i) => {
      if (results[i].error) {
        cmd.reject(results[i].error);
      } else {
        cmd.resolve(results[i].data);
      }
    });
  }
}
```

---

## Expected Performance

### **Monitoring Mode:**
- **Update frequency**: 10 seconds (with countdown timer)
- **Manual refresh**: Instant via 'R' key
- **CPU usage**: <3%
- **Memory usage**: ~30MB
- **Network usage**: ~1KB/s

### **Switch Operation:**
- **Total time**: 30-45 seconds
- **Pre-flight**: 2-3 seconds
- **Execution**: 20-30 seconds
- **Verification**: 8-12 seconds
- **Voting gap**: 15-25 seconds

### **Comparison to Manual Process:**
- **Manual switch**: 2-5 minutes
- **CLI switch**: 30-45 seconds
- **Improvement**: 75-85% faster
- **Error rate**: 90% reduction

This architecture delivers a professional-grade CLI tool optimized for reliability and speed while maintaining security best practices.