import fs from 'fs/promises';
import path from 'path';
import os from 'os';
import { spawn } from 'child_process';
import type { SSHKey } from '../types/config.js';
import { Logger } from './logger.js';
import { Validator } from './validator.js';

export interface SSHKeyDetectionResult {
  keys: SSHKey[];
  errors: string[];
  warnings: string[];
}

export interface SSHKeyInfo {
  path: string;
  type: 'rsa' | 'ed25519' | 'ecdsa' | 'dsa';
  bits?: number;
  fingerprint: string;
  comment?: string;
  created?: Date;
  valid: boolean;
  accessible: boolean;
  hasPublicKey: boolean;
}

export class SSHKeyDetector {
  private logger: Logger;
  private sshDir: string;

  constructor(sshDir?: string) {
    this.logger = new Logger();
    this.sshDir = sshDir || path.join(os.homedir(), '.ssh');
  }

  /**
   * Detect all SSH keys in the SSH directory
   */
  async detectKeys(): Promise<SSHKeyDetectionResult> {
    const keys: SSHKey[] = [];
    const errors: string[] = [];
    const warnings: string[] = [];

    try {
      // Check if SSH directory exists
      if (!(await this.directoryExists(this.sshDir))) {
        errors.push('SSH directory not found. Please ensure SSH is set up.');
        return { keys, errors, warnings };
      }

      // Get list of potential key files
      const keyFiles = await this.findPotentialKeyFiles();

      if (keyFiles.length === 0) {
        warnings.push('No SSH key files found in SSH directory');
        return { keys, errors, warnings };
      }

      // Process each potential key file
      for (const keyFile of keyFiles) {
        try {
          const keyInfo = await this.analyzeKeyFile(keyFile);
          if (keyInfo) {
            // Validate the key
            const validation = Validator.validateSSHKey(keyInfo);

            const sshKey: SSHKey = {
              path: keyInfo.path,
              type: keyInfo.type,
              fingerprint: keyInfo.fingerprint,
              bits: keyInfo.bits || 0,
              ...(keyInfo.comment && { comment: keyInfo.comment }),
              ...(keyInfo.created && { created: keyInfo.created }),
              valid: keyInfo.valid && validation.valid,
            };

            keys.push(sshKey);

            // Add validation warnings
            if (validation.warnings.length > 0) {
              warnings.push(
                ...validation.warnings.map(w => `${keyFile}: ${w.message}`)
              );
            }
          }
        } catch (error) {
          const errorMessage =
            error instanceof Error ? error.message : 'Unknown error';
          warnings.push(
            `Failed to analyze key file ${keyFile}: ${errorMessage}`
          );
        }
      }

      // Sort keys by preference (ed25519 first, then by key size)
      keys.sort(this.compareKeys);

      // Add recommendations
      if (keys.length === 0) {
        errors.push('No valid SSH keys found. Please generate SSH keys first.');
      } else {
        this.addRecommendations(keys, warnings);
      }
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : 'Unknown error';
      errors.push(`Failed to detect SSH keys: ${errorMessage}`);
    }

    return { keys, errors, warnings };
  }

  /**
   * Generate a new SSH key pair
   */
  async generateKeyPair(
    type: 'rsa' | 'ed25519' | 'ecdsa' = 'ed25519',
    bits?: number,
    comment?: string
  ): Promise<SSHKey> {
    const keyPath = path.join(this.sshDir, `id_${type}`);

    // Check if key already exists
    if (await this.fileExists(keyPath)) {
      throw new Error(`SSH key already exists at ${keyPath}`);
    }

    try {
      await this.runSSHKeygen(type, keyPath, bits, comment);

      // Analyze the generated key
      const keyInfo = await this.analyzeKeyFile(keyPath);
      if (!keyInfo) {
        throw new Error('Failed to analyze generated key');
      }

      this.logger.success(`SSH key generated successfully: ${keyPath}`);

      return {
        path: keyInfo.path,
        type: keyInfo.type,
        fingerprint: keyInfo.fingerprint,
        bits: keyInfo.bits || 0,
        ...(keyInfo.comment && { comment: keyInfo.comment }),
        created: new Date(),
        valid: true,
      };
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : 'Unknown error';
      throw new Error(`Failed to generate SSH key: ${errorMessage}`);
    }
  }

  /**
   * Test SSH key connectivity to a host
   */
  async testKeyConnectivity(
    sshKey: SSHKey,
    host: string,
    port: number = 22,
    user: string = 'root'
  ): Promise<{ success: boolean; message: string; latency?: number }> {
    const startTime = Date.now();

    try {
      const result = await this.runSSHTest(sshKey.path, host, port, user);
      const latency = Date.now() - startTime;

      if (result.success) {
        return {
          success: true,
          message: 'SSH connection successful',
          latency,
        };
      } else {
        return {
          success: false,
          message: result.error || 'SSH connection failed',
        };
      }
    } catch (error) {
      const errorMessage =
        error instanceof Error ? error.message : 'Unknown error';
      return {
        success: false,
        message: `SSH test failed: ${errorMessage}`,
      };
    }
  }

  /**
   * Get recommended SSH key from detected keys
   */
  getRecommendedKey(keys: SSHKey[]): SSHKey | null {
    if (keys.length === 0) return null;

    // Prefer ed25519 keys
    const ed25519Keys = keys.filter(k => k.type === 'ed25519' && k.valid);
    if (ed25519Keys.length > 0) {
      return ed25519Keys[0] || null;
    }

    // Then prefer RSA keys with 4096 bits
    const rsaKeys = keys.filter(
      k => k.type === 'rsa' && k.valid && (k.bits || 0) >= 4096
    );
    if (rsaKeys.length > 0) {
      return rsaKeys[0] || null;
    }

    // Return the first valid key
    const validKeys = keys.filter(k => k.valid);
    return validKeys[0] ?? null;
  }

  // Private helper methods

  private async directoryExists(dirPath: string): Promise<boolean> {
    try {
      const stats = await fs.stat(dirPath);
      return stats.isDirectory();
    } catch {
      return false;
    }
  }

  private async fileExists(filePath: string): Promise<boolean> {
    try {
      await fs.access(filePath);
      return true;
    } catch {
      return false;
    }
  }

  private async findPotentialKeyFiles(): Promise<string[]> {
    try {
      const files = await fs.readdir(this.sshDir);
      const keyFiles: string[] = [];

      // Common SSH key file patterns
      const keyPatterns = [
        /^id_rsa$/,
        /^id_ed25519$/,
        /^id_ecdsa$/,
        /^id_dsa$/,
        // Additional patterns for custom named keys
        /^.*_rsa$/,
        /^.*_ed25519$/,
        /^.*_ecdsa$/,
        /^.*_dsa$/,
      ];

      for (const file of files) {
        // Skip .pub files, known_hosts, config, etc.
        if (
          file.endsWith('.pub') ||
          file.includes('known_hosts') ||
          file === 'config' ||
          file === 'authorized_keys'
        ) {
          continue;
        }

        const filePath = path.join(this.sshDir, file);

        // Check if it matches key patterns or could be a private key
        const matchesPattern = keyPatterns.some(pattern => pattern.test(file));
        if (matchesPattern || (await this.couldBePrivateKey(filePath))) {
          keyFiles.push(filePath);
        }
      }

      return keyFiles;
    } catch (error) {
      this.logger.warning(`Failed to read SSH directory: ${error}`);
      return [];
    }
  }

  private async couldBePrivateKey(filePath: string): Promise<boolean> {
    try {
      const content = await fs.readFile(filePath, 'utf-8');
      const lines = content.split('\n');

      // Check for common private key headers
      const privateKeyHeaders = [
        '-----BEGIN OPENSSH PRIVATE KEY-----',
        '-----BEGIN RSA PRIVATE KEY-----',
        '-----BEGIN DSA PRIVATE KEY-----',
        '-----BEGIN EC PRIVATE KEY-----',
        '-----BEGIN PRIVATE KEY-----',
      ];

      return lines.some(line =>
        privateKeyHeaders.some(header => line.trim().startsWith(header))
      );
    } catch {
      return false;
    }
  }

  private async analyzeKeyFile(keyPath: string): Promise<SSHKeyInfo | null> {
    try {
      // Check if file is accessible
      const accessible = await this.fileExists(keyPath);
      if (!accessible) {
        return null;
      }

      // Check if public key exists
      const publicKeyPath = `${keyPath}.pub`;
      const hasPublicKey = await this.fileExists(publicKeyPath);

      // Get key information using ssh-keygen
      const keyInfo = await this.getKeyInfoWithSSHKeygen(keyPath);
      if (!keyInfo) {
        return null;
      }

      // Get file creation time
      let created: Date | undefined;
      try {
        const stats = await fs.stat(keyPath);
        created = stats.birthtime;
      } catch {
        // Creation time not available
      }

      const result: any = {
        path: keyPath,
        type: keyInfo.type,
        fingerprint: keyInfo.fingerprint,
        comment: keyInfo.comment ?? undefined,
        created,
        valid: keyInfo.valid,
        accessible,
        hasPublicKey,
      };
      
      if (keyInfo.bits !== undefined) {
        result.bits = keyInfo.bits;
      }
      
      return result;
    } catch (error) {
      this.logger.warning(`Failed to analyze key ${keyPath}: ${error}`);
      return null;
    }
  }

  private async getKeyInfoWithSSHKeygen(keyPath: string): Promise<{
    type: 'rsa' | 'ed25519' | 'ecdsa' | 'dsa';
    bits?: number;
    fingerprint: string;
    comment?: string;
    valid: boolean;
  } | null> {
    try {
      // Use ssh-keygen to get key information
      const fingerprintResult = await this.runCommand('ssh-keygen', [
        '-l',
        '-f',
        keyPath,
      ]);

      if (fingerprintResult.code !== 0) {
        return null;
      }

      // Parse output: "2048 SHA256:fingerprint comment (RSA)"
      const output = fingerprintResult.stdout.trim();
      const match = output.match(
        /^(\d+)\s+([A-Za-z0-9+\/=:]+)\s+(.*?)\s+\(([A-Z0-9]+)\)$/
      );

      if (!match) {
        return null;
      }

      const [, bitsStr, fingerprint, comment, typeStr] = match;
      const bits = parseInt(bitsStr || '0', 10);
      const type = typeStr?.toLowerCase() as 'rsa' | 'ed25519' | 'ecdsa' | 'dsa';

      // Validate type
      if (!type || !(['rsa', 'ed25519', 'ecdsa', 'dsa'] as string[]).includes(type)) {
        return null;
      }

      const result: any = {
        type,
        fingerprint: fingerprint || '',
        comment: comment?.trim() || undefined,
        valid: true,
      };
      
      if (!isNaN(bits)) {
        result.bits = bits;
      }
      
      return result;
    } catch (error) {
      this.logger.warning(`Failed to get key info for ${keyPath}: ${error}`);
      return null;
    }
  }

  private async runSSHKeygen(
    type: 'rsa' | 'ed25519' | 'ecdsa',
    keyPath: string,
    bits?: number,
    comment?: string
  ): Promise<void> {
    const args = ['-t', type, '-f', keyPath, '-N', ''];

    if (bits && type === 'rsa') {
      args.push('-b', bits.toString());
    }

    if (comment) {
      args.push('-C', comment);
    }

    const result = await this.runCommand('ssh-keygen', args);

    if (result.code !== 0) {
      throw new Error(`ssh-keygen failed: ${result.stderr}`);
    }
  }

  private async runSSHTest(
    keyPath: string,
    host: string,
    port: number,
    user: string
  ): Promise<{ success: boolean; error?: string }> {
    const args = [
      '-i',
      keyPath,
      '-o',
      'ConnectTimeout=10',
      '-o',
      'BatchMode=yes',
      '-o',
      'StrictHostKeyChecking=no',
      '-p',
      port.toString(),
      `${user}@${host}`,
      'exit',
    ];

    const result = await this.runCommand('ssh', args, { timeout: 15000 });

    const response: { success: boolean; error?: string } = {
      success: result.code === 0,
    };
    
    if (result.code !== 0 && result.stderr) {
      response.error = result.stderr;
    }
    
    return response;
  }

  private async runCommand(
    command: string,
    args: string[],
    options: { timeout?: number } = {}
  ): Promise<{ code: number; stdout: string; stderr: string }> {
    return new Promise((resolve, reject) => {
      const child = spawn(command, args, {
        stdio: ['pipe', 'pipe', 'pipe'],
      });

      let stdout = '';
      let stderr = '';

      child.stdout?.on('data', data => {
        stdout += data.toString();
      });

      child.stderr?.on('data', data => {
        stderr += data.toString();
      });

      const timeout = options.timeout || 30000;
      const timer = setTimeout(() => {
        child.kill();
        reject(new Error(`Command timeout after ${timeout}ms`));
      }, timeout);

      child.on('close', code => {
        clearTimeout(timer);
        resolve({
          code: code || 0,
          stdout: stdout.trim(),
          stderr: stderr.trim(),
        });
      });

      child.on('error', error => {
        clearTimeout(timer);
        reject(error);
      });
    });
  }

  private compareKeys(a: SSHKey, b: SSHKey): number {
    // Prefer ed25519
    if (a.type === 'ed25519' && b.type !== 'ed25519') return -1;
    if (b.type === 'ed25519' && a.type !== 'ed25519') return 1;

    // Then prefer by key size (larger is better for RSA)
    if (a.type === 'rsa' && b.type === 'rsa') {
      return (b.bits ?? 0) - (a.bits ?? 0);
    }

    // Default to alphabetical
    return a.path.localeCompare(b.path);
  }

  private addRecommendations(keys: SSHKey[], warnings: string[]): void {
    // Check for weak keys
    const weakKeys = keys.filter(
      k => (k.type === 'rsa' && (k.bits || 0) < 2048) || k.type === 'dsa'
    );

    if (weakKeys.length > 0) {
      warnings.push(
        'Some SSH keys use weak algorithms or key sizes. Consider using ed25519 or RSA 4096-bit keys.'
      );
    }

    // Recommend ed25519 if not present
    const hasEd25519 = keys.some(k => k.type === 'ed25519');
    if (!hasEd25519) {
      warnings.push(
        'Consider generating an ed25519 key for better security and performance.'
      );
    }

    // Check for missing public keys
    const missingPublicKeys = keys.filter(k => !(k as any).hasPublicKey);
    if (missingPublicKeys.length > 0) {
      warnings.push(
        'Some private keys are missing their corresponding public keys (.pub files).'
      );
    }
  }

  /**
   * Test SSH connection to a host
   */
  async testConnection(
    host: string,
    port: number,
    user: string,
    keyPath: string
  ): Promise<{ success: boolean; error?: string }> {
    const args = [
      '-o', 'BatchMode=yes',
      '-o', 'ConnectTimeout=10',
      '-o', 'StrictHostKeyChecking=no',
      '-o', 'UserKnownHostsFile=/dev/null',
      '-o', 'LogLevel=QUIET',
      '-i', keyPath,
      '-p', port.toString(),
      `${user}@${host}`,
      'echo "connection_test_successful"'
    ];

    try {
      const result = await this.runCommand('ssh', args, { timeout: 15000 });
      
      if (result.code === 0 && result.stdout.includes('connection_test_successful')) {
        return { success: true };
      } else {
        return {
          success: false,
          error: result.stderr || 'Connection failed'
        };
      }
    } catch (error) {
      return {
        success: false,
        error: error instanceof Error ? error.message : 'Unknown error'
      };
    }
  }
}
