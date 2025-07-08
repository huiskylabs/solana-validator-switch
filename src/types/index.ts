// Core configuration interfaces
export interface NodeConfig {
  label: string;
  host: string;
  port: number;
  user: string;
  keyPath: string;
  paths: {
    fundedIdentity: string;
    unfundedIdentity: string;
    ledger: string;
    tower: string;
    solanaCliPath: string;
  };
}

export interface MonitoringConfig {
  interval: number;
  healthThreshold: number;
  readinessThreshold: number;
}

export interface DisplayConfig {
  theme: 'dark' | 'light';
  compact: boolean;
  showTechnicalDetails: boolean;
}

export interface Config {
  version: string;
  nodes: {
    primary: NodeConfig;
    backup: NodeConfig;
  };
  rpc: {
    endpoint: string;
    timeout: number;
  };
  monitoring: MonitoringConfig;
  security: {
    confirmSwitches: boolean;
    sessionTimeout: number;
    maxRetries: number;
  };
  display: DisplayConfig;
}

// Environment configuration
export interface EnvironmentConfig {
  SVS_CONFIG_PATH?: string;
  SVS_SSH_TIMEOUT?: string;
  SVS_LOG_LEVEL?: 'debug' | 'info' | 'warn' | 'error';
  SVS_NO_COLOR?: string;
  SVS_REFRESH_INTERVAL?: string;
  SVS_RPC_ENDPOINT?: string;
  SVS_MAX_RETRIES?: string;
}

// SSH connection types
export interface SSHConnection {
  host: string;
  port: number;
  username: string;
  privateKey: string;
  connected: boolean;
  busy: boolean;
  lastUsed: number;
}

export interface SSHPoolConfig {
  maxConnections: number;
  keepAliveInterval: number;
  connectionTimeout: number;
  retryAttempts: number;
  retryDelay: number;
}

// Health and monitoring types
export interface SystemResources {
  cpu: number;
  memory: number;
  disk: number;
}

export interface HealthStatus {
  score: number;
  status: 'excellent' | 'good' | 'fair' | 'poor';
}

export interface MonitoringData {
  slot: number;
  voteDistance: number;
  lastVoteTime: number;
  health: HealthStatus;
  identity: string;
  version: string;
  client: ValidatorClient;
  resources: SystemResources;
}

// Validator client types
export type ValidatorClient = 'agave' | 'firedancer' | 'jito' | 'unknown';

// Switch operation types
export interface SwitchState {
  phase:
    | 'pre-flight'
    | 'stopping-primary'
    | 'transferring-tower'
    | 'starting-backup'
    | 'verification';
  startTime: number;
  estimatedTime: number;
  progress: number;
  error?: string;
}

export interface SwitchPlan {
  from: NodeConfig;
  to: NodeConfig;
  estimatedTime: number;
  riskLevel: 'low' | 'medium' | 'high';
  warnings: string[];
}

// Error handling types
export enum ErrorSeverity {
  CRITICAL = 'critical',
  WARNING = 'warning',
  INFO = 'info',
}

export interface SwitchError extends Error {
  code: string;
  severity: ErrorSeverity;
  recoverable: boolean;
  suggestions: string[];
  timestamp: number;
}

// CLI command types
export interface CLIOptions {
  config?: string;
  verbose?: boolean;
  quiet?: boolean;
  noColor?: boolean;
  logLevel?: string;
  dryRun?: boolean;
  force?: boolean;
  auto?: boolean;
  // Command-specific options
  list?: boolean;
  edit?: boolean;
  test?: boolean;
  interval?: string;
  compact?: boolean;
  json?: boolean;
  continuous?: boolean;
  threshold?: string;
}

// Recovery types
export interface RecoveryPlan {
  strategy: 'rollback' | 'retry' | 'manual';
  steps: RecoveryStep[];
  estimatedTime: number;
  riskLevel: 'low' | 'medium' | 'high';
}

export interface RecoveryStep {
  description: string;
  command: string;
  timeout: number;
  critical: boolean;
}

// SSH key types
export interface SSHKey {
  path: string;
  type: 'rsa' | 'ed25519' | 'ecdsa' | 'dsa';
  fingerprint: string;
  bits?: number;
  comment?: string;
  created?: Date;
  valid: boolean;
}

// Logger types
export interface LogEntry {
  timestamp: number;
  level: 'debug' | 'info' | 'warn' | 'error';
  message: string;
  context?: Record<string, unknown>;
}

export interface LoggerConfig {
  level: 'debug' | 'info' | 'warn' | 'error';
  file?: string;
  maxSize?: number;
  maxFiles?: number;
  colorize?: boolean;
}
