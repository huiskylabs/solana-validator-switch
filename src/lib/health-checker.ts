// Health Checker - comprehensive node health monitoring and validation

import type { NodeConfig } from '../types/config.js';
import { SSHManager } from './ssh-manager.js';
import { Logger } from '../utils/logger.js';

export interface HealthCheckResult {
  nodeId: string;
  timestamp: Date;
  overall: HealthStatus;
  checks: {
    connectivity: HealthCheck;
    diskSpace: HealthCheck;
    memoryUsage: HealthCheck;
    cpuLoad: HealthCheck;
    validatorProcess: HealthCheck;
    ledgerHealth: HealthCheck;
    identityFiles: HealthCheck;
    towerFile: HealthCheck;
    solanaCliAccess: HealthCheck;
  };
  metrics: HealthMetrics;
  warnings: string[];
  errors: string[];
}

export interface HealthCheck {
  status: HealthStatus;
  value?: number | string | boolean;
  threshold?: number | string;
  message: string;
  details?: Record<string, unknown>;
}

export interface HealthMetrics {
  diskUsagePercent: number;
  memoryUsagePercent: number;
  cpuLoadAverage: number;
  uptimeHours: number;
  ledgerSlots: number;
  processCount: number;
  networkLatency?: number;
}

export type HealthStatus = 'healthy' | 'warning' | 'critical' | 'unknown';

export interface HealthThresholds {
  diskUsageWarning: number;      // % - warn above this
  diskUsageCritical: number;     // % - critical above this
  memoryUsageWarning: number;    // % - warn above this
  memoryUsageCritical: number;   // % - critical above this
  cpuLoadWarning: number;        // load average - warn above this
  cpuLoadCritical: number;       // load average - critical above this
  uptimeMinimum: number;         // hours - warn below this
}

export const DEFAULT_HEALTH_THRESHOLDS: HealthThresholds = {
  diskUsageWarning: 80,
  diskUsageCritical: 90,
  memoryUsageWarning: 85,
  memoryUsageCritical: 95,
  cpuLoadWarning: 8.0,
  cpuLoadCritical: 16.0,
  uptimeMinimum: 1.0
};

export class HealthCheckService {
  private sshManager: SSHManager;
  private logger: Logger;
  private thresholds: HealthThresholds;

  constructor(sshManager: SSHManager, thresholds: Partial<HealthThresholds> = {}) {
    this.sshManager = sshManager;
    this.logger = new Logger({ level: 'info' });
    this.thresholds = { ...DEFAULT_HEALTH_THRESHOLDS, ...thresholds };
  }

  /**
   * Run comprehensive health check on a node
   */
  async checkNodeHealth(connectionId: string, nodeConfig: NodeConfig): Promise<HealthCheckResult> {
    this.logger.info(`Running health check for ${connectionId}`);
    
    const result: HealthCheckResult = {
      nodeId: connectionId,
      timestamp: new Date(),
      overall: 'unknown',
      checks: {
        connectivity: { status: 'unknown', message: 'Not checked' },
        diskSpace: { status: 'unknown', message: 'Not checked' },
        memoryUsage: { status: 'unknown', message: 'Not checked' },
        cpuLoad: { status: 'unknown', message: 'Not checked' },
        validatorProcess: { status: 'unknown', message: 'Not checked' },
        ledgerHealth: { status: 'unknown', message: 'Not checked' },
        identityFiles: { status: 'unknown', message: 'Not checked' },
        towerFile: { status: 'unknown', message: 'Not checked' },
        solanaCliAccess: { status: 'unknown', message: 'Not checked' }
      },
      metrics: {
        diskUsagePercent: 0,
        memoryUsagePercent: 0,
        cpuLoadAverage: 0,
        uptimeHours: 0,
        ledgerSlots: 0,
        processCount: 0
      },
      warnings: [],
      errors: []
    };

    try {
      // Run all health checks
      await Promise.all([
        this.checkConnectivity(connectionId, result),
        this.checkDiskSpace(connectionId, result),
        this.checkMemoryUsage(connectionId, result),
        this.checkCpuLoad(connectionId, result),
        this.checkValidatorProcess(connectionId, result),
        this.checkLedgerHealth(connectionId, nodeConfig, result),
        this.checkIdentityFiles(connectionId, nodeConfig, result),
        this.checkTowerFile(connectionId, nodeConfig, result),
        this.checkSolanaCliAccess(connectionId, nodeConfig, result)
      ]);

      // Calculate overall health status
      result.overall = this.calculateOverallHealth(result);

    } catch (error) {
      this.logger.error('Health check failed:', { error: String(error) });
      result.errors.push(`Health check failed: ${error}`);
      result.overall = 'critical';
    }

    return result;
  }

  /**
   * Check SSH connectivity
   */
  private async checkConnectivity(connectionId: string, result: HealthCheckResult): Promise<void> {
    try {
      const startTime = Date.now();
      await this.sshManager.executeCommand(connectionId, 'echo "connectivity_test"');
      const latency = Date.now() - startTime;
      
      result.metrics.networkLatency = latency;
      result.checks.connectivity = {
        status: latency < 1000 ? 'healthy' : latency < 3000 ? 'warning' : 'critical',
        value: latency,
        threshold: '< 1000ms',
        message: `SSH connection responsive (${latency}ms)`,
        details: { latency }
      };
    } catch (error) {
      result.checks.connectivity = {
        status: 'critical',
        message: `SSH connection failed: ${error}`,
        details: { error: String(error) }
      };
    }
  }

  /**
   * Check disk space usage
   */
  private async checkDiskSpace(connectionId: string, result: HealthCheckResult): Promise<void> {
    try {
      const diskResult = await this.sshManager.executeCommand(
        connectionId,
        'df -h / | tail -1 | awk \'{print $5}\' | sed \'s/%//\''
      );
      
      const diskUsage = parseInt(diskResult.stdout.trim());
      result.metrics.diskUsagePercent = diskUsage;
      
      let status: HealthStatus = 'healthy';
      if (diskUsage >= this.thresholds.diskUsageCritical) {
        status = 'critical';
      } else if (diskUsage >= this.thresholds.diskUsageWarning) {
        status = 'warning';
      }
      
      result.checks.diskSpace = {
        status,
        value: diskUsage,
        threshold: `< ${this.thresholds.diskUsageWarning}%`,
        message: `Disk usage: ${diskUsage}%`,
        details: { usage: diskUsage, warningThreshold: this.thresholds.diskUsageWarning }
      };
      
    } catch (error) {
      result.checks.diskSpace = {
        status: 'unknown',
        message: `Failed to check disk space: ${error}`
      };
    }
  }

  /**
   * Check memory usage
   */
  private async checkMemoryUsage(connectionId: string, result: HealthCheckResult): Promise<void> {
    try {
      const memResult = await this.sshManager.executeCommand(
        connectionId,
        'free | grep Mem | awk \'{printf "%.1f", $3/$2 * 100.0}\''
      );
      
      const memUsage = parseFloat(memResult.stdout.trim());
      result.metrics.memoryUsagePercent = memUsage;
      
      let status: HealthStatus = 'healthy';
      if (memUsage >= this.thresholds.memoryUsageCritical) {
        status = 'critical';
      } else if (memUsage >= this.thresholds.memoryUsageWarning) {
        status = 'warning';
      }
      
      result.checks.memoryUsage = {
        status,
        value: memUsage,
        threshold: `< ${this.thresholds.memoryUsageWarning}%`,
        message: `Memory usage: ${memUsage.toFixed(1)}%`,
        details: { usage: memUsage }
      };
      
    } catch (error) {
      result.checks.memoryUsage = {
        status: 'unknown',
        message: `Failed to check memory usage: ${error}`
      };
    }
  }

  /**
   * Check CPU load average
   */
  private async checkCpuLoad(connectionId: string, result: HealthCheckResult): Promise<void> {
    try {
      const loadResult = await this.sshManager.executeCommand(
        connectionId,
        'uptime | awk -F\'load average:\' \'{ print $2 }\' | awk -F\', \' \'{ print $1 }\' | tr -d \' \''
      );
      
      const cpuLoad = parseFloat(loadResult.stdout.trim());
      result.metrics.cpuLoadAverage = cpuLoad;
      
      let status: HealthStatus = 'healthy';
      if (cpuLoad >= this.thresholds.cpuLoadCritical) {
        status = 'critical';
      } else if (cpuLoad >= this.thresholds.cpuLoadWarning) {
        status = 'warning';
      }
      
      result.checks.cpuLoad = {
        status,
        value: cpuLoad,
        threshold: `< ${this.thresholds.cpuLoadWarning}`,
        message: `CPU load average: ${cpuLoad.toFixed(2)}`,
        details: { load: cpuLoad }
      };
      
    } catch (error) {
      result.checks.cpuLoad = {
        status: 'unknown',
        message: `Failed to check CPU load: ${error}`
      };
    }
  }

  /**
   * Check validator process status
   */
  private async checkValidatorProcess(connectionId: string, result: HealthCheckResult): Promise<void> {
    try {
      const processResult = await this.sshManager.executeCommand(
        connectionId,
        'ps aux | grep -E "(solana-validator|jito-validator|firedancer)" | grep -v grep | wc -l'
      );
      
      const processCount = parseInt(processResult.stdout.trim());
      result.metrics.processCount = processCount;
      
      let status: HealthStatus;
      let message: string;
      
      if (processCount === 0) {
        status = 'critical';
        message = 'No validator processes running';
      } else if (processCount === 1) {
        status = 'healthy';
        message = 'Validator process running normally';
      } else {
        status = 'warning';
        message = `Multiple validator processes detected (${processCount})`;
      }
      
      result.checks.validatorProcess = {
        status,
        value: processCount,
        message,
        details: { processCount }
      };
      
    } catch (error) {
      result.checks.validatorProcess = {
        status: 'unknown',
        message: `Failed to check validator process: ${error}`
      };
    }
  }

  /**
   * Check ledger health
   */
  private async checkLedgerHealth(
    connectionId: string, 
    nodeConfig: NodeConfig, 
    result: HealthCheckResult
  ): Promise<void> {
    try {
      if (!nodeConfig.paths.ledger) {
        result.checks.ledgerHealth = {
          status: 'warning',
          message: 'Ledger path not configured'
        };
        return;
      }

      // Check if ledger directory exists and has recent activity
      const ledgerCheck = await this.sshManager.executeCommand(
        connectionId,
        `test -d "${nodeConfig.paths.ledger}" && find "${nodeConfig.paths.ledger}" -name "*.sst" | wc -l`
      );
      
      const slotFiles = parseInt(ledgerCheck.stdout.trim()) || 0;
      result.metrics.ledgerSlots = slotFiles;
      
      let status: HealthStatus = 'healthy';
      let message = `Ledger directory healthy (${slotFiles} slot files)`;
      
      if (slotFiles === 0) {
        status = 'critical';
        message = 'Ledger directory appears empty or corrupted';
      } else if (slotFiles < 100) {
        status = 'warning';
        message = `Low number of slot files in ledger (${slotFiles})`;
      }
      
      result.checks.ledgerHealth = {
        status,
        value: slotFiles,
        message,
        details: { slotFiles, ledgerPath: nodeConfig.paths.ledger }
      };
      
    } catch (error) {
      result.checks.ledgerHealth = {
        status: 'unknown',
        message: `Failed to check ledger: ${error}`
      };
    }
  }

  /**
   * Check identity files
   */
  private async checkIdentityFiles(
    connectionId: string, 
    nodeConfig: NodeConfig, 
    result: HealthCheckResult
  ): Promise<void> {
    try {
      const filesToCheck = [
        nodeConfig.paths.fundedIdentity,
        nodeConfig.paths.unfundedIdentity,
        nodeConfig.paths.voteKeypair
      ];
      
      const fileStatuses: string[] = [];
      let criticalMissing = false;
      
      for (const filePath of filesToCheck) {
        if (!filePath) continue;
        
        try {
          const fileCheck = await this.sshManager.executeCommand(
            connectionId,
            `test -r "${filePath}" && echo "OK" || echo "MISSING"`
          );
          
          if (fileCheck.stdout.includes('OK')) {
            fileStatuses.push(`${filePath}: OK`);
          } else {
            fileStatuses.push(`${filePath}: MISSING`);
            if (filePath === nodeConfig.paths.fundedIdentity || filePath === nodeConfig.paths.voteKeypair) {
              criticalMissing = true;
            }
          }
        } catch (error) {
          fileStatuses.push(`${filePath}: ERROR`);
          criticalMissing = true;
        }
      }
      
      result.checks.identityFiles = {
        status: criticalMissing ? 'critical' : 'healthy',
        message: criticalMissing ? 'Critical identity files missing' : 'All identity files accessible',
        details: { files: fileStatuses }
      };
      
    } catch (error) {
      result.checks.identityFiles = {
        status: 'unknown',
        message: `Failed to check identity files: ${error}`
      };
    }
  }

  /**
   * Check tower file
   */
  private async checkTowerFile(
    connectionId: string, 
    nodeConfig: NodeConfig, 
    result: HealthCheckResult
  ): Promise<void> {
    try {
      if (!nodeConfig.paths.tower) {
        result.checks.towerFile = {
          status: 'warning',
          message: 'Tower file path not configured'
        };
        return;
      }

      const towerCheck = await this.sshManager.executeCommand(
        connectionId,
        `test -f "${nodeConfig.paths.tower}" && stat -c%s "${nodeConfig.paths.tower}" || echo "0"`
      );
      
      const towerSize = parseInt(towerCheck.stdout.trim()) || 0;
      
      let status: HealthStatus = 'healthy';
      let message = `Tower file present (${towerSize} bytes)`;
      
      if (towerSize === 0) {
        status = 'warning';
        message = 'Tower file missing or empty';
      }
      
      result.checks.towerFile = {
        status,
        value: towerSize,
        message,
        details: { size: towerSize, path: nodeConfig.paths.tower }
      };
      
    } catch (error) {
      result.checks.towerFile = {
        status: 'unknown',
        message: `Failed to check tower file: ${error}`
      };
    }
  }

  /**
   * Check Solana CLI access
   */
  private async checkSolanaCliAccess(
    connectionId: string, 
    nodeConfig: NodeConfig, 
    result: HealthCheckResult
  ): Promise<void> {
    try {
      const cliPath = nodeConfig.paths.solanaCliPath || 'solana';
      const versionResult = await this.sshManager.executeCommand(
        connectionId,
        `${cliPath} --version 2>/dev/null`
      );
      
      if (versionResult.stdout.includes('solana-cli')) {
        result.checks.solanaCliAccess = {
          status: 'healthy',
          value: versionResult.stdout.trim(),
          message: 'Solana CLI accessible',
          details: { version: versionResult.stdout.trim(), path: cliPath }
        };
      } else {
        result.checks.solanaCliAccess = {
          status: 'warning',
          message: 'Solana CLI not accessible or invalid',
          details: { path: cliPath }
        };
      }
      
    } catch (error) {
      result.checks.solanaCliAccess = {
        status: 'warning',
        message: `Solana CLI check failed: ${error}`
      };
    }
  }

  /**
   * Calculate overall health status based on individual checks
   */
  private calculateOverallHealth(result: HealthCheckResult): HealthStatus {
    const checkStatuses = Object.values(result.checks).map(check => check.status);
    
    if (checkStatuses.includes('critical')) {
      return 'critical';
    } else if (checkStatuses.includes('warning')) {
      return 'warning';
    } else if (checkStatuses.some(status => status === 'unknown')) {
      return 'warning';
    } else {
      return 'healthy';
    }
  }

  /**
   * Generate a human-readable health report
   */
  generateHealthReport(result: HealthCheckResult): string {
    const lines: string[] = [];
    
    lines.push(`Health Check Report for ${result.nodeId}`);
    lines.push(`Generated: ${result.timestamp.toISOString()}`);
    lines.push(`Overall Status: ${result.overall.toUpperCase()}`);
    lines.push('');
    
    lines.push('System Metrics:');
    lines.push(`  Disk Usage: ${result.metrics.diskUsagePercent}%`);
    lines.push(`  Memory Usage: ${result.metrics.memoryUsagePercent.toFixed(1)}%`);
    lines.push(`  CPU Load: ${result.metrics.cpuLoadAverage.toFixed(2)}`);
    if (result.metrics.networkLatency) {
      lines.push(`  Network Latency: ${result.metrics.networkLatency}ms`);
    }
    lines.push('');
    
    lines.push('Health Checks:');
    Object.entries(result.checks).forEach(([checkName, check]) => {
      const statusIcon = this.getStatusIcon(check.status);
      lines.push(`  ${statusIcon} ${checkName}: ${check.message}`);
    });
    
    if (result.warnings.length > 0) {
      lines.push('');
      lines.push('Warnings:');
      result.warnings.forEach(warning => lines.push(`  ⚠ ${warning}`));
    }
    
    if (result.errors.length > 0) {
      lines.push('');
      lines.push('Errors:');
      result.errors.forEach(error => lines.push(`  ✗ ${error}`));
    }
    
    return lines.join('\n');
  }

  private getStatusIcon(status: HealthStatus): string {
    switch (status) {
      case 'healthy': return '✓';
      case 'warning': return '⚠';
      case 'critical': return '✗';
      default: return '?';
    }
  }
}