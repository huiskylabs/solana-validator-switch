// Node Detection Service - detects validator client type and auto-discovers paths

import type { 
  ValidatorClient, 
  NodeConfig, 
  NodePaths, 
  NodeMetadata 
} from '../types/config.js';
import { SSHManager } from './ssh-manager.js';
import { Logger } from '../utils/logger.js';

export interface DetectionResult {
  clientType: ValidatorClient;
  clientVersion?: string;
  detectedPaths: Partial<NodePaths>;
  systemInfo: {
    os: string;
    architecture: string;
    timezone: string;
    hostname: string;
    uptime: string;
  };
  validatorRunning: boolean;
  processes: ProcessInfo[];
  warnings: string[];
  errors: string[];
}

export interface ProcessInfo {
  pid: number;
  command: string;
  user: string;
  cpu: number;
  memory: number;
  startTime: string;
}

export class NodeDetectionService {
  private sshManager: SSHManager;
  private logger: Logger;

  constructor(sshManager: SSHManager) {
    this.sshManager = sshManager;
    this.logger = new Logger({ level: 'info' });
  }

  /**
   * Comprehensive node detection and path discovery
   */
  async detectNode(connectionId: string): Promise<DetectionResult> {
    this.logger.info(`Starting node detection for ${connectionId}`);
    
    const result: DetectionResult = {
      clientType: 'unknown',
      detectedPaths: {},
      systemInfo: {
        os: '',
        architecture: '',
        timezone: '',
        hostname: '',
        uptime: ''
      },
      validatorRunning: false,
      processes: [],
      warnings: [],
      errors: []
    };

    try {
      // Step 1: Get system information
      await this.detectSystemInfo(connectionId, result);
      
      // Step 2: Detect validator client type
      await this.detectValidatorClient(connectionId, result);
      
      // Step 3: Find validator processes
      await this.detectValidatorProcesses(connectionId, result);
      
      // Step 4: Auto-detect file paths
      await this.detectFilePaths(connectionId, result);
      
      // Step 5: Validate detected paths
      await this.validateDetectedPaths(connectionId, result);
      
    } catch (error) {
      this.logger.error('Node detection failed:', { error: String(error) });
      result.errors.push(`Detection failed: ${error}`);
    }

    return result;
  }

  /**
   * Detect system information
   */
  private async detectSystemInfo(connectionId: string, result: DetectionResult): Promise<void> {
    try {
      // Get basic system info
      const systemCommands = [
        'uname -s',           // OS name
        'uname -m',           // Architecture
        'hostname',           // Hostname
        'uptime -p 2>/dev/null || uptime',  // Uptime
        'timedatectl show -p Timezone --value 2>/dev/null || date +%Z'  // Timezone
      ];

      for (let i = 0; i < systemCommands.length; i++) {
        try {
          const command = systemCommands[i];
          if (command) {
            const cmdResult = await this.sshManager.executeCommand(connectionId, command);
            const output = cmdResult.stdout.trim();
            
            switch (i) {
              case 0: result.systemInfo.os = output; break;
              case 1: result.systemInfo.architecture = output; break;
              case 2: result.systemInfo.hostname = output; break;
              case 3: result.systemInfo.uptime = output; break;
              case 4: result.systemInfo.timezone = output; break;
            }
          }
        } catch (error) {
          this.logger.warn(`Failed to get system info (command ${i}):`, { error: String(error) });
        }
      }
      
    } catch (error) {
      result.warnings.push(`Failed to detect system info: ${error}`);
    }
  }

  /**
   * Detect validator client type and version
   */
  private async detectValidatorClient(connectionId: string, result: DetectionResult): Promise<void> {
    const detectionMethods = [
      this.detectAgave.bind(this),
      this.detectJito.bind(this),
      this.detectFiredancer.bind(this)
    ];

    for (const method of detectionMethods) {
      try {
        const detection = await method(connectionId);
        if (detection.found) {
          result.clientType = detection.clientType;
          if (detection.version) {
            result.clientVersion = detection.version;
          }
          this.logger.info(`Detected ${detection.clientType} validator (${detection.version || 'unknown version'})`);
          return;
        }
      } catch (error) {
        this.logger.debug(`Detection method failed:`, { error: String(error) });
      }
    }
    
    result.warnings.push('Could not determine validator client type');
  }

  /**
   * Detect Agave validator
   */
  private async detectAgave(connectionId: string): Promise<{
    found: boolean;
    clientType: ValidatorClient;
    version?: string;
  }> {
    try {
      // Try common Agave binary locations
      const binaryPaths = [
        'solana-validator',
        '/usr/local/bin/solana-validator',
        '~/.local/share/solana/install/active_release/bin/solana-validator',
        '/opt/solana/bin/solana-validator'
      ];

      for (const binaryPath of binaryPaths) {
        try {
          const versionResult = await this.sshManager.executeCommand(
            connectionId, 
            `${binaryPath} --version 2>/dev/null`
          );
          
          if (versionResult.stdout.includes('solana-validator')) {
            const versionMatch = versionResult.stdout.match(/solana-validator ([0-9.]+)/);
            return {
              found: true,
              clientType: 'agave',
              ...(versionMatch?.[1] && { version: versionMatch[1] })
            };
          }
        } catch {
          // Try next path
          continue;
        }
      }
      
      return { found: false, clientType: 'agave' };
    } catch (error) {
      return { found: false, clientType: 'agave' };
    }
  }

  /**
   * Detect Jito validator
   */
  private async detectJito(connectionId: string): Promise<{
    found: boolean;
    clientType: ValidatorClient;
    version?: string;
  }> {
    try {
      // Check for Jito-specific processes or binaries
      const processResult = await this.sshManager.executeCommand(
        connectionId,
        'ps aux | grep -i jito | grep -v grep'
      );
      
      if (processResult.stdout.includes('jito')) {
        // Try to get Jito version
        try {
          const versionResult = await this.sshManager.executeCommand(
            connectionId,
            'jito-validator --version 2>/dev/null || jito-solana --version 2>/dev/null'
          );
          
          const versionMatch = versionResult.stdout.match(/jito[^0-9]*([0-9.]+)/i);
          return {
            found: true,
            clientType: 'jito',
            ...(versionMatch?.[1] && { version: versionMatch[1] })
          };
        } catch {
          return { found: true, clientType: 'jito' };
        }
      }
      
      return { found: false, clientType: 'jito' };
    } catch (error) {
      return { found: false, clientType: 'jito' };
    }
  }

  /**
   * Detect Firedancer validator
   */
  private async detectFiredancer(connectionId: string): Promise<{
    found: boolean;
    clientType: ValidatorClient;
    version?: string;
  }> {
    try {
      // Check for Firedancer-specific processes
      const processResult = await this.sshManager.executeCommand(
        connectionId,
        'ps aux | grep -i firedancer | grep -v grep'
      );
      
      if (processResult.stdout.includes('firedancer')) {
        return { found: true, clientType: 'firedancer' };
      }
      
      return { found: false, clientType: 'firedancer' };
    } catch (error) {
      return { found: false, clientType: 'firedancer' };
    }
  }

  /**
   * Detect running validator processes
   */
  private async detectValidatorProcesses(connectionId: string, result: DetectionResult): Promise<void> {
    try {
      const psResult = await this.sshManager.executeCommand(
        connectionId,
        'ps aux | grep -E "(solana-validator|jito-validator|firedancer)" | grep -v grep'
      );
      
      const lines = psResult.stdout.trim().split('\n').filter(line => line.trim());
      
      for (const line of lines) {
        try {
          const parts = line.trim().split(/\s+/);
          if (parts.length >= 11 && parts[1] && parts[0] && parts[2] && parts[3] && parts[8]) {
            const process: ProcessInfo = {
              pid: parseInt(parts[1]),
              command: parts.slice(10).join(' '),
              user: parts[0],
              cpu: parseFloat(parts[2]),
              memory: parseFloat(parts[3]),
              startTime: parts[8]
            };
            
            result.processes.push(process);
            
            if (process.command.includes('validator')) {
              result.validatorRunning = true;
            }
          }
        } catch (error) {
          this.logger.debug('Failed to parse process line:', { line });
        }
      }
      
    } catch (error) {
      result.warnings.push(`Failed to detect validator processes: ${error}`);
    }
  }

  /**
   * Auto-detect common file paths
   */
  private async detectFilePaths(connectionId: string, result: DetectionResult): Promise<void> {
    const pathsToCheck = {
      fundedIdentity: [
        '~/validator-keypair.json',
        '~/identity.json',
        '~/solana/validator-keypair.json',
        '/var/lib/solana/validator-keypair.json'
      ],
      unfundedIdentity: [
        '~/unstaked-identity.json', 
        '~/backup-keypair.json',
        '~/solana/unstaked-identity.json'
      ],
      voteKeypair: [
        '~/vote-keypair.json',
        '~/vote-account-keypair.json',
        '~/solana/vote-keypair.json'
      ],
      ledger: [
        '~/ledger',
        '~/validator-ledger',
        '~/solana/ledger',
        '/var/lib/solana/ledger',
        '/mnt/solana/ledger'
      ],
      tower: [
        '~/tower.bin',
        '~/validator-tower.bin', 
        '~/solana/tower.bin',
        '/var/lib/solana/tower.bin'
      ],
      solanaCliPath: [
        'solana',
        '/usr/local/bin/solana',
        '~/.local/share/solana/install/active_release/bin/solana'
      ]
    };

    for (const [pathType, candidates] of Object.entries(pathsToCheck)) {
      for (const candidate of candidates) {
        try {
          const testCommand = pathType === 'solanaCliPath' 
            ? `which ${candidate} 2>/dev/null || command -v ${candidate}`
            : `test -e "${candidate}" && echo "EXISTS" || echo "NOT_FOUND"`;
            
          const testResult = await this.sshManager.executeCommand(connectionId, testCommand);
          
          if (pathType === 'solanaCliPath') {
            if (testResult.stdout.trim()) {
              result.detectedPaths[pathType as keyof NodePaths] = testResult.stdout.trim();
              break;
            }
          } else {
            if (testResult.stdout.includes('EXISTS')) {
              result.detectedPaths[pathType as keyof NodePaths] = candidate;
              break;
            }
          }
        } catch (error) {
          // Continue to next candidate
        }
      }
    }
  }

  /**
   * Validate detected paths
   */
  private async validateDetectedPaths(connectionId: string, result: DetectionResult): Promise<void> {
    for (const [pathType, path] of Object.entries(result.detectedPaths)) {
      if (!path) continue;
      
      try {
        if (pathType === 'solanaCliPath') {
          // Test CLI executable
          const versionResult = await this.sshManager.executeCommand(
            connectionId,
            `${path} --version 2>/dev/null`
          );
          
          if (!versionResult.stdout.includes('solana-cli')) {
            result.warnings.push(`Solana CLI at ${path} may not be valid`);
          }
        } else if (pathType === 'ledger') {
          // Test ledger directory
          const ledgerTest = await this.sshManager.executeCommand(
            connectionId,
            `test -d "${path}" && ls -la "${path}" | head -5`
          );
          
          if (!ledgerTest.stdout) {
            result.warnings.push(`Ledger directory ${path} appears empty or inaccessible`);
          }
        } else {
          // Test file existence and permissions
          const fileTest = await this.sshManager.executeCommand(
            connectionId,
            `test -r "${path}" && echo "READABLE" || echo "NOT_READABLE"`
          );
          
          if (!fileTest.stdout.includes('READABLE')) {
            result.warnings.push(`File ${path} is not readable`);
          }
        }
      } catch (error) {
        result.warnings.push(`Failed to validate ${pathType} at ${path}: ${error}`);
      }
    }
  }

  /**
   * Generate auto-detected node configuration
   */
  generateNodeConfig(
    baseConfig: Partial<NodeConfig>, 
    detectionResult: DetectionResult
  ): NodeConfig {
    const metadata: NodeMetadata = {
      detected: true,
      clientType: detectionResult.clientType,
      ...(detectionResult.clientVersion && { clientVersion: detectionResult.clientVersion }),
      lastConnected: Date.now(),
      timezone: detectionResult.systemInfo.timezone,
    };

    const paths: NodePaths = {
      fundedIdentity: detectionResult.detectedPaths.fundedIdentity || '',
      unfundedIdentity: detectionResult.detectedPaths.unfundedIdentity || '',
      voteKeypair: detectionResult.detectedPaths.voteKeypair || '',
      ledger: detectionResult.detectedPaths.ledger || '',
      tower: detectionResult.detectedPaths.tower || '',
      solanaCliPath: detectionResult.detectedPaths.solanaCliPath || 'solana',
    };

    return {
      label: baseConfig.label || detectionResult.systemInfo.hostname || 'detected-node',
      host: baseConfig.host || '',
      port: baseConfig.port || 22,
      user: baseConfig.user || '',
      paths,
      metadata
    };
  }

  /**
   * Generate detection report
   */
  generateDetectionReport(result: DetectionResult): string {
    const lines: string[] = [];
    
    lines.push('Node Detection Report');
    lines.push('===================');
    lines.push('');
    
    lines.push('System Information:');
    lines.push(`  OS: ${result.systemInfo.os} (${result.systemInfo.architecture})`);
    lines.push(`  Hostname: ${result.systemInfo.hostname}`);
    lines.push(`  Timezone: ${result.systemInfo.timezone}`);
    lines.push(`  Uptime: ${result.systemInfo.uptime}`);
    lines.push('');
    
    lines.push('Validator Information:');
    lines.push(`  Client Type: ${result.clientType}`);
    if (result.clientVersion) {
      lines.push(`  Version: ${result.clientVersion}`);
    }
    lines.push(`  Running: ${result.validatorRunning ? 'Yes' : 'No'}`);
    lines.push('');
    
    if (result.processes.length > 0) {
      lines.push('Validator Processes:');
      result.processes.forEach(proc => {
        lines.push(`  PID ${proc.pid}: ${proc.command.substring(0, 80)}...`);
      });
      lines.push('');
    }
    
    lines.push('Detected Paths:');
    Object.entries(result.detectedPaths).forEach(([type, path]) => {
      if (path) {
        lines.push(`  ${type}: ${path}`);
      }
    });
    lines.push('');
    
    if (result.warnings.length > 0) {
      lines.push('Warnings:');
      result.warnings.forEach(warning => lines.push(`  ⚠ ${warning}`));
      lines.push('');
    }
    
    if (result.errors.length > 0) {
      lines.push('Errors:');
      result.errors.forEach(error => lines.push(`  ✗ ${error}`));
    }
    
    return lines.join('\n');
  }
}