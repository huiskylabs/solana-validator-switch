import { Validator } from '../../src/utils/validator.js';
import type { Config, SSHConfig, NodeConfig } from '../../src/types/config.js';

describe('Validator', () => {
  const validSSHConfig: SSHConfig = {
    keyPath: '/home/user/.ssh/id_ed25519',
    timeout: 30,
  };

  const validNodeConfig: NodeConfig = {
    label: 'test-node',
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
  };

  const validConfig: Config = {
    version: '1.0.0',
    ssh: validSSHConfig,
    nodes: {
      primary: { ...validNodeConfig, label: 'primary' },
      backup: { ...validNodeConfig, label: 'backup', host: '192.168.1.11' },
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

  describe('validateConfig', () => {
    it('should validate a complete valid configuration', () => {
      const result = Validator.validateConfig(validConfig);
      
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should reject invalid configuration structure', () => {
      const result = Validator.validateConfig(null);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'root',
          message: 'Configuration must be an object',
          code: 'INVALID_TYPE',
        })
      );
    });

    it('should reject configuration missing required fields', () => {
      const incompleteConfig = { version: '1.0.0' };
      const result = Validator.validateConfig(incompleteConfig);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'ssh',
          message: "Required field 'ssh' is missing",
          code: 'REQUIRED_FIELD_MISSING',
        })
      );
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'nodes',
          message: "Required field 'nodes' is missing",
          code: 'REQUIRED_FIELD_MISSING',
        })
      );
    });

    it('should validate SSH configuration', () => {
      const configWithInvalidSSH = {
        ...validConfig,
        ssh: { keyPath: '', timeout: 30 },
      };
      
      const result = Validator.validateConfig(configWithInvalidSSH);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'ssh.keyPath',
          message: 'SSH key path is required',
          code: 'REQUIRED_FIELD_MISSING',
        })
      );
    });

    it('should warn about relative SSH key paths', () => {
      const configWithRelativePath = {
        ...validConfig,
        ssh: { keyPath: 'relative/path/key', timeout: 30 },
      };
      
      const result = Validator.validateConfig(configWithRelativePath);
      
      expect(result.warnings).toContainEqual(
        expect.objectContaining({
          field: 'ssh.keyPath',
          message: 'SSH key path should be an absolute path',
          code: 'SUSPICIOUS_VALUE',
        })
      );
    });

    it('should validate SSH timeout ranges', () => {
      const configWithInvalidTimeout = {
        ...validConfig,
        ssh: { keyPath: '/path/to/key', timeout: 2 },
      };
      
      const result = Validator.validateConfig(configWithInvalidTimeout);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'ssh.timeout',
          message: 'SSH timeout must be between 5 and 300 seconds',
          code: 'INVALID_VALUE',
        })
      );
    });

    it('should validate node configurations', () => {
      const configWithMissingNode = {
        ...validConfig,
        nodes: {
          primary: validNodeConfig,
          // backup missing
        },
      };
      
      const result = Validator.validateConfig(configWithMissingNode);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'nodes.backup',
          message: 'Backup node configuration is required',
          code: 'REQUIRED_FIELD_MISSING',
        })
      );
    });

    it('should warn about duplicate node hosts', () => {
      const configWithDuplicateHosts = {
        ...validConfig,
        nodes: {
          primary: { ...validNodeConfig, host: '192.168.1.10' },
          backup: { ...validNodeConfig, host: '192.168.1.10' },
        },
      };
      
      const result = Validator.validateConfig(configWithDuplicateHosts);
      
      expect(result.warnings).toContainEqual(
        expect.objectContaining({
          field: 'nodes',
          message: 'Primary and backup nodes have the same host',
        })
      );
    });
  });

  describe('validateNodeConfig', () => {
    it('should validate a complete node configuration', () => {
      const result = Validator.validateNodeConfig(validNodeConfig);
      
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should reject invalid node structure', () => {
      const result = Validator.validateNodeConfig('invalid');
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'node',
          message: 'Node configuration must be an object',
          code: 'INVALID_TYPE',
        })
      );
    });

    it('should require all node fields', () => {
      const incompleteNode = { label: 'test' };
      const result = Validator.validateNodeConfig(incompleteNode);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'node.host',
          message: "Required field 'host' is missing",
          code: 'REQUIRED_FIELD_MISSING',
        })
      );
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'node.port',
          message: "Required field 'port' is missing",
          code: 'REQUIRED_FIELD_MISSING',
        })
      );
    });

    it('should validate host formats', () => {
      const nodeWithInvalidHost = {
        ...validNodeConfig,
        host: 'invalid..host',
      };
      
      const result = Validator.validateNodeConfig(nodeWithInvalidHost);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'node.host',
          message: 'Invalid hostname format',
          code: 'INVALID_VALUE',
        })
      );
    });

    it('should validate IPv4 addresses', () => {
      const nodeWithInvalidIP = {
        ...validNodeConfig,
        host: '300.300.300.300',
      };
      
      const result = Validator.validateNodeConfig(nodeWithInvalidIP);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'node.host',
          message: 'Invalid IPv4 address',
          code: 'INVALID_VALUE',
        })
      );
    });

    it('should validate port ranges', () => {
      const nodeWithInvalidPort = {
        ...validNodeConfig,
        port: 70000,
      };
      
      const result = Validator.validateNodeConfig(nodeWithInvalidPort);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'node.port',
          message: 'Port must be an integer between 1 and 65535',
          code: 'INVALID_RANGE',
        })
      );
    });

    it('should warn about privileged ports', () => {
      const nodeWithPrivilegedPort = {
        ...validNodeConfig,
        port: 80,
      };
      
      const result = Validator.validateNodeConfig(nodeWithPrivilegedPort);
      
      expect(result.warnings).toContainEqual(
        expect.objectContaining({
          field: 'node.port',
          message: 'Using privileged port (< 1024)',
        })
      );
    });

    it('should warn about localhost usage', () => {
      const nodeWithLocalhost = {
        ...validNodeConfig,
        host: 'localhost',
      };
      
      const result = Validator.validateNodeConfig(nodeWithLocalhost);
      
      expect(result.warnings).toContainEqual(
        expect.objectContaining({
          field: 'node.host',
          message: 'Using localhost may cause issues in production',
        })
      );
    });
  });

  describe('validateSSHKey', () => {
    const validSSHKey = {
      path: '/home/user/.ssh/id_ed25519',
      type: 'ed25519',
      fingerprint: 'SHA256:abcd1234567890abcdef1234567890abcdef1234',
    };

    it('should validate a complete SSH key', () => {
      const result = Validator.validateSSHKey(validSSHKey);
      
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should reject invalid SSH key structure', () => {
      const result = Validator.validateSSHKey('invalid');
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'sshKey',
          message: 'SSH key must be an object',
          code: 'INVALID_TYPE',
        })
      );
    });

    it('should require SSH key path', () => {
      const sshKeyWithoutPath = { type: 'ed25519' };
      const result = Validator.validateSSHKey(sshKeyWithoutPath);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'sshKey.path',
          message: 'SSH key path is required and must be a string',
          code: 'REQUIRED_FIELD_MISSING',
        })
      );
    });

    it('should validate SSH key types', () => {
      const sshKeyWithInvalidType = {
        ...validSSHKey,
        type: 'invalid',
      };
      
      const result = Validator.validateSSHKey(sshKeyWithInvalidType);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'sshKey.type',
          message: "Invalid SSH key type 'invalid'. Must be one of: rsa, ed25519, ecdsa, dsa",
          code: 'INVALID_VALUE',
        })
      );
    });
  });

  describe('validateEnvironment', () => {
    it('should validate known environment variables', () => {
      const env = {
        SVS_CONFIG_PATH: '/path/to/config',
        SVS_SSH_TIMEOUT: '30',
        SVS_LOG_LEVEL: 'info',
      };
      
      const result = Validator.validateEnvironment(env);
      
      expect(result.valid).toBe(true);
      expect(result.errors).toHaveLength(0);
    });

    it('should warn about unknown environment variables', () => {
      const env = {
        UNKNOWN_VAR: 'value',
      };
      
      const result = Validator.validateEnvironment(env);
      
      expect(result.warnings).toContainEqual(
        expect.objectContaining({
          field: 'UNKNOWN_VAR',
          message: "Unknown environment variable 'UNKNOWN_VAR'",
        })
      );
    });

    it('should validate environment variable values', () => {
      const env = {
        SVS_SSH_TIMEOUT: 'invalid',
      };
      
      const result = Validator.validateEnvironment(env);
      
      expect(result.valid).toBe(false);
      expect(result.errors).toContainEqual(
        expect.objectContaining({
          field: 'SVS_SSH_TIMEOUT',
          message: 'Expected number, got string',
          code: 'INVALID_TYPE',
        })
      );
    });
  });
});