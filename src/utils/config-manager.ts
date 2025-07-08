import fs from 'fs/promises';
import path from 'path';
import os from 'os';
import type {
  Config,
  PartialConfig,
  ConfigStatus,
  EnvironmentConfig,
} from '../types/config.js';
import { DEFAULT_CONFIG, CONFIG_PATHS } from '../types/config.js';
import { Logger } from './logger.js';
import { ErrorHandler } from './error-handler.js';

export class ConfigManager {
  private config: Config | null = null;
  private configPath: string;
  private logger: Logger;
  private errorHandler: ErrorHandler;

  constructor(configPath?: string) {
    this.logger = new Logger();
    this.errorHandler = new ErrorHandler(this.logger);
    this.configPath = this.resolveConfigPath(configPath);
  }

  /**
   * Load configuration from file with validation and migration
   */
  async load(): Promise<Config> {
    try {
      // Check if config file exists
      if (!(await this.exists())) {
        this.logger.info(
          'No configuration file found, creating default configuration'
        );
        return this.createDefault();
      }

      // Read and parse configuration file
      const configData = await fs.readFile(this.configPath, 'utf-8');
      const parsedConfig = JSON.parse(configData) as unknown;

      // Validate configuration structure
      if (!this.isValidConfig(parsedConfig)) {
        throw new Error('Invalid configuration structure');
      }

      let config = parsedConfig as Config;

      // Check if migration is needed
      const currentVersion = '1.0.0';
      if (config.version !== currentVersion) {
        this.logger.info(
          `Migrating configuration from v${config.version} to v${currentVersion}`
        );
        config = await this.migrateConfig(config);
      }

      // Merge with defaults to ensure all properties exist
      config = this.mergeWithDefaults(config);

      // Validate final configuration
      const validation = this.validateConfig(config);
      if (!validation.valid) {
        throw new Error(
          `Configuration validation failed: ${validation.errors.join(', ')}`
        );
      }

      // Set resolved config path
      config.configPath = this.configPath;

      this.config = config;
      this.logger.success(`Configuration loaded from ${this.configPath}`);

      return config;
    } catch (error) {
      this.errorHandler.handle(error);
      throw error;
    }
  }

  /**
   * Save configuration to file with atomic write
   */
  async save(config?: PartialConfig): Promise<void> {
    try {
      const configToSave = config ? this.mergeWithCurrent(config) : this.config;

      if (!configToSave) {
        throw new Error('No configuration to save');
      }

      // Validate before saving
      const validation = this.validateConfig(configToSave);
      if (!validation.valid) {
        throw new Error(
          `Cannot save invalid configuration: ${validation.errors.join(', ')}`
        );
      }

      // Ensure directory exists
      await this.ensureConfigDirectory();

      // Atomic write using temporary file
      const tempPath = `${this.configPath}.tmp`;
      const configJson = JSON.stringify(configToSave, null, 2);

      await fs.writeFile(tempPath, configJson, 'utf-8');
      await fs.rename(tempPath, this.configPath);

      // Set secure permissions (readable/writable by owner only)
      await fs.chmod(this.configPath, 0o600);

      this.config = configToSave;
      this.logger.success(`Configuration saved to ${this.configPath}`);
    } catch (error) {
      this.errorHandler.handle(error);
      throw error;
    }
  }

  /**
   * Get current configuration
   */
  get(): Config | null {
    return this.config;
  }

  /**
   * Update configuration partially
   */
  async update(updates: PartialConfig): Promise<Config> {
    if (!this.config) {
      throw new Error('No configuration loaded');
    }

    const updatedConfig = this.deepMerge(this.config, updates);
    await this.save(updatedConfig);
    return updatedConfig;
  }

  /**
   * Get configuration status
   */
  async getStatus(): Promise<ConfigStatus> {
    try {
      const exists = await this.exists();
      if (!exists) {
        return {
          valid: false,
          version: 'none',
          path: this.configPath,
          lastModified: new Date(0),
          errors: ['Configuration file does not exist'],
          warnings: [],
          migrationNeeded: false,
        };
      }

      const stat = await fs.stat(this.configPath);
      const configData = await fs.readFile(this.configPath, 'utf-8');
      const parsedConfig = JSON.parse(configData) as unknown;

      const validation = this.validateConfig(parsedConfig);
      const version = this.isValidConfig(parsedConfig)
        ? (parsedConfig as Config).version
        : 'unknown';

      return {
        valid: validation.valid,
        version,
        path: this.configPath,
        lastModified: stat.mtime,
        errors: validation.errors,
        warnings: validation.warnings,
        migrationNeeded: version !== '1.0.0',
      };
    } catch (error) {
      return {
        valid: false,
        version: 'unknown',
        path: this.configPath,
        lastModified: new Date(0),
        errors: [error instanceof Error ? error.message : 'Unknown error'],
        warnings: [],
        migrationNeeded: false,
      };
    }
  }

  /**
   * Export configuration to JSON string
   */
  export(pretty = true): string {
    if (!this.config) {
      throw new Error('No configuration loaded');
    }

    return JSON.stringify(this.config, null, pretty ? 2 : 0);
  }

  /**
   * Import configuration from JSON string
   */
  async import(configJson: string): Promise<Config> {
    try {
      const parsedConfig = JSON.parse(configJson) as unknown;

      if (!this.isValidConfig(parsedConfig)) {
        throw new Error('Invalid configuration format');
      }

      const config = parsedConfig as Config;
      const validation = this.validateConfig(config);

      if (!validation.valid) {
        throw new Error(
          `Invalid configuration: ${validation.errors.join(', ')}`
        );
      }

      await this.save(config);
      return config;
    } catch (error) {
      this.errorHandler.handle(error);
      throw error;
    }
  }

  /**
   * Reset configuration to defaults
   */
  async reset(): Promise<Config> {
    this.logger.info('Resetting configuration to defaults');
    const defaultConfig = this.createDefaultConfig();
    await this.save(defaultConfig);
    return defaultConfig;
  }

  /**
   * Backup current configuration
   */
  async backup(): Promise<string> {
    if (!(await this.exists())) {
      throw new Error('No configuration file to backup');
    }

    const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
    const backupPath = `${this.configPath}.backup.${timestamp}`;

    await fs.copyFile(this.configPath, backupPath);
    this.logger.success(`Configuration backed up to ${backupPath}`);

    return backupPath;
  }

  /**
   * Load environment variables into configuration
   */
  loadEnvironmentOverrides(): PartialConfig {
    const env = process.env as EnvironmentConfig;
    const overrides: PartialConfig = {};

    // RPC endpoint override
    if (env.SVS_RPC_ENDPOINT) {
      overrides.rpc = {
        endpoint: env.SVS_RPC_ENDPOINT,
        timeout: 30000,
        retries: 3,
      };
    }

    // Logging configuration
    if (env.SVS_LOG_LEVEL) {
      overrides.advanced = {
        sshPoolSize: 2,
        sshKeepAlive: 30,
        commandTimeout: 30,
        towerBackups: 5,
        performance: {
          logLevel: env.SVS_LOG_LEVEL as 'debug' | 'info' | 'warn' | 'error',
          cacheSize: 100,
          maxLogFiles: 5,
          maxLogSize: 10,
          enableProfiling: false,
        },
      };
    }

    // Display settings
    if (env.SVS_THEME || env.SVS_NO_COLOR || env.SVS_COMPACT_MODE) {
      overrides.display = {
        theme: (env.SVS_THEME as 'dark' | 'light' | 'auto') || 'auto',
        compact: env.SVS_COMPACT_MODE === 'true',
        showTechnicalDetails: false,
        showTimestamps: true,
        showLogLevel: true,
        colorOutput: env.SVS_NO_COLOR !== 'true',
        refreshRate: 2,
      };
    }

    // SSH timeout
    if (env.SVS_SSH_TIMEOUT) {
      const timeout = parseInt(env.SVS_SSH_TIMEOUT, 10);
      if (!isNaN(timeout)) {
        overrides.security = {
          confirmSwitches: true,
          sessionTimeout: 60,
          maxRetries: 3,
          sshTimeout: timeout,
          requireHealthCheck: true,
          allowForceSwitch: false,
          auditLog: true,
        };
      }
    }

    // Refresh interval
    if (env.SVS_REFRESH_INTERVAL) {
      const interval = parseInt(env.SVS_REFRESH_INTERVAL, 10);
      if (!isNaN(interval)) {
        overrides.monitoring = {
          interval: interval * 1000, // Convert to ms
          healthThreshold: 10,
          readinessThreshold: 50,
          enableMetrics: true,
          metricsRetention: 7,
        };
      }
    }

    return overrides;
  }

  // Private helper methods

  private resolveConfigPath(providedPath?: string): string {
    if (providedPath) {
      return path.resolve(providedPath);
    }

    // Check environment variable
    const envPath = process.env.SVS_CONFIG_PATH;
    if (envPath) {
      return path.resolve(envPath);
    }

    // Default to user config path
    const userConfigPath = CONFIG_PATHS.userConfig.replace('~', os.homedir());
    return path.resolve(userConfigPath);
  }

  private async exists(): Promise<boolean> {
    try {
      await fs.access(this.configPath);
      return true;
    } catch {
      return false;
    }
  }

  private async ensureConfigDirectory(): Promise<void> {
    const configDir = path.dirname(this.configPath);
    await fs.mkdir(configDir, { recursive: true });
  }

  private async createDefault(): Promise<Config> {
    const defaultConfig = this.createDefaultConfig();
    await this.save(defaultConfig);
    return defaultConfig;
  }

  private createDefaultConfig(): Config {
    return {
      version: '1.0.0',
      configPath: this.configPath,
      nodes: {
        primary: {
          label: 'primary',
          host: '',
          port: 22,
          user: '',
          keyPath: '',
          paths: {
            fundedIdentity: '',
            unfundedIdentity: '',
            ledger: '',
            tower: '',
            solanaCliPath: '',
          },
        },
        backup: {
          label: 'backup',
          host: '',
          port: 22,
          user: '',
          keyPath: '',
          paths: {
            fundedIdentity: '',
            unfundedIdentity: '',
            ledger: '',
            tower: '',
            solanaCliPath: '',
          },
        },
      },
      ...DEFAULT_CONFIG,
    } as Config;
  }

  private isValidConfig(config: unknown): config is Config {
    return (
      typeof config === 'object' &&
      config !== null &&
      'version' in config &&
      'nodes' in config &&
      'rpc' in config
    );
  }

  private validateConfig(config: unknown): {
    valid: boolean;
    errors: string[];
    warnings: string[];
  } {
    const errors: string[] = [];
    const warnings: string[] = [];

    if (!this.isValidConfig(config)) {
      errors.push('Invalid configuration structure');
      return { valid: false, errors, warnings };
    }

    // Validate nodes
    if (!config.nodes.primary.host) {
      errors.push('Primary node host is required');
    }
    if (!config.nodes.backup.host) {
      errors.push('Backup node host is required');
    }

    // Validate RPC endpoint
    if (!config.rpc.endpoint) {
      errors.push('RPC endpoint is required');
    }

    // Add warnings for common issues
    if (config.nodes.primary.host === config.nodes.backup.host) {
      warnings.push('Primary and backup nodes have the same host');
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  private mergeWithDefaults(config: Config): Config {
    return this.deepMerge(this.createDefaultConfig(), config);
  }

  private mergeWithCurrent(updates: PartialConfig): Config {
    if (!this.config) {
      throw new Error('No current configuration to merge with');
    }
    return this.deepMerge(this.config, updates);
  }

  private deepMerge(target: any, source: any): any {
    const result = { ...target };

    for (const key in source) {
      if (
        source[key] &&
        typeof source[key] === 'object' &&
        !Array.isArray(source[key])
      ) {
        result[key] = this.deepMerge(target[key] || {}, source[key]);
      } else {
        result[key] = source[key];
      }
    }

    return result;
  }

  private async migrateConfig(config: Config): Promise<Config> {
    // Placeholder for future migrations
    // For now, just update version
    return {
      ...config,
      version: '1.0.0',
    };
  }
}
