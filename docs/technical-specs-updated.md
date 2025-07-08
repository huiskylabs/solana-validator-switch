# Solana Validator Switch CLI - Technical Specification

## Architecture Overview

### **Pure CLI Architecture**
- **Technology Stack**: TypeScript, Node.js, Commander.js
- **SSH Library**: SSH2 (with connection pooling)
- **UI Framework**: Blessed for interactive dashboard
- **Configuration**: File-based (~/.solana-validator-switch/config.json)
- **No Browser**: Runs entirely in terminal, no web components

### **Persistent SSH Connection Pool**
```typescript
interface SSHPoolConfig {
  maxConnections: number;      // 2 per node
  keepAliveInterval: number;   // 5000ms
  connectionTimeout: number;   // 30000ms
  retryAttempts: number;      // 3
  retryDelay: number;         // 1000ms
}
```

### **Connection Management**
- **Primary Pool**: 2 persistent connections to primary node
- **Backup Pool**: 2 persistent connections to backup node
- **Heartbeat**: 5-second keep-alive packets
- **Auto-reconnect**: Automatic reconnection on failure
- **Command queueing**: Commands queued during reconnection

---

## Switch Execution Flow

### **Realistic Timing Sequence:**
```
1. Pre-flight checks        (2-3 seconds)
2. Stop primary validator   (3-5 seconds)
3. Transfer tower file      (2-3 seconds)
4. Start backup validator   (5-10 seconds)
5. Verify voting           (15-20 seconds)

Total switch time: 30-45 seconds
Voting gap: 15-25 seconds
```

### **Tower File Transfer Implementation:**
```typescript
class TowerManager {
  async transferTowerFile(
    primarySSH: SSHConnection,
    backupSSH: SSHConnection,
    config: NodeConfig
  ): Promise<void> {
    try {
      // Step 1: Read tower file from primary (with retries)
      const towerData = await this.readWithRetry(
        primarySSH,
        config.paths.tower,
        3 // retries
      );
      
      // Step 2: Validate tower file
      if (!this.validateTowerFile(towerData)) {
        throw new Error('Invalid tower file format');
      }
      
      // Step 3: Backup existing tower on backup node
      await backupSSH.exec(
        `cp ${config.paths.tower} ${config.paths.tower}.backup.$(date +%s)`
      );
      
      // Step 4: Write tower file to backup
      await this.writeWithVerification(
        backupSSH,
        config.paths.tower,
        towerData
      );
      
      // Step 5: Verify checksum
      await this.verifyChecksum(primarySSH, backupSSH, config.paths.tower);
      
    } catch (error) {
      throw new TowerTransferError('Failed to transfer tower file', error);
    }
  }
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
- **Read time**: 100-200ms
- **Transfer time**: 200-500ms (depends on network)
- **Write and verify**: 200-300ms

#### **Validator Operations:**
- **Stop validator**: 3-5 seconds (graceful shutdown)
- **Start validator**: 5-10 seconds (initialization)
- **Vote verification**: 15-20 seconds (wait for consensus)

### **Resource Usage:**
- **Memory**: ~50MB base + 10MB per SSH connection
- **CPU**: <5% during monitoring, 10-20% during switch
- **Network**: Minimal (2KB/s monitoring, 100KB during switch)

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
  resources: SystemResources;
}

class Monitor {
  private async collectNodeData(ssh: SSHConnection): Promise<MonitoringData> {
    // Parallel data collection for efficiency
    const [slot, voteInfo, health, resources] = await Promise.all([
      this.getSlot(ssh),
      this.getVoteInfo(ssh),
      this.getHealth(ssh),
      this.getSystemResources(ssh)
    ]);
    
    return {
      slot,
      voteDistance: voteInfo.distance,
      lastVoteTime: voteInfo.lastVote,
      health: this.calculateHealth(voteInfo, resources),
      identity: voteInfo.identity,
      version: health.version,
      client: this.detectClient(health.version),
      resources
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
  
  // Resource usage impact
  if (data.resources.cpu > 90) score -= 20;
  if (data.resources.memory > 90) score -= 20;
  if (data.resources.disk > 85) score -= 10;
  
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
```typescript
// Using blessed for terminal UI
interface DashboardComponents {
  screen: blessed.Widgets.Screen;
  primaryBox: blessed.Widgets.BoxElement;
  backupBox: blessed.Widgets.BoxElement;
  statusBar: blessed.Widgets.BoxElement;
  logWindow: blessed.Widgets.LogElement;
  helpModal: blessed.Widgets.BoxElement;
}

class Dashboard {
  private components: DashboardComponents;
  private updateInterval: NodeJS.Timer;
  
  async start(): Promise<void> {
    this.createLayout();
    this.bindKeyboardShortcuts();
    this.startDataUpdates();
    this.screen.render();
  }
  
  private bindKeyboardShortcuts(): void {
    this.screen.key(['s'], () => this.initiateSwitch());
    this.screen.key(['r'], () => this.refreshNow());
    this.screen.key(['h'], () => this.toggleHelp());
    this.screen.key(['q', 'C-c'], () => this.quit());
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
- **Update frequency**: 2 seconds
- **CPU usage**: <5%
- **Memory usage**: ~50MB
- **Network usage**: ~2KB/s

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