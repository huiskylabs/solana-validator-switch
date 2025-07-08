import fs from 'fs/promises';
import path from 'path';
import os from 'os';
import { ConfigManager } from '../../src/utils/config-manager.js';
import type { Config, PartialConfig } from '../../src/types/config.js';

// Mock fs operations for testing
jest.mock('fs/promises');
const mockFs = fs as jest.Mocked<typeof fs>;

// Mock logger to avoid console output during tests
jest.mock('../../src/utils/logger.js', () => ({
  Logger: jest.fn().mockImplementation(() => ({
    info: jest.fn(),
    success: jest.fn(),
    error: jest.fn(),
    warn: jest.fn(),
  })),
}));

// Mock error handler
jest.mock('../../src/utils/error-handler.js', () => ({
  ErrorHandler: jest.fn().mockImplementation(() => ({
    handle: jest.fn(),
  })),
}));

describe('ConfigManager', () => {
  let configManager: ConfigManager;
  let tempConfigPath: string;

  const validConfig: Config = {
    version: '1.0.0',
    ssh: {
      keyPath: '/home/user/.ssh/id_ed25519',
      timeout: 30,
    },
    nodes: {
      primary: {
        label: 'primary',
        host: '192.168.1.10',
        port: 22,
        user: 'solana',
        paths: {
          fundedIdentity: '/home/solana/funded-validator-keypair.json',
          unfundedIdentity: '/home/solana/unfunded-validator-keypair.json',
          voteKeypair: '/home/solana/vote-account-keypair.json',
          ledger: '/mnt/ledger',
          tower: '/mnt/ledger/tower-1_9-*.bin',
          solanaCliPath: '/home/solana/.local/share/solana/install/active_release/bin/solana',
        },
      },
      backup: {
        label: 'backup',
        host: '192.168.1.11',
        port: 22,
        user: 'solana',
        paths: {
          fundedIdentity: '/home/solana/funded-validator-keypair.json',
          unfundedIdentity: '/home/solana/unfunded-validator-keypair.json',
          voteKeypair: '/home/solana/vote-account-keypair.json',
          ledger: '/mnt/ledger',
          tower: '/mnt/ledger/tower-1_9-*.bin',
          solanaCliPath: '/home/solana/.local/share/solana/install/active_release/bin/solana',
        },
      },
    },
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

  beforeEach(() => {
    tempConfigPath = path.join(os.tmpdir(), `test-config-${Date.now()}.json`);
    configManager = new ConfigManager(tempConfigPath);
    
    // Reset all mocks
    jest.clearAllMocks();
    
    // Default mock implementations
    mockFs.access.mockResolvedValue(undefined);
    mockFs.readFile.mockResolvedValue(JSON.stringify(validConfig));
    mockFs.writeFile.mockResolvedValue(undefined);
    mockFs.rename.mockResolvedValue(undefined);
    mockFs.chmod.mockResolvedValue(undefined);
    mockFs.mkdir.mockResolvedValue(undefined);
    mockFs.stat.mockResolvedValue({
      mtime: new Date(),
    } as any);
  });

  describe('load', () => {
    it('should load valid configuration from file', async () => {
      const config = await configManager.load();
      
      expect(config).toEqual({
        ...validConfig,
        configPath: tempConfigPath,
      });
      expect(mockFs.readFile).toHaveBeenCalledWith(tempConfigPath, 'utf-8');
    });

    it('should create default configuration when file does not exist', async () => {
      mockFs.access.mockRejectedValue(new Error('File not found'));
      
      const config = await configManager.load();
      
      expect(config.version).toBe('1.0.0');
      expect(config.ssh).toBeDefined();
      expect(config.nodes).toBeDefined();
      expect(mockFs.writeFile).toHaveBeenCalled();
    });

    it('should handle invalid JSON in config file', async () => {
      mockFs.readFile.mockResolvedValue('invalid json');
      
      await expect(configManager.load()).rejects.toThrow();
    });

    it('should migrate old configuration versions', async () => {
      const oldConfig = { ...validConfig, version: '0.9.0' };
      mockFs.readFile.mockResolvedValue(JSON.stringify(oldConfig));
      
      const config = await configManager.load();
      
      expect(config.version).toBe('1.0.0');
    });

    it('should validate configuration structure', async () => {
      const invalidConfig = { version: '1.0.0' }; // Missing required fields
      mockFs.readFile.mockResolvedValue(JSON.stringify(invalidConfig));
      
      await expect(configManager.load()).rejects.toThrow();
    });
  });

  describe('save', () => {
    beforeEach(async () => {
      // Load config first to simulate normal usage
      await configManager.load();
    });

    it('should save configuration to file with atomic write', async () => {
      await configManager.save(validConfig);
      
      expect(mockFs.writeFile).toHaveBeenCalledWith(
        `${tempConfigPath}.tmp`,
        expect.stringContaining('"version": "1.0.0"'),
        'utf-8'
      );
      expect(mockFs.rename).toHaveBeenCalledWith(
        `${tempConfigPath}.tmp`,
        tempConfigPath
      );
      expect(mockFs.chmod).toHaveBeenCalledWith(tempConfigPath, 0o600);
    });

    it('should validate configuration before saving', async () => {
      const invalidConfig = { version: '1.0.0' } as any;
      
      await expect(configManager.save(invalidConfig)).rejects.toThrow();
    });

    it('should create config directory if it does not exist', async () => {
      await configManager.save(validConfig);
      
      expect(mockFs.mkdir).toHaveBeenCalledWith(
        path.dirname(tempConfigPath),
        { recursive: true }
      );
    });

    it('should handle partial configuration updates', async () => {
      const partialUpdate: PartialConfig = {
        rpc: {
          endpoint: 'https://api.testnet.solana.com',
          timeout: 15000,
          retries: 2,
        },
      };
      
      await configManager.save(partialUpdate);
      
      // Should merge with existing config
      expect(mockFs.writeFile).toHaveBeenCalledWith(
        expect.any(String),
        expect.stringContaining('api.testnet.solana.com'),
        'utf-8'
      );
    });

    it('should save new configuration when no current config exists', async () => {
      const newConfigManager = new ConfigManager('/tmp/new-config.json');
      
      await newConfigManager.save(validConfig);
      
      expect(mockFs.writeFile).toHaveBeenCalled();
    });
  });

  describe('update', () => {
    beforeEach(async () => {
      await configManager.load();
    });

    it('should update configuration partially', async () => {
      const updates: PartialConfig = {
        display: {
          theme: 'light',
          compact: false,
          showTechnicalDetails: true,
        },
      };
      
      const updatedConfig = await configManager.update(updates);
      
      expect(updatedConfig.display?.theme).toBe('light');
      expect(updatedConfig.display?.compact).toBe(false);
      expect(updatedConfig.nodes.primary.host).toBe('192.168.1.10'); // Should preserve existing
    });

    it('should throw error when no configuration is loaded', async () => {
      const newConfigManager = new ConfigManager('/tmp/empty-config.json');
      
      await expect(newConfigManager.update({ version: '1.0.0' })).rejects.toThrow(
        'No configuration loaded'
      );
    });
  });

  describe('getStatus', () => {
    it('should return valid status for existing valid config', async () => {
      const status = await configManager.getStatus();
      
      expect(status.valid).toBe(true);
      expect(status.version).toBe('1.0.0');
      expect(status.path).toBe(tempConfigPath);
      expect(status.errors).toHaveLength(0);
      expect(status.migrationNeeded).toBe(false);
    });

    it('should return invalid status when config file does not exist', async () => {
      mockFs.access.mockRejectedValue(new Error('File not found'));
      
      const status = await configManager.getStatus();
      
      expect(status.valid).toBe(false);
      expect(status.version).toBe('none');
      expect(status.errors).toContain('Configuration file does not exist');
    });

    it('should detect migration needed for old versions', async () => {
      const oldConfig = { ...validConfig, version: '0.9.0' };
      mockFs.readFile.mockResolvedValue(JSON.stringify(oldConfig));
      
      const status = await configManager.getStatus();
      
      expect(status.migrationNeeded).toBe(true);
      expect(status.version).toBe('0.9.0');
    });

    it('should handle invalid configuration gracefully', async () => {
      mockFs.readFile.mockResolvedValue('invalid json');
      
      const status = await configManager.getStatus();
      
      expect(status.valid).toBe(false);
      expect(status.errors.length).toBeGreaterThan(0);
    });
  });

  describe('export', () => {
    beforeEach(async () => {
      await configManager.load();
    });

    it('should export configuration as JSON string', () => {
      const exported = configManager.export();
      const parsed = JSON.parse(exported);
      
      expect(parsed.version).toBe('1.0.0');
      expect(parsed.nodes.primary.host).toBe('192.168.1.10');
    });

    it('should export with pretty formatting by default', () => {
      const exported = configManager.export();
      
      expect(exported).toContain('\\n'); // Should have newlines for pretty formatting
    });

    it('should export without pretty formatting when disabled', () => {
      const exported = configManager.export(false);
      
      expect(exported).not.toContain('\\n'); // Should be minified
    });

    it('should throw error when no configuration is loaded', () => {
      const newConfigManager = new ConfigManager('/tmp/empty-config.json');
      
      expect(() => newConfigManager.export()).toThrow('No configuration loaded');
    });
  });

  describe('import', () => {
    it('should import valid configuration from JSON string', async () => {
      const configJson = JSON.stringify(validConfig);
      
      const importedConfig = await configManager.import(configJson);
      
      expect(importedConfig.version).toBe('1.0.0');
      expect(importedConfig.nodes.primary.host).toBe('192.168.1.10');
    });

    it('should reject invalid JSON', async () => {
      await expect(configManager.import('invalid json')).rejects.toThrow();
    });

    it('should reject invalid configuration structure', async () => {
      const invalidConfig = { version: '1.0.0' }; // Missing required fields
      
      await expect(
        configManager.import(JSON.stringify(invalidConfig))
      ).rejects.toThrow();
    });
  });

  describe('reset', () => {
    beforeEach(async () => {
      await configManager.load();
    });

    it('should reset configuration to defaults', async () => {
      const resetConfig = await configManager.reset();
      
      expect(resetConfig.version).toBe('1.0.0');
      expect(resetConfig.ssh).toBeDefined();
      expect(resetConfig.nodes).toBeDefined();
      expect(mockFs.writeFile).toHaveBeenCalled();
    });
  });

  describe('backup', () => {
    it('should create backup of existing configuration', async () => {
      mockFs.copyFile.mockResolvedValue(undefined);
      
      const backupPath = await configManager.backup();
      
      expect(backupPath).toMatch(/\\.backup\\./);
      expect(mockFs.copyFile).toHaveBeenCalledWith(tempConfigPath, backupPath);
    });

    it('should throw error when no configuration file exists', async () => {
      mockFs.access.mockRejectedValue(new Error('File not found'));
      
      await expect(configManager.backup()).rejects.toThrow(
        'No configuration file to backup'
      );
    });
  });

  describe('loadEnvironmentOverrides', () => {
    const originalEnv = process.env;

    beforeEach(() => {
      process.env = { ...originalEnv };
    });

    afterEach(() => {
      process.env = originalEnv;
    });

    it('should load RPC endpoint override from environment', () => {
      process.env.SVS_RPC_ENDPOINT = 'https://custom-rpc.example.com';
      
      const overrides = configManager.loadEnvironmentOverrides();
      
      expect(overrides.rpc?.endpoint).toBe('https://custom-rpc.example.com');
    });

    it('should load log level override from environment', () => {
      process.env.SVS_LOG_LEVEL = 'debug';
      
      const overrides = configManager.loadEnvironmentOverrides();
      
      expect(overrides.advanced?.performance?.logLevel).toBe('debug');
    });

    it('should load theme override from environment', () => {
      process.env.SVS_THEME = 'light';
      process.env.SVS_COMPACT_MODE = 'true';
      
      const overrides = configManager.loadEnvironmentOverrides();
      
      expect(overrides.display?.theme).toBe('light');
      expect(overrides.display?.compact).toBe(true);
    });

    it('should load SSH timeout override from environment', () => {
      process.env.SVS_SSH_TIMEOUT = '60';
      
      const overrides = configManager.loadEnvironmentOverrides();
      
      expect(overrides.security?.sshTimeout).toBe(60);
    });

    it('should load refresh interval override from environment', () => {
      process.env.SVS_REFRESH_INTERVAL = '10';
      
      const overrides = configManager.loadEnvironmentOverrides();
      
      expect(overrides.monitoring?.interval).toBe(10000); // Converted to ms
    });

    it('should ignore invalid numeric environment variables', () => {
      process.env.SVS_SSH_TIMEOUT = 'invalid';
      
      const overrides = configManager.loadEnvironmentOverrides();
      
      expect(overrides.security?.sshTimeout).toBeUndefined();
    });
  });

  describe('getConfigPath', () => {
    it('should return the configured path', () => {
      const path = configManager.getConfigPath();
      
      expect(path).toBe(tempConfigPath);
    });
  });

  describe('exists', () => {
    it('should return true when config file exists', async () => {
      const exists = await configManager.exists();
      
      expect(exists).toBe(true);
      expect(mockFs.access).toHaveBeenCalledWith(tempConfigPath);
    });

    it('should return false when config file does not exist', async () => {
      mockFs.access.mockRejectedValue(new Error('File not found'));
      
      const exists = await configManager.exists();
      
      expect(exists).toBe(false);
    });
  });

  describe('error handling', () => {
    it('should handle file system errors gracefully', async () => {
      mockFs.readFile.mockRejectedValue(new Error('Permission denied'));
      
      await expect(configManager.load()).rejects.toThrow();
    });

    it('should handle write errors during save', async () => {
      await configManager.load();
      mockFs.writeFile.mockRejectedValue(new Error('Disk full'));
      
      await expect(configManager.save(validConfig)).rejects.toThrow();
    });

    it('should clean up temporary files on write failure', async () => {
      await configManager.load();
      mockFs.rename.mockRejectedValue(new Error('Rename failed'));
      
      await expect(configManager.save(validConfig)).rejects.toThrow();
    });
  });
});