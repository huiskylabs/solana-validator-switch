// Configuration schema for Solana Validator Switch CLI
// This file contains all configuration-related TypeScript interfaces

// Node configuration with enhanced validation
export interface NodeConfig {
  label: string;
  host: string;
  port: number;
  user: string;
  keyPath: string;
  paths: NodePaths;
  metadata?: NodeMetadata;
}

export interface NodePaths {
  fundedIdentity: string;
  unfundedIdentity: string;
  ledger: string;
  tower: string;
  solanaCliPath: string;
  configFile?: string;
  logFile?: string;
}

export interface NodeMetadata {
  detected?: boolean;
  clientType?: ValidatorClient;
  clientVersion?: string;
  lastConnected?: number;
  timezone?: string;
  notes?: string;
}

// Monitoring configuration with detailed settings
export interface MonitoringConfig {
  interval: number; // milliseconds
  healthThreshold: number; // vote distance threshold
  readinessThreshold: number; // slots behind threshold
  enableMetrics: boolean;
  metricsRetention: number; // days
  alerting?: AlertingConfig;
}

export interface AlertingConfig {
  enabled: boolean;
  healthAlerts: boolean;
  switchAlerts: boolean;
  errorAlerts: boolean;
  webhookUrl?: string;
  slackChannel?: string;
  discordWebhook?: string;
}

// Display configuration with theme and layout options
export interface DisplayConfig {
  theme: 'dark' | 'light' | 'auto';
  compact: boolean;
  showTechnicalDetails: boolean;
  showTimestamps: boolean;
  showLogLevel: boolean;
  colorOutput: boolean;
  terminalWidth?: number;
  refreshRate: number; // seconds
}

// Security configuration
export interface SecurityConfig {
  confirmSwitches: boolean;
  sessionTimeout: number; // minutes
  maxRetries: number;
  sshTimeout: number; // seconds
  requireHealthCheck: boolean;
  allowForceSwitch: boolean;
  auditLog: boolean;
}

// RPC configuration with failover
export interface RPCConfig {
  endpoint: string;
  timeout: number; // milliseconds
  retries: number;
  failoverEndpoints?: string[];
  customHeaders?: Record<string, string>;
  rateLimiting?: {
    enabled: boolean;
    requestsPerMinute: number;
  };
}

// Main configuration interface
export interface Config {
  version: string;
  configPath?: string;
  nodes: {
    primary: NodeConfig;
    backup: NodeConfig;
  };
  rpc: RPCConfig;
  monitoring: MonitoringConfig;
  security: SecurityConfig;
  display: DisplayConfig;
  advanced?: AdvancedConfig;
}

// Advanced configuration for power users
export interface AdvancedConfig {
  sshPoolSize: number;
  sshKeepAlive: number; // seconds
  commandTimeout: number; // seconds
  towerBackups: number;
  automaticSwitching?: AutoSwitchConfig;
  performance?: PerformanceConfig;
}

export interface AutoSwitchConfig {
  enabled: boolean;
  triggers: {
    voteDistance: number;
    unhealthyDuration: number; // seconds
    diskThreshold: number; // percentage
    memoryThreshold: number; // percentage
  };
  schedule?: {
    enabled: boolean;
    maintenanceWindows: TimeWindow[];
  };
}

export interface TimeWindow {
  start: string; // HH:MM format
  end: string; // HH:MM format
  timezone: string;
  days: number[]; // 0-6, Sunday = 0
}

export interface PerformanceConfig {
  cacheSize: number; // MB
  logLevel: 'debug' | 'info' | 'warn' | 'error';
  maxLogFiles: number;
  maxLogSize: number; // MB
  enableProfiling: boolean;
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
  SVS_THEME?: 'dark' | 'light' | 'auto';
  SVS_COMPACT_MODE?: string;
  SVS_AUTO_SWITCH?: string;
}

// Configuration validation schemas
export interface ConfigValidationSchema {
  required: string[];
  optional: string[];
  types: Record<string, string>;
  ranges: Record<string, [number, number]>;
  patterns: Record<string, RegExp>;
}

export interface ValidationSchema {
  required?: string[];
  optional?: string[];
  types?: Record<string, string>;
  ranges?: Record<string, [number, number]>;
  patterns?: Record<string, RegExp>;
  custom?: Record<string, (value: unknown) => boolean>;
}

// Configuration migration interface
export interface ConfigMigration {
  fromVersion: string;
  toVersion: string;
  migrate: (oldConfig: unknown) => Config;
  validate: (config: unknown) => boolean;
}

// Setup wizard state
export interface SetupWizardState {
  step:
    | 'welcome'
    | 'ssh-keys'
    | 'primary-node'
    | 'backup-node'
    | 'rpc'
    | 'preferences'
    | 'validation'
    | 'complete';
  sshKeys: SSHKey[];
  selectedKey?: SSHKey;
  primaryNode?: Partial<NodeConfig>;
  backupNode?: Partial<NodeConfig>;
  rpcEndpoint?: string;
  preferences?: Partial<DisplayConfig>;
  errors: string[];
  warnings: string[];
}

// Default configuration values
export const DEFAULT_CONFIG: Partial<Config> = {
  version: '1.0.0',
  rpc: {
    endpoint: 'https://api.mainnet-beta.solana.com',
    timeout: 30000,
    retries: 3,
    rateLimiting: {
      enabled: true,
      requestsPerMinute: 100,
    },
  },
  monitoring: {
    interval: 2000,
    healthThreshold: 10,
    readinessThreshold: 50,
    enableMetrics: true,
    metricsRetention: 7,
    alerting: {
      enabled: false,
      healthAlerts: true,
      switchAlerts: true,
      errorAlerts: true,
    },
  },
  security: {
    confirmSwitches: true,
    sessionTimeout: 60,
    maxRetries: 3,
    sshTimeout: 30,
    requireHealthCheck: true,
    allowForceSwitch: false,
    auditLog: true,
  },
  display: {
    theme: 'auto',
    compact: false,
    showTechnicalDetails: false,
    showTimestamps: true,
    showLogLevel: true,
    colorOutput: true,
    refreshRate: 2,
  },
  advanced: {
    sshPoolSize: 2,
    sshKeepAlive: 30,
    commandTimeout: 30,
    towerBackups: 5,
    performance: {
      cacheSize: 100,
      logLevel: 'info',
      maxLogFiles: 5,
      maxLogSize: 10,
      enableProfiling: false,
    },
  },
};

// Configuration file locations
export const CONFIG_PATHS = {
  userConfig: '~/.solana-validator-switch/config.json',
  globalConfig: '/etc/solana-validator-switch/config.json',
  projectConfig: './svs.config.json',
  envFile: '.env',
} as const;

// Validator client types
export type ValidatorClient = 'agave' | 'firedancer' | 'jito' | 'unknown';

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

// Export utility type for partial configurations
export type PartialConfig = Partial<Config>;
export type RequiredConfig = Required<
  Pick<Config, 'version' | 'nodes' | 'rpc'>
>;
export type ConfigUpdate = Partial<Omit<Config, 'version'>>;

// Configuration status
export interface ConfigStatus {
  valid: boolean;
  version: string;
  path: string;
  lastModified: Date;
  errors: string[];
  warnings: string[];
  migrationNeeded: boolean;
}
