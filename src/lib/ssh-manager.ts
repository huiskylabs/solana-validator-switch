// SSH Connection Manager - handles persistent SSH connections with pooling and error recovery

import { Client } from 'ssh2';
import { readFileSync } from 'fs';
import { EventEmitter } from 'events';

import type { 
  SSHConnectionConfig, 
  SSHConnectionStatus, 
  SSHCommandResult, 
  SSHCommandOptions,
  SSHConnectionEvent,
  SSHConnectionEventData,
  SSHPoolConfig
} from '../types/ssh';
import { 
  SSHConnectionError, 
  SSHCommandError, 
  SSHTimeoutError,
  DEFAULT_SSH_CONFIG,
  DEFAULT_SSH_POOL_CONFIG
} from '../types/ssh.js';
import { Logger } from '../utils/logger.js';
import type { NodeConfig } from '../types/config.js';

// SSH connection wrapper with persistent connection and retry logic
class SSHConnection extends EventEmitter {
  private client: Client;
  private config: SSHConnectionConfig;
  private status: SSHConnectionStatus;
  private keepAliveTimer?: NodeJS.Timeout;
  private retryTimer?: NodeJS.Timeout;
  private logger: Logger;

  constructor(config: SSHConnectionConfig, _poolConfig: SSHPoolConfig) {
    super();
    this.config = { ...DEFAULT_SSH_CONFIG, ...config };
    this.client = new Client();
    this.logger = new Logger({ level: 'info' });
    
    this.status = {
      id: `${config.host}:${config.port}`,
      host: config.host,
      port: config.port || 22,
      username: config.username,
      connected: false,
      retryCount: 0,
    };

    this.setupEventHandlers();
  }

  private setupEventHandlers(): void {
    this.client.on('ready', () => {
      this.status.connected = true;
      this.status.lastConnected = new Date();
      this.status.retryCount = 0;
      delete this.status.lastError;
      this.startKeepAlive();
      this.logger.info('SSH connection established');
      this.emitEvent('connected');
    });

    this.client.on('error', (error) => {
      this.status.connected = false;
      this.status.lastError = error.message;
      this.logger.error('SSH connection error:', { error: error.message, level: error.level });
      this.emitEvent('error', { error });
    });

    this.client.on('close', () => {
      this.status.connected = false;
      this.stopKeepAlive();
      this.logger.info('SSH connection closed');
      this.emitEvent('disconnected');
    });

    this.client.on('timeout', () => {
      this.status.lastError = 'Connection timeout';
      this.logger.warn('SSH connection timeout');
      this.emitEvent('timeout');
    });
  }

  private emitEvent(event: SSHConnectionEvent, data?: { error?: Error }): void {
    const eventData: SSHConnectionEventData = {
      connectionId: this.status.id,
      event,
      timestamp: new Date(),
      data,
      ...(data?.error && { error: data.error }),
    };
    this.emit('event', eventData);
  }

  private startKeepAlive(): void {
    if (this.keepAliveTimer) {
      clearInterval(this.keepAliveTimer);
    }

    this.keepAliveTimer = setInterval(() => {
      if (this.status.connected) {
        // Keep connection alive by sending a simple command
        this.emitEvent('keepalive');
      }
    }, this.config.keepaliveInterval || 30000);
  }

  private stopKeepAlive(): void {
    if (this.keepAliveTimer) {
      clearInterval(this.keepAliveTimer);
      this.keepAliveTimer = null as any;
    }
  }

  async connect(): Promise<void> {
    return new Promise((resolve, reject) => {
      const timeout = setTimeout(() => {
        reject(new SSHTimeoutError(
          `Connection timeout after ${this.config.timeout}ms`,
          this.config.timeout || 30000,
          'connect'
        ));
      }, this.config.timeout || 30000);

      this.client.once('ready', () => {
        clearTimeout(timeout);
        resolve();
      });

      this.client.once('error', (error) => {
        clearTimeout(timeout);
        reject(new SSHConnectionError(
          `Failed to connect to ${this.config.host}:${this.config.port}`,
          this.config.host,
          this.config.port || 22,
          error
        ));
      });

      this.emitEvent('connecting');
      this.client.connect(this.config);
    });
  }

  async disconnect(): Promise<void> {
    this.stopKeepAlive();
    if (this.retryTimer) {
      clearTimeout(this.retryTimer);
    }
    
    if (this.status.connected) {
      this.client.end();
    }
  }

  async executeCommand(
    command: string, 
    options: SSHCommandOptions = {}
  ): Promise<SSHCommandResult> {
    if (!this.status.connected) {
      throw new SSHConnectionError(
        'SSH connection not established',
        this.config.host,
        this.config.port || 22
      );
    }

    return new Promise((resolve, reject) => {
      const startTime = Date.now();
      let stdout = '';
      let stderr = '';

      const timeout = setTimeout(() => {
        reject(new SSHTimeoutError(
          `Command timeout after ${options.timeout || 30000}ms`,
          options.timeout || 30000,
          'executeCommand'
        ));
      }, options.timeout || 30000);

      this.client.exec(command, { 
        pty: options.pty || false,
        x11: options.x11 || false,
        ...(options.env && { env: options.env }),
      }, (err, stream) => {
        if (err) {
          clearTimeout(timeout);
          reject(new SSHCommandError(
            `Failed to execute command: ${command}`,
            command,
            -1,
            err.message,
            ''
          ));
          return;
        }

        if (options.stdin) {
          stream.write(options.stdin);
          stream.end();
        }

        stream.on('close', (code: number, signal: string) => {
          clearTimeout(timeout);
          const result: SSHCommandResult = {
            command,
            exitCode: code,
            stdout,
            stderr,
            signal,
            executionTime: Date.now() - startTime,
            timestamp: new Date(),
          };

          if (code === 0) {
            resolve(result);
          } else {
            reject(new SSHCommandError(
              `Command failed with exit code ${code}`,
              command,
              code,
              stderr,
              stdout
            ));
          }
        });

        stream.on('data', (data: Buffer) => {
          stdout += data.toString();
        });

        stream.stderr.on('data', (data: Buffer) => {
          stderr += data.toString();
        });
      });
    });
  }

  getStatus(): SSHConnectionStatus {
    return { ...this.status };
  }

  isConnected(): boolean {
    return this.status.connected;
  }
}

// Main SSH Manager class with connection pooling
export class SSHManager extends EventEmitter {
  private connections: Map<string, SSHConnection> = new Map();
  private poolConfig: SSHPoolConfig;
  private logger: Logger;

  constructor(poolConfig: Partial<SSHPoolConfig> = {}) {
    super();
    this.poolConfig = { ...DEFAULT_SSH_POOL_CONFIG, ...poolConfig };
    this.logger = new Logger({ level: 'info' });
  }

  private createConnectionId(host: string, port: number, username: string): string {
    return `${username}@${host}:${port}`;
  }

  private async loadPrivateKey(keyPath: string): Promise<Buffer> {
    try {
      return readFileSync(keyPath);
    } catch (error) {
      throw new Error(`Failed to load SSH private key from ${keyPath}: ${error}`);
    }
  }

  async addConnection(nodeConfig: NodeConfig, sshKeyPath: string): Promise<string> {
    const connectionId = this.createConnectionId(
      nodeConfig.host,
      nodeConfig.port,
      nodeConfig.user
    );

    if (this.connections.has(connectionId)) {
      this.logger.warn(`Connection ${connectionId} already exists`);
      return connectionId;
    }

    if (this.connections.size >= this.poolConfig.maxConnections) {
      throw new Error(`Maximum connections (${this.poolConfig.maxConnections}) reached`);
    }

    const privateKey = await this.loadPrivateKey(sshKeyPath);
    
    const sshConfig: SSHConnectionConfig = {
      host: nodeConfig.host,
      port: nodeConfig.port,
      username: nodeConfig.user,
      privateKey,
      timeout: this.poolConfig.connectionTimeout,
      keepaliveInterval: this.poolConfig.keepAliveInterval,
      keepaliveCountMax: this.poolConfig.keepAliveCountMax,
    };

    const connection = new SSHConnection(sshConfig, this.poolConfig);
    
    // Forward connection events
    connection.on('event', (eventData: SSHConnectionEventData) => {
      this.emit('connectionEvent', eventData);
    });

    this.connections.set(connectionId, connection);
    this.logger.info(`Added SSH connection: ${connectionId}`);
    
    return connectionId;
  }

  async connect(connectionId: string): Promise<void> {
    const connection = this.connections.get(connectionId);
    if (!connection) {
      throw new Error(`Connection ${connectionId} not found`);
    }

    if (connection.isConnected()) {
      this.logger.debug(`Connection ${connectionId} already connected`);
      return;
    }

    let retryCount = 0;
    while (retryCount < this.poolConfig.retryAttempts) {
      try {
        await connection.connect();
        this.logger.info(`Connected to ${connectionId}`);
        return;
      } catch (error) {
        retryCount++;
        this.logger.warn(`Connection attempt ${retryCount} failed for ${connectionId}:`, { error: String(error) });
        
        if (retryCount < this.poolConfig.retryAttempts) {
          await new Promise(resolve => setTimeout(resolve, this.poolConfig.retryDelay));
        }
      }
    }

    const hostPort = connectionId.split('@')[1];
    const host = hostPort?.split(':')[0] || 'unknown';
    const port = parseInt(hostPort?.split(':')[1] || '22') || 22;
    
    throw new SSHConnectionError(
      `Failed to connect after ${this.poolConfig.retryAttempts} attempts`,
      host,
      port
    );
  }

  async connectAll(): Promise<void> {
    const connectionPromises = Array.from(this.connections.keys()).map(
      connectionId => this.connect(connectionId).catch(error => {
        this.logger.error(`Failed to connect ${connectionId}:`, error);
        return error;
      })
    );

    const results = await Promise.allSettled(connectionPromises);
    const failures = results.filter(result => result.status === 'rejected');
    
    if (failures.length > 0) {
      this.logger.error(`${failures.length} connections failed to establish`);
    }
  }

  async executeCommand(
    connectionId: string, 
    command: string, 
    options: SSHCommandOptions = {}
  ): Promise<SSHCommandResult> {
    const connection = this.connections.get(connectionId);
    if (!connection) {
      throw new Error(`Connection ${connectionId} not found`);
    }

    if (!connection.isConnected()) {
      await this.connect(connectionId);
    }

    this.logger.debug(`Executing command on ${connectionId}: ${command}`);
    return connection.executeCommand(command, options);
  }

  async disconnect(connectionId: string): Promise<void> {
    const connection = this.connections.get(connectionId);
    if (!connection) {
      throw new Error(`Connection ${connectionId} not found`);
    }

    await connection.disconnect();
    this.logger.info(`Disconnected from ${connectionId}`);
  }

  async disconnectAll(): Promise<void> {
    const disconnectPromises = Array.from(this.connections.values()).map(
      connection => connection.disconnect()
    );

    await Promise.allSettled(disconnectPromises);
    this.logger.info('All SSH connections closed');
  }

  getConnectionStatus(connectionId: string): SSHConnectionStatus | undefined {
    const connection = this.connections.get(connectionId);
    return connection?.getStatus();
  }

  getAllConnectionStatuses(): SSHConnectionStatus[] {
    return Array.from(this.connections.values()).map(conn => conn.getStatus());
  }

  removeConnection(connectionId: string): boolean {
    const connection = this.connections.get(connectionId);
    if (!connection) {
      return false;
    }

    connection.disconnect();
    this.connections.delete(connectionId);
    this.logger.info(`Removed SSH connection: ${connectionId}`);
    return true;
  }

  getConnectionCount(): number {
    return this.connections.size;
  }

  getConnectedCount(): number {
    return Array.from(this.connections.values())
      .filter(conn => conn.isConnected()).length;
  }
}