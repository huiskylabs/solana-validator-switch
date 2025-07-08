import fs from 'fs/promises';
import path from 'path';
import os from 'os';
import { SSHKeyDetector } from '../../src/utils/ssh-key-detector.js';
import type { SSHKey } from '../../src/types/config.js';

// Mock fs operations
jest.mock('fs/promises');
const mockFs = fs as jest.Mocked<typeof fs>;

// Mock os module
jest.mock('os', () => ({
  homedir: jest.fn().mockReturnValue('/home/testuser'),
}));

// Mock SSH2 library
jest.mock('ssh2', () => ({
  Client: jest.fn().mockImplementation(() => ({
    connect: jest.fn(),
    end: jest.fn(),
    on: jest.fn(),
  })),
}));

describe('SSHKeyDetector', () => {
  let detector: SSHKeyDetector;

  const mockSSHKeys = [
    {
      name: 'id_ed25519',
      content: '-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZWQyNTUxOQAAACDTvvqHjk\n-----END OPENSSH PRIVATE KEY-----',
    },
    {
      name: 'id_ed25519.pub',
      content: 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINe++oeOTQ== user@example.com',
    },
    {
      name: 'id_rsa',
      content: '-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----',
    },
    {
      name: 'id_rsa.pub',
      content: 'ssh-rsa AAAAB3NzaC1yc2EAAAADAQABAAABAQ... user@example.com',
    },
    {
      name: 'config',
      content: 'Host example\\n  HostName example.com',
    },
  ];

  beforeEach(() => {
    detector = new SSHKeyDetector();
    jest.clearAllMocks();
    
    // Default mock implementations
    mockFs.readdir.mockResolvedValue(
      mockSSHKeys.map(key => ({ name: key.name, isFile: () => true }) as any)
    );
    
    mockFs.readFile.mockImplementation((filePath: string) => {
      const fileName = path.basename(filePath as string);
      const keyFile = mockSSHKeys.find(key => key.name === fileName);
      return Promise.resolve(keyFile?.content || '');
    });
    
    mockFs.access.mockResolvedValue(undefined);
    mockFs.stat.mockResolvedValue({
      mode: 0o600,
      mtime: new Date(),
    } as any);
  });

  describe('detectKeys', () => {
    it('should detect SSH keys in default directory', async () => {
      const result = await detector.detectKeys();
      
      expect(result.keys).toHaveLength(2); // Only private keys
      expect(result.keys[0]).toEqual(
        expect.objectContaining({
          path: '/home/testuser/.ssh/id_ed25519',
          type: 'ed25519',
          valid: true,
        })
      );
      expect(result.keys[1]).toEqual(
        expect.objectContaining({
          path: '/home/testuser/.ssh/id_rsa',
          type: 'rsa',
          valid: true,
        })
      );
    });

    it('should detect keys in custom directory', async () => {
      const customDir = '/custom/ssh/path';
      
      const result = await detector.detectKeys(customDir);
      
      expect(mockFs.readdir).toHaveBeenCalledWith(customDir, { withFileTypes: true });
      expect(result.keys).toHaveLength(2);
    });

    it('should handle missing SSH directory gracefully', async () => {
      mockFs.readdir.mockRejectedValue(new Error('Directory not found'));
      
      const result = await detector.detectKeys();
      
      expect(result.keys).toHaveLength(0);
      expect(result.warnings).toContain('SSH directory not found or not accessible');
    });

    it('should validate key file permissions', async () => {
      mockFs.stat.mockResolvedValue({
        mode: 0o644, // Too permissive
        mtime: new Date(),
      } as any);
      
      const result = await detector.detectKeys();
      
      expect(result.warnings).toContain(
        expect.stringContaining('has overly permissive permissions')
      );
    });

    it('should filter out non-key files', async () => {
      mockFs.readdir.mockResolvedValue([
        { name: 'id_ed25519', isFile: () => true },
        { name: 'known_hosts', isFile: () => true },
        { name: 'config', isFile: () => true },
        { name: 'subdirectory', isFile: () => false },
      ] as any);
      
      const result = await detector.detectKeys();
      
      expect(result.keys).toHaveLength(1); // Only id_ed25519
    });

    it('should handle corrupted key files', async () => {
      mockFs.readFile.mockResolvedValueOnce('invalid key content');
      
      const result = await detector.detectKeys();
      
      expect(result.keys.some(key => !key.valid)).toBe(true);
      expect(result.warnings.length).toBeGreaterThan(0);
    });

    it('should extract comments from public keys', async () => {
      const result = await detector.detectKeys();
      
      const ed25519Key = result.keys.find(key => key.type === 'ed25519');
      expect(ed25519Key?.comment).toBe('user@example.com');
    });

    it('should calculate fingerprints for keys', async () => {
      const result = await detector.detectKeys();
      
      result.keys.forEach(key => {
        expect(key.fingerprint).toBeDefined();
        expect(key.fingerprint).toMatch(/^[a-fA-F0-9:]+$/);
      });
    });
  });

  describe('getRecommendedKey', () => {
    const sampleKeys: SSHKey[] = [
      {
        path: '/home/user/.ssh/id_rsa',
        type: 'rsa',
        fingerprint: 'SHA256:abc123',
        bits: 2048,
        valid: true,
        created: new Date('2020-01-01'),
      },
      {
        path: '/home/user/.ssh/id_ed25519',
        type: 'ed25519',
        fingerprint: 'SHA256:def456',
        valid: true,
        created: new Date('2022-01-01'),
      },
      {
        path: '/home/user/.ssh/id_ecdsa',
        type: 'ecdsa',
        fingerprint: 'SHA256:ghi789',
        bits: 256,
        valid: true,
        created: new Date('2021-01-01'),
      },
    ];

    it('should prefer ed25519 keys', () => {
      const recommended = detector.getRecommendedKey(sampleKeys);
      
      expect(recommended?.type).toBe('ed25519');
      expect(recommended?.path).toBe('/home/user/.ssh/id_ed25519');
    });

    it('should prefer newer keys when types are equal', () => {
      const rsaKeys: SSHKey[] = [
        {
          path: '/home/user/.ssh/id_rsa_old',
          type: 'rsa',
          fingerprint: 'SHA256:old',
          bits: 2048,
          valid: true,
          created: new Date('2020-01-01'),
        },
        {
          path: '/home/user/.ssh/id_rsa_new',
          type: 'rsa',
          fingerprint: 'SHA256:new',
          bits: 4096,
          valid: true,
          created: new Date('2022-01-01'),
        },
      ];
      
      const recommended = detector.getRecommendedKey(rsaKeys);
      
      expect(recommended?.path).toBe('/home/user/.ssh/id_rsa_new');
    });

    it('should prefer larger key sizes for RSA', () => {
      const rsaKeys: SSHKey[] = [
        {
          path: '/home/user/.ssh/id_rsa_2048',
          type: 'rsa',
          fingerprint: 'SHA256:small',
          bits: 2048,
          valid: true,
          created: new Date('2022-01-01'),
        },
        {
          path: '/home/user/.ssh/id_rsa_4096',
          type: 'rsa',
          fingerprint: 'SHA256:large',
          bits: 4096,
          valid: true,
          created: new Date('2022-01-01'),
        },
      ];
      
      const recommended = detector.getRecommendedKey(rsaKeys);
      
      expect(recommended?.bits).toBe(4096);
    });

    it('should skip invalid keys', () => {
      const keysWithInvalid: SSHKey[] = [
        {
          path: '/home/user/.ssh/id_invalid',
          type: 'rsa',
          fingerprint: 'SHA256:invalid',
          valid: false,
          created: new Date('2022-01-01'),
        },
        ...sampleKeys,
      ];
      
      const recommended = detector.getRecommendedKey(keysWithInvalid);
      
      expect(recommended?.valid).toBe(true);
      expect(recommended?.path).not.toBe('/home/user/.ssh/id_invalid');
    });

    it('should return null for empty key list', () => {
      const recommended = detector.getRecommendedKey([]);
      
      expect(recommended).toBeNull();
    });

    it('should return null when all keys are invalid', () => {
      const invalidKeys: SSHKey[] = [
        {
          path: '/home/user/.ssh/id_invalid',
          type: 'rsa',
          fingerprint: 'SHA256:invalid',
          valid: false,
          created: new Date('2022-01-01'),
        },
      ];
      
      const recommended = detector.getRecommendedKey(invalidKeys);
      
      expect(recommended).toBeNull();
    });
  });

  describe('testConnection', () => {
    let mockClient: any;

    beforeEach(() => {
      const { Client } = require('ssh2');
      mockClient = {
        connect: jest.fn(),
        end: jest.fn(),
        on: jest.fn(),
      };
      Client.mockImplementation(() => mockClient);
    });

    it('should test SSH connection successfully', async () => {
      // Mock successful connection
      mockClient.on.mockImplementation((event: string, callback: Function) => {
        if (event === 'ready') {
          setTimeout(callback, 10);
        }
      });

      const result = await detector.testConnection(
        '192.168.1.10',
        22,
        'testuser',
        '/home/user/.ssh/id_ed25519'
      );

      expect(result.success).toBe(true);
      expect(result.error).toBeUndefined();
      expect(mockClient.connect).toHaveBeenCalledWith({
        host: '192.168.1.10',
        port: 22,
        username: 'testuser',
        privateKey: expect.any(String),
        readyTimeout: 30000,
      });
    });

    it('should handle connection errors', async () => {
      const testError = new Error('Connection refused');
      
      mockClient.on.mockImplementation((event: string, callback: Function) => {
        if (event === 'error') {
          setTimeout(() => callback(testError), 10);
        }
      });

      const result = await detector.testConnection(
        '192.168.1.10',
        22,
        'testuser',
        '/home/user/.ssh/id_ed25519'
      );

      expect(result.success).toBe(false);
      expect(result.error).toBe('Connection refused');
    });

    it('should handle connection timeout', async () => {
      // Don't trigger any events to simulate timeout
      mockClient.on.mockImplementation(() => {});

      const result = await detector.testConnection(
        '192.168.1.10',
        22,
        'testuser',
        '/home/user/.ssh/id_ed25519',
        1000 // 1 second timeout
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain('timeout');
    });

    it('should handle missing key file', async () => {
      mockFs.readFile.mockRejectedValue(new Error('Key file not found'));

      const result = await detector.testConnection(
        '192.168.1.10',
        22,
        'testuser',
        '/nonexistent/key'
      );

      expect(result.success).toBe(false);
      expect(result.error).toContain('Key file not found');
    });

    it('should use custom timeout', async () => {
      mockClient.on.mockImplementation((event: string, callback: Function) => {
        if (event === 'ready') {
          setTimeout(callback, 10);
        }
      });

      await detector.testConnection(
        '192.168.1.10',
        22,
        'testuser',
        '/home/user/.ssh/id_ed25519',
        45000
      );

      expect(mockClient.connect).toHaveBeenCalledWith(
        expect.objectContaining({
          readyTimeout: 45000,
        })
      );
    });
  });

  describe('parseKeyType', () => {
    it('should detect ed25519 keys', () => {
      const content = '-----BEGIN OPENSSH PRIVATE KEY-----\\nb3BlbnNzaC1rZXktdjEAAAAABG5vbmUAAAAEbm9uZQAAAAAAAAABAAAAMwAAAAtzc2gtZWQyNTUxOQ==\\n-----END OPENSSH PRIVATE KEY-----';
      
      const type = (detector as any).parseKeyType(content);
      
      expect(type).toBe('ed25519');
    });

    it('should detect RSA keys', () => {
      const content = '-----BEGIN RSA PRIVATE KEY-----\\nMIIEowIBAAKCAQEA...\\n-----END RSA PRIVATE KEY-----';
      
      const type = (detector as any).parseKeyType(content);
      
      expect(type).toBe('rsa');
    });

    it('should detect ECDSA keys', () => {
      const content = '-----BEGIN EC PRIVATE KEY-----\\nMHcCAQEEI...\\n-----END EC PRIVATE KEY-----';
      
      const type = (detector as any).parseKeyType(content);
      
      expect(type).toBe('ecdsa');
    });

    it('should detect DSA keys', () => {
      const content = '-----BEGIN DSA PRIVATE KEY-----\\nMIIBuwIBAAKBgQD...\\n-----END DSA PRIVATE KEY-----';
      
      const type = (detector as any).parseKeyType(content);
      
      expect(type).toBe('dsa');
    });

    it('should handle unknown key types', () => {
      const content = '-----BEGIN UNKNOWN KEY-----\\n...\\n-----END UNKNOWN KEY-----';
      
      const type = (detector as any).parseKeyType(content);
      
      expect(type).toBe('unknown');
    });
  });

  describe('calculateFingerprint', () => {
    it('should calculate SHA256 fingerprint', () => {
      const keyContent = 'ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAINe++oeOTQ== user@example.com';
      
      const fingerprint = (detector as any).calculateFingerprint(keyContent);
      
      expect(fingerprint).toMatch(/^[a-fA-F0-9:]+$/);
      expect(fingerprint.split(':')).toHaveLength(16); // SHA256 produces 32 hex chars = 16 pairs
    });

    it('should handle invalid key content', () => {
      const invalidContent = 'invalid key content';
      
      const fingerprint = (detector as any).calculateFingerprint(invalidContent);
      
      expect(fingerprint).toBe('unknown');
    });
  });

  describe('edge cases and error handling', () => {
    it('should handle permission errors when reading SSH directory', async () => {
      mockFs.readdir.mockRejectedValue(new Error('Permission denied'));
      
      const result = await detector.detectKeys();
      
      expect(result.keys).toHaveLength(0);
      expect(result.warnings).toContain('SSH directory not found or not accessible');
    });

    it('should handle corrupted SSH key files', async () => {
      mockFs.readFile.mockImplementation((filePath: string) => {
        const fileName = path.basename(filePath as string);
        if (fileName === 'id_ed25519') {
          return Promise.resolve('corrupted key content');
        }
        return Promise.reject(new Error('File not found'));
      });
      
      const result = await detector.detectKeys();
      
      expect(result.keys.some(key => !key.valid)).toBe(true);
    });

    it('should handle empty SSH directory', async () => {
      mockFs.readdir.mockResolvedValue([]);
      
      const result = await detector.detectKeys();
      
      expect(result.keys).toHaveLength(0);
      expect(result.warnings).toContain('No SSH keys found in directory');
    });

    it('should handle mixed valid and invalid keys', async () => {
      mockFs.readdir.mockResolvedValue([
        { name: 'id_ed25519', isFile: () => true },
        { name: 'id_corrupted', isFile: () => true },
      ] as any);
      
      mockFs.readFile.mockImplementation((filePath: string) => {
        const fileName = path.basename(filePath as string);
        if (fileName === 'id_ed25519') {
          return Promise.resolve(mockSSHKeys[0].content);
        }
        if (fileName === 'id_corrupted') {
          return Promise.resolve('corrupted content');
        }
        return Promise.reject(new Error('File not found'));
      });
      
      const result = await detector.detectKeys();
      
      expect(result.keys).toHaveLength(2);
      expect(result.keys.filter(key => key.valid)).toHaveLength(1);
      expect(result.keys.filter(key => !key.valid)).toHaveLength(1);
    });
  });
});