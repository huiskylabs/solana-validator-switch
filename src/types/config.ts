// Configuration schema for Solana Validator Switch CLI
// This file contains all configuration-related TypeScript interfaces

// Node configuration with enhanced validation
export interface NodeConfig {
  label: string;
  host: string;
  port: number;
  user: string;
  paths: NodePaths;
  metadata?: NodeMetadata;
}

export interface NodePaths {
  fundedIdentity: string;
  unfundedIdentity: string;
  voteKeypair: string;
  ledger: string;
  tower: string;
  solanaCliPath: string;
}

export interface NodeMetadata {
  detected?: boolean;
  clientType?: ValidatorClient;
  clientVersion?: string;
  lastConnected?: number;
  timezone?: string;
  notes?: string;
}

// Monitoring configuration - simplified (no advanced options in setup)
export interface MonitoringConfig {
  interval: number; // milliseconds
  healthThreshold: number; // vote distance threshold
  readinessThreshold: number; // slots behind threshold
  enableMetrics: boolean;
  metricsRetention: number; // days
}


// Display configuration with simplified options
export interface DisplayConfig {
  theme: 'dark'; // Fixed to dark theme as per setup simplification
  compact: boolean; // Always true in setup
  showTechnicalDetails: boolean; // Always false in setup
}

// Security configuration - simplified
export interface SecurityConfig {
  confirmSwitches: boolean;
  maxRetries: number;
}

// RPC configuration - simplified
export interface RPCConfig {
  endpoint: string;
  timeout: number; // milliseconds
  retries: number;
}

// SSH configuration for the CLI machine
export interface SSHConfig {
  keyPath: string;
  agent?: boolean;
  timeout: number;
}

// Main configuration interface - simplified
export interface Config {
  version: string;
  configPath?: string;
  ssh: SSHConfig;
  nodes: {
    primary: NodeConfig;
    backup: NodeConfig;
  };
  rpc: RPCConfig;
  monitoring: MonitoringConfig;
  security: SecurityConfig;
  display: DisplayConfig;
}


// Environment configuration - simplified
export interface EnvironmentConfig {
  SVS_CONFIG_PATH?: string;
  SVS_SSH_TIMEOUT?: string;
  SVS_LOG_LEVEL?: 'debug' | 'info' | 'warn' | 'error';
  SVS_RPC_ENDPOINT?: string;
  SVS_REFRESH_INTERVAL?: string;
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

// Default configuration values - simplified to match setup
export const DEFAULT_CONFIG: Partial<Config> = {
  version: '1.0.0',
  rpc: {
    endpoint: 'https://api.mainnet-beta.solana.com',
    timeout: 30000,
    retries: 3,
  },
  monitoring: {
    interval: 5000,
    healthThreshold: 100,
    readinessThreshold: 50,
    enableMetrics: true,
    metricsRetention: 7,
  },
  security: {
    confirmSwitches: true,
    maxRetries: 3,
  },
  display: {
    theme: 'dark',
    compact: true,
    showTechnicalDetails: false,
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
