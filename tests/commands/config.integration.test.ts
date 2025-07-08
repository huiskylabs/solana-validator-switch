import { ConfigManager } from '../../src/utils/config-manager.js';
import { Validator } from '../../src/utils/validator.js';
import type { Config } from '../../src/types/config.js';
import fs from 'fs/promises';
import path from 'path';
import os from 'os';

// Integration tests for configuration management commands
describe('Configuration Integration Tests', () => {
  let tempDir: string;
  let configPath: string;
  let configManager: ConfigManager;

  const validConfig: Config = {
    version: '1.0.0',
    ssh: {
      keyPath: '/home/user/.ssh/id_ed25519',
      timeout: 30,
    },
    nodes: {
      primary: {
        label: 'primary validator',
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
        label: 'backup validator',
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

  beforeEach(async () => {
    // Create temporary directory for test configs
    tempDir = await fs.mkdtemp(path.join(os.tmpdir(), 'svs-test-'));
    configPath = path.join(tempDir, 'config.json');
    configManager = new ConfigManager(configPath);
  });

  afterEach(async () => {
    // Clean up temporary directory
    try {
      await fs.rm(tempDir, { recursive: true, force: true });
    } catch (error) {
      // Ignore cleanup errors in tests
    }
  });

  describe('Full Configuration Lifecycle', () => {
    it('should create, save, load, and validate configuration', async () => {
      // Initial state - no config file exists
      const initialExists = await configManager.exists();
      expect(initialExists).toBe(false);

      // Save initial configuration
      await configManager.save(validConfig);
      
      // Verify file was created
      const afterSaveExists = await configManager.exists();
      expect(afterSaveExists).toBe(true);

      // Load configuration back
      const loadedConfig = await configManager.load();
      
      // Verify loaded configuration matches saved configuration
      expect(loadedConfig.version).toBe(validConfig.version);
      expect(loadedConfig.ssh.keyPath).toBe(validConfig.ssh.keyPath);
      expect(loadedConfig.nodes.primary.host).toBe(validConfig.nodes.primary.host);
      expect(loadedConfig.nodes.backup.host).toBe(validConfig.nodes.backup.host);

      // Validate the loaded configuration
      const validationResult = Validator.validateConfig(loadedConfig);
      expect(validationResult.valid).toBe(true);
      expect(validationResult.errors).toHaveLength(0);
    });

    it('should handle configuration updates correctly', async () => {
      // Save initial configuration
      await configManager.save(validConfig);
      
      // Load and update SSH timeout
      await configManager.load();
      const updatedConfig = await configManager.update({
        ssh: {
          keyPath: '/home/user/.ssh/id_rsa',
          timeout: 60,
        },
      });

      // Verify update was applied
      expect(updatedConfig.ssh.keyPath).toBe('/home/user/.ssh/id_rsa');
      expect(updatedConfig.ssh.timeout).toBe(60);
      
      // Verify other fields were preserved
      expect(updatedConfig.nodes.primary.host).toBe(validConfig.nodes.primary.host);
      expect(updatedConfig.rpc.endpoint).toBe(validConfig.rpc.endpoint);

      // Reload from disk and verify persistence
      const reloadedConfig = await configManager.load();
      expect(reloadedConfig.ssh.timeout).toBe(60);
    });

    it('should handle configuration migration', async () => {
      // Create old format configuration
      const oldConfig = {
        ...validConfig,
        version: '0.9.0',
      };

      // Write old config directly to file
      await fs.writeFile(configPath, JSON.stringify(oldConfig, null, 2));

      // Load should trigger migration
      const migratedConfig = await configManager.load();
      
      expect(migratedConfig.version).toBe('1.0.0');
      expect(migratedConfig.ssh).toBeDefined();
      expect(migratedConfig.nodes).toBeDefined();
    });

    it('should create proper file permissions', async () => {
      await configManager.save(validConfig);
      
      const stats = await fs.stat(configPath);
      const permissions = stats.mode & parseInt('777', 8);
      
      // Should be readable/writable by owner only (600)
      expect(permissions).toBe(parseInt('600', 8));
    });

    it('should handle atomic writes correctly', async () => {
      await configManager.save(validConfig);
      
      // Verify no temporary files remain
      const dirContents = await fs.readdir(tempDir);
      expect(dirContents).toEqual(['config.json']);
      expect(dirContents).not.toContain('config.json.tmp');
    });
  });

  describe('Configuration Validation Integration', () => {
    it('should prevent saving invalid configurations', async () => {
      const invalidConfig = {
        version: '1.0.0',
        // Missing required ssh and nodes fields
      } as any;

      await expect(configManager.save(invalidConfig)).rejects.toThrow();
      
      // Verify no file was created
      const exists = await configManager.exists();
      expect(exists).toBe(false);
    });

    it('should validate all required node fields', async () => {
      const configWithIncompleteNode = {
        ...validConfig,
        nodes: {
          primary: {
            label: 'primary',
            host: '192.168.1.10',
            // Missing port, user, and paths
          },
          backup: validConfig.nodes.backup,
        },
      } as any;

      const validationResult = Validator.validateConfig(configWithIncompleteNode);
      
      expect(validationResult.valid).toBe(false);
      expect(validationResult.errors.some(e => e.field.includes('port'))).toBe(true);
      expect(validationResult.errors.some(e => e.field.includes('user'))).toBe(true);
      expect(validationResult.errors.some(e => e.field.includes('paths'))).toBe(true);
    });

    it('should validate SSH configuration properly', async () => {
      const configWithInvalidSSH = {
        ...validConfig,
        ssh: {
          keyPath: '', // Empty path should be invalid
          timeout: 2, // Too low timeout
        },
      };

      const validationResult = Validator.validateConfig(configWithInvalidSSH);
      
      expect(validationResult.valid).toBe(false);
      expect(validationResult.errors.some(e => e.field === 'ssh.keyPath')).toBe(true);
      expect(validationResult.errors.some(e => e.field === 'ssh.timeout')).toBe(true);
    });

    it('should validate vote keypair requirement', async () => {
      const configWithoutVoteKeypair = {
        ...validConfig,
        nodes: {
          primary: {
            ...validConfig.nodes.primary,
            paths: {
              ...validConfig.nodes.primary.paths,
              voteKeypair: '', // Missing vote keypair
            },
          },
          backup: validConfig.nodes.backup,
        },
      };

      const validationResult = Validator.validateConfig(configWithoutVoteKeypair);
      
      expect(validationResult.valid).toBe(false);
      expect(validationResult.errors.some(e => 
        e.field.includes('voteKeypair') && e.message.includes('missing')
      )).toBe(true);
    });
  });

  describe('Configuration Status and Export', () => {
    it('should provide accurate configuration status', async () => {
      // No config initially
      let status = await configManager.getStatus();
      expect(status.valid).toBe(false);
      expect(status.version).toBe('none');
      expect(status.errors).toContain('Configuration file does not exist');

      // Save valid config
      await configManager.save(validConfig);
      
      status = await configManager.getStatus();
      expect(status.valid).toBe(true);
      expect(status.version).toBe('1.0.0');
      expect(status.errors).toHaveLength(0);
      expect(status.migrationNeeded).toBe(false);
    });

    it('should export and import configuration correctly', async () => {
      await configManager.save(validConfig);
      await configManager.load();
      
      // Export configuration
      const exported = configManager.export();
      const exportedData = JSON.parse(exported);
      
      expect(exportedData.version).toBe('1.0.0');
      expect(exportedData.ssh.keyPath).toBe('/home/user/.ssh/id_ed25519');

      // Create new config manager and import
      const newConfigPath = path.join(tempDir, 'imported.json');
      const newConfigManager = new ConfigManager(newConfigPath);
      
      const importedConfig = await newConfigManager.import(exported);
      
      expect(importedConfig.version).toBe(validConfig.version);
      expect(importedConfig.ssh.keyPath).toBe(validConfig.ssh.keyPath);
      expect(importedConfig.nodes.primary.host).toBe(validConfig.nodes.primary.host);
    });

    it('should create and restore backups', async () => {
      await configManager.save(validConfig);
      
      // Create backup
      const backupPath = await configManager.backup();
      expect(backupPath).toMatch(/\\.backup\\./);
      
      // Verify backup file exists
      const backupExists = await fs.access(backupPath).then(() => true).catch(() => false);
      expect(backupExists).toBe(true);
      
      // Verify backup content matches original
      const backupContent = await fs.readFile(backupPath, 'utf-8');
      const originalContent = await fs.readFile(configPath, 'utf-8');
      expect(backupContent).toBe(originalContent);
    });
  });

  describe('Environment Variable Integration', () => {
    const originalEnv = process.env;

    beforeEach(() => {
      process.env = { ...originalEnv };
    });

    afterEach(() => {
      process.env = originalEnv;
    });

    it('should apply environment variable overrides', async () => {
      process.env.SVS_RPC_ENDPOINT = 'https://custom-rpc.example.com';
      process.env.SVS_SSH_TIMEOUT = '45';
      process.env.SVS_THEME = 'light';
      
      const overrides = configManager.loadEnvironmentOverrides();
      
      expect(overrides.rpc?.endpoint).toBe('https://custom-rpc.example.com');
      expect(overrides.security?.sshTimeout).toBe(45);
      expect(overrides.display?.theme).toBe('light');
    });

    it('should handle invalid environment variables gracefully', async () => {
      process.env.SVS_SSH_TIMEOUT = 'not-a-number';
      process.env.SVS_REFRESH_INTERVAL = 'invalid';
      
      const overrides = configManager.loadEnvironmentOverrides();
      
      // Should not include invalid numeric values
      expect(overrides.security?.sshTimeout).toBeUndefined();
      expect(overrides.monitoring?.interval).toBeUndefined();
    });
  });

  describe('Error Handling and Recovery', () => {
    it('should handle corrupted configuration files', async () => {
      // Write invalid JSON to config file
      await fs.writeFile(configPath, 'invalid json content');
      
      await expect(configManager.load()).rejects.toThrow();
    });

    it('should handle permission errors gracefully', async () => {
      // Create config file
      await configManager.save(validConfig);
      
      // Change permissions to make it unreadable (on Unix systems)
      if (process.platform !== 'win32') {
        await fs.chmod(configPath, 0o000);
        
        await expect(configManager.load()).rejects.toThrow();
        
        // Restore permissions for cleanup
        await fs.chmod(configPath, 0o600);
      }
    });

    it('should handle disk space issues during save', async () => {
      // This is difficult to test directly, but we can test that
      // atomic writes are properly handled
      await configManager.save(validConfig);
      
      // Verify the main config file exists and temp file doesn't
      const configExists = await fs.access(configPath).then(() => true).catch(() => false);
      const tempExists = await fs.access(`${configPath}.tmp`).then(() => true).catch(() => false);
      
      expect(configExists).toBe(true);
      expect(tempExists).toBe(false);
    });
  });

  describe('Real-world Configuration Scenarios', () => {
    it('should handle typical mainnet configuration', async () => {
      const mainnetConfig: Config = {
        ...validConfig,
        rpc: {
          endpoint: 'https://api.mainnet-beta.solana.com',
          timeout: 30000,
          retries: 3,
        },
        monitoring: {
          interval: 2000, // 2 seconds for mainnet
          healthThreshold: 10,
          readinessThreshold: 50,
          enableMetrics: true,
          metricsRetention: 30, // 30 days for mainnet
        },
      };

      await configManager.save(mainnetConfig);
      const loaded = await configManager.load();
      const validation = Validator.validateConfig(loaded);
      
      expect(validation.valid).toBe(true);
      expect(loaded.rpc.endpoint).toBe('https://api.mainnet-beta.solana.com');
      expect(loaded.monitoring.interval).toBe(2000);
    });

    it('should handle testnet configuration', async () => {
      const testnetConfig: Config = {
        ...validConfig,
        rpc: {
          endpoint: 'https://api.testnet.solana.com',
          timeout: 15000,
          retries: 5,
        },
        monitoring: {
          interval: 5000, // 5 seconds for testnet
          healthThreshold: 100,
          readinessThreshold: 100,
          enableMetrics: true,
          metricsRetention: 7, // 7 days for testnet
        },
      };

      await configManager.save(testnetConfig);
      const loaded = await configManager.load();
      const validation = Validator.validateConfig(loaded);
      
      expect(validation.valid).toBe(true);
      expect(loaded.rpc.endpoint).toBe('https://api.testnet.solana.com');
      expect(loaded.monitoring.interval).toBe(5000);
    });

    it('should handle high-security configuration', async () => {
      const secureConfig: Config = {
        ...validConfig,
        security: {
          confirmSwitches: true,
          maxRetries: 1, // Very conservative
        },
        ssh: {
          keyPath: '/home/user/.ssh/id_ed25519', // Prefer ed25519
          timeout: 10, // Short timeout
        },
      };

      await configManager.save(secureConfig);
      const loaded = await configManager.load();
      const validation = Validator.validateConfig(loaded);
      
      expect(validation.valid).toBe(true);
      expect(loaded.security.confirmSwitches).toBe(true);
      expect(loaded.security.maxRetries).toBe(1);
      expect(loaded.ssh.timeout).toBe(10);
    });
  });
});