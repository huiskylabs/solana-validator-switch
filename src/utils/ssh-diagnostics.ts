// SSH Connection Diagnostics - comprehensive connection testing and analysis

import { Socket } from 'net';
import { promisify } from 'util';
import { exec } from 'child_process';

import type { 
  SSHDiagnostics
} from '../types/ssh.js';
import { Logger } from './logger.js';
import type { NodeConfig } from '../types/config.js';

const execAsync = promisify(exec);

export class SSHDiagnosticsService {
  private logger: Logger;

  constructor() {
    this.logger = new Logger({ level: 'info' });
  }

  /**
   * Comprehensive SSH connection diagnostics
   */
  async runDiagnostics(
    nodeConfig: NodeConfig,
    sshKeyPath: string
  ): Promise<SSHDiagnostics> {
    const connectionId = `${nodeConfig.user}@${nodeConfig.host}:${nodeConfig.port}`;
    
    this.logger.info(`Running SSH diagnostics for ${connectionId}`);
    
    const diagnostics: SSHDiagnostics = {
      connectionId,
      host: nodeConfig.host,
      port: nodeConfig.port,
      reachable: false,
      sshService: false,
      authentication: false,
      errors: [],
      warnings: [],
      timestamp: new Date(),
    };

    try {
      // Test 1: Network reachability
      const reachabilityResult = await this.testReachability(nodeConfig.host, nodeConfig.port);
      diagnostics.reachable = reachabilityResult.reachable;
      if (reachabilityResult.latency) {
        diagnostics.latency = reachabilityResult.latency;
      }
      
      if (!diagnostics.reachable) {
        diagnostics.errors.push(`Host ${nodeConfig.host}:${nodeConfig.port} is not reachable`);
        return diagnostics;
      }

      // Test 2: SSH service availability
      const sshServiceResult = await this.testSSHService(nodeConfig.host, nodeConfig.port);
      diagnostics.sshService = sshServiceResult.available;
      if (sshServiceResult.version) {
        diagnostics.serverVersion = sshServiceResult.version;
      }
      
      if (sshServiceResult.banner) {
        this.logger.debug(`SSH banner: ${sshServiceResult.banner}`);
      }
      
      if (!diagnostics.sshService) {
        diagnostics.errors.push('SSH service is not available or responding');
        return diagnostics;
      }

      // Test 3: SSH key authentication
      const authResult = await this.testAuthentication(nodeConfig, sshKeyPath);
      diagnostics.authentication = authResult.success;
      
      if (authResult.keyExchange) diagnostics.keyExchange = authResult.keyExchange;
      if (authResult.cipher) diagnostics.cipher = authResult.cipher;
      if (authResult.mac) diagnostics.mac = authResult.mac;
      if (authResult.compression) diagnostics.compression = authResult.compression;
      
      if (!diagnostics.authentication) {
        diagnostics.errors.push(authResult.error || 'SSH authentication failed');
      }

      // Test 4: Basic command execution (if authenticated)
      if (diagnostics.authentication) {
        const commandResult = await this.testCommandExecution(nodeConfig, sshKeyPath);
        if (!commandResult.success) {
          diagnostics.warnings.push('Basic command execution test failed');
        }
      }

      // Test 5: Key file validation
      const keyValidation = await this.validateSSHKey(sshKeyPath);
      if (!keyValidation.valid) {
        diagnostics.errors.push(`SSH key validation failed: ${keyValidation.error}`);
      }

    } catch (error) {
      this.logger.error('Diagnostics failed:', { error: String(error) });
      diagnostics.errors.push(`Diagnostics error: ${error}`);
    }

    return diagnostics;
  }

  /**
   * Test network reachability and measure latency
   */
  private async testReachability(host: string, port: number): Promise<{
    reachable: boolean;
    latency?: number;
  }> {
    return new Promise((resolve) => {
      const startTime = Date.now();
      const socket = new Socket();
      
      const timeout = setTimeout(() => {
        socket.destroy();
        resolve({ reachable: false });
      }, 10000); // 10 second timeout

      socket.connect(port, host, () => {
        clearTimeout(timeout);
        const latency = Date.now() - startTime;
        socket.end();
        resolve({ reachable: true, latency });
      });

      socket.on('error', () => {
        clearTimeout(timeout);
        resolve({ reachable: false });
      });
    });
  }

  /**
   * Test SSH service availability and get server information
   */
  private async testSSHService(host: string, port: number): Promise<{
    available: boolean;
    version?: string;
    banner?: string;
  }> {
    return new Promise((resolve) => {
      const socket = new Socket();
      let banner = '';
      
      const timeout = setTimeout(() => {
        socket.destroy();
        resolve({ available: false });
      }, 15000);

      socket.connect(port, host, () => {
        // SSH servers should send a banner upon connection
      });

      socket.on('data', (data: Buffer) => {
        banner += data.toString();
        
        // SSH banner format: SSH-<version>-<software_version>
        const sshBannerMatch = banner.match(/SSH-([0-9.]+)-(.+)/);
        if (sshBannerMatch) {
          clearTimeout(timeout);
          socket.end();
          resolve({ 
            available: true, 
            ...(sshBannerMatch[1] && { version: sshBannerMatch[1] }),
            banner: banner.trim()
          });
        }
      });

      socket.on('error', () => {
        clearTimeout(timeout);
        resolve({ available: false });
      });

      socket.on('timeout', () => {
        clearTimeout(timeout);
        resolve({ available: false });
      });
    });
  }

  /**
   * Test SSH authentication with detailed connection info
   */
  private async testAuthentication(nodeConfig: NodeConfig, sshKeyPath: string): Promise<{
    success: boolean;
    error?: string;
    keyExchange?: string;
    cipher?: string;
    mac?: string;
    compression?: string;
  }> {
    try {
      // Use ssh command with verbose output to get connection details
      const sshCommand = [
        'ssh',
        '-o', 'BatchMode=yes',
        '-o', 'ConnectTimeout=15',
        '-o', 'PasswordAuthentication=no',
        '-o', 'PubkeyAuthentication=yes',
        '-o', 'StrictHostKeyChecking=no',
        '-o', 'UserKnownHostsFile=/dev/null',
        '-o', 'LogLevel=ERROR',
        '-i', sshKeyPath,
        '-p', nodeConfig.port.toString(),
        `${nodeConfig.user}@${nodeConfig.host}`,
        'echo "SSH_AUTH_TEST_SUCCESS"'
      ].join(' ');

      const { stdout, stderr } = await execAsync(sshCommand, { timeout: 20000 });
      
      if (stdout.includes('SSH_AUTH_TEST_SUCCESS')) {
        return { success: true };
      } else {
        return { 
          success: false, 
          error: stderr || 'Authentication test command did not return expected output'
        };
      }
    } catch (error) {
      let errorMessage = 'Unknown authentication error';
      
      if (error instanceof Error) {
        errorMessage = error.message;
        
        // Parse common SSH errors
        if (errorMessage.includes('Permission denied')) {
          errorMessage = 'SSH key authentication failed - permission denied';
        } else if (errorMessage.includes('Connection refused')) {
          errorMessage = 'SSH connection refused';
        } else if (errorMessage.includes('Host key verification failed')) {
          errorMessage = 'SSH host key verification failed';
        } else if (errorMessage.includes('timeout')) {
          errorMessage = 'SSH connection timeout';
        }
      }
      
      return { success: false, error: errorMessage };
    }
  }

  /**
   * Test basic command execution capability
   */
  private async testCommandExecution(nodeConfig: NodeConfig, sshKeyPath: string): Promise<{
    success: boolean;
    error?: string;
  }> {
    try {
      const sshCommand = [
        'ssh',
        '-o', 'BatchMode=yes',
        '-o', 'ConnectTimeout=10',
        '-o', 'StrictHostKeyChecking=no',
        '-o', 'UserKnownHostsFile=/dev/null',
        '-o', 'LogLevel=ERROR',
        '-i', sshKeyPath,
        '-p', nodeConfig.port.toString(),
        `${nodeConfig.user}@${nodeConfig.host}`,
        'whoami && pwd && date'
      ].join(' ');

      const { stdout } = await execAsync(sshCommand, { timeout: 15000 });
      
      return { success: stdout.trim().length > 0 };
    } catch (error) {
      return { 
        success: false, 
        error: error instanceof Error ? error.message : 'Command execution test failed'
      };
    }
  }

  /**
   * Validate SSH private key file
   */
  private async validateSSHKey(keyPath: string): Promise<{
    valid: boolean;
    error?: string;
    type?: string;
    fingerprint?: string;
  }> {
    try {
      // Use ssh-keygen to validate and get key information
      const { stdout } = await execAsync(`ssh-keygen -l -f "${keyPath}" 2>/dev/null`);
      
      // Parse ssh-keygen output: "2048 SHA256:... user@host (RSA)"
      const keyInfoMatch = stdout.match(/^(\d+)\s+([A-Za-z0-9+\/=:]+)\s+.*\(([A-Z0-9]+)\)$/);
      
      if (keyInfoMatch) {
        return {
          valid: true,
          ...(keyInfoMatch[3] && { type: keyInfoMatch[3].toLowerCase() }),
          ...(keyInfoMatch[2] && { fingerprint: keyInfoMatch[2] })
        };
      }
      
      return { valid: true }; // Basic validation passed
    } catch (error) {
      return { 
        valid: false, 
        error: error instanceof Error ? error.message : 'Key validation failed'
      };
    }
  }

  /**
   * Get detailed SSH client information
   */
  async getSSHClientInfo(): Promise<{
    version?: string;
    available: boolean;
    supportedAlgorithms?: string[];
  }> {
    try {
      const { stdout } = await execAsync('ssh -V 2>&1');
      const versionMatch = stdout.match(/OpenSSH_([0-9.]+)/);
      
      return {
        available: true,
        ...(versionMatch?.[1] && { version: versionMatch[1] })
      };
    } catch (error) {
      return { available: false };
    }
  }

  /**
   * Generate a diagnostic report in human-readable format
   */
  generateReport(diagnostics: SSHDiagnostics): string {
    const lines: string[] = [];
    
    lines.push(`SSH Diagnostics Report for ${diagnostics.connectionId}`);
    lines.push(`Generated: ${diagnostics.timestamp.toISOString()}`);
    lines.push('');
    
    lines.push('Test Results:');
    lines.push(`  ✓ Network Reachability: ${diagnostics.reachable ? 'PASS' : 'FAIL'}`);
    if (diagnostics.latency) {
      lines.push(`    Latency: ${diagnostics.latency}ms`);
    }
    
    lines.push(`  ✓ SSH Service: ${diagnostics.sshService ? 'PASS' : 'FAIL'}`);
    if (diagnostics.serverVersion) {
      lines.push(`    Server Version: SSH-${diagnostics.serverVersion}`);
    }
    
    lines.push(`  ✓ Authentication: ${diagnostics.authentication ? 'PASS' : 'FAIL'}`);
    if (diagnostics.keyExchange) {
      lines.push(`    Key Exchange: ${diagnostics.keyExchange}`);
    }
    if (diagnostics.cipher) {
      lines.push(`    Cipher: ${diagnostics.cipher}`);
    }
    
    if (diagnostics.errors.length > 0) {
      lines.push('');
      lines.push('Errors:');
      diagnostics.errors.forEach(error => lines.push(`  ✗ ${error}`));
    }
    
    if (diagnostics.warnings.length > 0) {
      lines.push('');
      lines.push('Warnings:');
      diagnostics.warnings.forEach(warning => lines.push(`  ⚠ ${warning}`));
    }
    
    return lines.join('\n');
  }
}