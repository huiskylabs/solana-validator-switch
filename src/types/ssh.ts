// SSH-related TypeScript interfaces and types

import type { ConnectConfig } from 'ssh2';

// SSH connection configuration
export interface SSHConnectionConfig extends Partial<ConnectConfig> {
  host: string;
  port: number;
  username: string;
  privateKey?: Buffer | string;
  passphrase?: string;
  agent?: string;
  timeout?: number;
  keepaliveInterval?: number;
  keepaliveCountMax?: number;
  readyTimeout?: number;
}

// SSH connection status
export interface SSHConnectionStatus {
  id: string;
  host: string;
  port: number;
  username: string;
  connected: boolean;
  lastConnected?: Date;
  lastError?: string;
  retryCount: number;
  networkLatency?: number;
}

// SSH command execution result
export interface SSHCommandResult {
  command: string;
  exitCode: number;
  stdout: string;
  stderr: string;
  signal?: string;
  executionTime: number;
  timestamp: Date;
}

// SSH command execution options
export interface SSHCommandOptions {
  timeout?: number;
  cwd?: string;
  env?: Record<string, string>;
  stdin?: string;
  pty?: boolean;
  x11?: boolean;
}

// SSH connection pool configuration
export interface SSHPoolConfig {
  maxConnections: number;
  keepAliveInterval: number;
  keepAliveCountMax: number;
  connectionTimeout: number;
  idleTimeout: number;
  retryAttempts: number;
  retryDelay: number;
}

// SSH diagnostics information
export interface SSHDiagnostics {
  connectionId: string;
  host: string;
  port: number;
  reachable: boolean;
  latency?: number;
  sshService: boolean;
  authentication: boolean;
  keyExchange?: string;
  cipher?: string;
  mac?: string;
  compression?: string;
  serverVersion?: string;
  errors: string[];
  warnings: string[];
  timestamp: Date;
}

// SSH connection events
export type SSHConnectionEvent = 
  | 'connecting'
  | 'connected'
  | 'disconnected'
  | 'error'
  | 'timeout'
  | 'retry'
  | 'keepalive';

// SSH connection event data
export interface SSHConnectionEventData {
  connectionId: string;
  event: SSHConnectionEvent;
  timestamp: Date;
  data?: unknown;
  error?: Error;
}

// SSH file transfer options
export interface SSHFileTransferOptions {
  preserveTimestamps?: boolean;
  preservePermissions?: boolean;
  recursive?: boolean;
  concurrency?: number;
  progress?: (transferred: number, total: number) => void;
}

// SSH tunnel configuration
export interface SSHTunnelConfig {
  localHost?: string;
  localPort: number;
  remoteHost: string;
  remotePort: number;
  autoReconnect?: boolean;
}

// Default SSH configurations
export const DEFAULT_SSH_CONFIG: Partial<SSHConnectionConfig> = {
  port: 22,
  timeout: 30000,
  keepaliveInterval: 30000,
  keepaliveCountMax: 3,
  readyTimeout: 20000,
};

export const DEFAULT_SSH_POOL_CONFIG: SSHPoolConfig = {
  maxConnections: 10,
  keepAliveInterval: 30000,
  keepAliveCountMax: 3,
  connectionTimeout: 30000,
  idleTimeout: 300000, // 5 minutes
  retryAttempts: 3,
  retryDelay: 2000,
};

// SSH error types
export class SSHConnectionError extends Error {
  public readonly host: string;
  public readonly port: number;
  public override readonly cause?: Error;

  constructor(
    message: string,
    host: string,
    port: number,
    cause?: Error
  ) {
    super(message);
    this.name = 'SSHConnectionError';
    this.host = host;
    this.port = port;
    if (cause) this.cause = cause;
  }
}

export class SSHAuthenticationError extends Error {
  public readonly host: string;
  public readonly username: string;
  public override readonly cause?: Error;

  constructor(
    message: string,
    host: string,
    username: string,
    cause?: Error
  ) {
    super(message);
    this.name = 'SSHAuthenticationError';
    this.host = host;
    this.username = username;
    if (cause) this.cause = cause;
  }
}

export class SSHCommandError extends Error {
  constructor(
    message: string,
    public readonly command: string,
    public readonly exitCode: number,
    public readonly stderr: string,
    public readonly stdout: string
  ) {
    super(message);
    this.name = 'SSHCommandError';
  }
}

export class SSHTimeoutError extends Error {
  constructor(
    message: string,
    public readonly timeout: number,
    public readonly operation: string
  ) {
    super(message);
    this.name = 'SSHTimeoutError';
  }
}