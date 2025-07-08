import { Command } from 'commander';
import inquirer from 'inquirer';
import chalk from 'chalk';
import Table from 'cli-table3';
import ora from 'ora';
import type { CLIOptions } from '../types/index.js';
import { ConfigManager } from '../utils/config-manager.js';
import { SSHKeyDetector } from '../utils/ssh-key-detector.js';
import { Validator } from '../utils/validator.js';
import { Logger } from '../utils/logger.js';
import type { NodeConfig } from '../types/config.js';

class ConfigCommandHandler {
  private configManager: ConfigManager;
  private sshDetector: SSHKeyDetector;
  private logger: Logger;

  constructor() {
    this.configManager = new ConfigManager();
    this.sshDetector = new SSHKeyDetector();
    this.logger = new Logger();
  }

  async handleCommand(options: CLIOptions): Promise<void> {
    try {
      if (options.list) {
        await this.listConfiguration();
      } else if (options.edit) {
        await this.editConfiguration();
      } else if (options.test) {
        await this.testConnections();
      } else if (options.export) {
        await this.exportConfiguration();
      } else {
        await this.showConfigMenu();
      }
    } catch (error) {
      this.logger.error('Config command failed', {
        error: (error as Error).message,
      });
      console.error(
        chalk.red('❌ Configuration operation failed:'),
        (error as Error).message
      );
      process.exit(1);
    }
  }

  private async showConfigMenu(): Promise<void> {
    const { action } = await inquirer.prompt([
      {
        type: 'list',
        name: 'action',
        message: 'What would you like to do?',
        choices: [
          { name: '📋 List current configuration', value: 'list' },
          { name: '✏️ Edit configuration', value: 'edit' },
          { name: '🔗 Test connections', value: 'test' },
          { name: '📤 Export configuration', value: 'export' },
          { name: '🔄 Reload configuration', value: 'reload' },
          { name: '🧹 Validate configuration', value: 'validate' },
          { name: '🏠 Back to main menu', value: 'back' },
        ],
      },
    ]);

    switch (action) {
      case 'list':
        await this.listConfiguration();
        break;
      case 'edit':
        await this.editConfiguration();
        break;
      case 'test':
        await this.testConnections();
        break;
      case 'export':
        await this.exportConfiguration();
        break;
      case 'reload':
        await this.reloadConfiguration();
        break;
      case 'validate':
        await this.validateConfiguration();
        break;
      case 'back':
        return;
    }
  }

  private async listConfiguration(): Promise<void> {
    const spinner = ora('Loading configuration...').start();

    try {
      const config = await this.configManager.load();
      spinner.succeed('Configuration loaded');

      console.log(chalk.cyan('\n📋 Current Configuration\n'));

      // Basic info table
      const basicTable = new Table({
        head: [chalk.cyan('Property'), chalk.cyan('Value')],
        style: { head: [], border: [] },
      });

      basicTable.push(
        ['Version', config.version],
        ['Config File', this.configManager.getConfigPath()],
        ['SSH Key', config.ssh?.keyPath || 'Not configured'],
        ['SSH Timeout', `${config.ssh?.timeout || 30}s`],
        ['RPC Endpoint', config.rpc?.endpoint || 'Not configured'],
        ['RPC Timeout', `${config.rpc?.timeout || 30000}ms`]
      );

      console.log(basicTable.toString());

      // Nodes table
      if (config.nodes) {
        console.log(chalk.cyan('\n🖥️ Configured Nodes\n'));

        const nodesTable = new Table({
          head: [
            chalk.cyan('Role'),
            chalk.cyan('Label'),
            chalk.cyan('Host'),
            chalk.cyan('Port'),
            chalk.cyan('User'),
          ],
          style: { head: [], border: [] },
        });

        if (config.nodes.primary) {
          nodesTable.push([
            chalk.green('Primary'),
            config.nodes.primary.label,
            config.nodes.primary.host,
            config.nodes.primary.port.toString(),
            config.nodes.primary.user,
          ]);
        }

        if (config.nodes.backup) {
          nodesTable.push([
            chalk.yellow('Backup'),
            config.nodes.backup.label,
            config.nodes.backup.host,
            config.nodes.backup.port.toString(),
            config.nodes.backup.user,
          ]);
        }

        console.log(nodesTable.toString());

        // Show validator paths for each node
        if (config.nodes.primary) {
          console.log(chalk.green('\n🟢 Primary Node Validator Paths\n'));
          const primaryPathsTable = new Table({
            head: [chalk.cyan('Path Type'), chalk.cyan('Location')],
            style: { head: [], border: [] },
          });

          primaryPathsTable.push(
            ['Funded Identity', config.nodes.primary.paths.fundedIdentity],
            ['Unfunded Identity', config.nodes.primary.paths.unfundedIdentity],
            ['Vote Keypair', config.nodes.primary.paths.voteKeypair],
            ['Ledger Directory', config.nodes.primary.paths.ledger],
            ['Tower File', config.nodes.primary.paths.tower],
            ['Solana CLI', config.nodes.primary.paths.solanaCliPath]
          );

          console.log(primaryPathsTable.toString());
        }

        if (config.nodes.backup) {
          console.log(chalk.yellow('\n🟡 Backup Node Validator Paths\n'));
          const backupPathsTable = new Table({
            head: [chalk.cyan('Path Type'), chalk.cyan('Location')],
            style: { head: [], border: [] },
          });

          backupPathsTable.push(
            ['Funded Identity', config.nodes.backup.paths.fundedIdentity],
            ['Unfunded Identity', config.nodes.backup.paths.unfundedIdentity],
            ['Vote Keypair', config.nodes.backup.paths.voteKeypair],
            ['Ledger Directory', config.nodes.backup.paths.ledger],
            ['Tower File', config.nodes.backup.paths.tower],
            ['Solana CLI', config.nodes.backup.paths.solanaCliPath]
          );

          console.log(backupPathsTable.toString());
        }
      }

      // Monitoring settings
      if (config.monitoring) {
        console.log(chalk.cyan('\n📊 Monitoring Settings\n'));

        const monitoringTable = new Table({
          head: [chalk.cyan('Setting'), chalk.cyan('Value')],
          style: { head: [], border: [] },
        });

        monitoringTable.push(
          ['Interval', `${config.monitoring.interval}ms`],
          ['Health Threshold', config.monitoring.healthThreshold.toString()],
          [
            'Readiness Threshold',
            config.monitoring.readinessThreshold.toString(),
          ],
          ['Enable Metrics', config.monitoring.enableMetrics ? '✅' : '❌']
        );

        console.log(monitoringTable.toString());
      }
    } catch (error) {
      spinner.fail('Failed to load configuration');
      throw error;
    }
  }

  private async editConfiguration(): Promise<void> {
    const { section } = await inquirer.prompt([
      {
        type: 'list',
        name: 'section',
        message: 'Which section would you like to edit?',
        choices: [
          { name: '🔑 SSH Configuration', value: 'ssh' },
          { name: '🖥️ Node Configuration', value: 'nodes' },
          { name: '🌐 RPC Settings', value: 'rpc' },
          { name: '📊 Monitoring Settings', value: 'monitoring' },
          { name: '🔒 Security Settings', value: 'security' },
          { name: '🎨 Display Settings', value: 'display' },
          { name: '🔙 Back', value: 'back' },
        ],
      },
    ]);

    if (section === 'back') return;

    switch (section) {
      case 'ssh':
        await this.editSSHConfiguration();
        break;
      case 'nodes':
        await this.editNodeConfiguration();
        break;
      case 'rpc':
        await this.editRPCConfiguration();
        break;
      case 'monitoring':
        await this.editMonitoringConfiguration();
        break;
      case 'security':
        await this.editSecurityConfiguration();
        break;
      case 'display':
        await this.editDisplayConfiguration();
        break;
    }
  }

  private async editSSHConfiguration(): Promise<void> {
    const config = await this.configManager.load();

    // Detect available SSH keys
    const sshKeysResult = await this.sshDetector.detectKeys();
    const keyChoices = sshKeysResult.keys.map(key => ({
      name: `${key.type.toUpperCase()} - ${key.path} ${key.comment ? '(' + key.comment + ')' : ''}`,
      value: key.path,
    }));
    keyChoices.push({ name: '📝 Enter custom path', value: 'custom' });

    const sshConfig = await inquirer.prompt([
      {
        type: 'list',
        name: 'keyPath',
        message: 'SSH private key:',
        choices: keyChoices,
        default: config.ssh.keyPath,
      },
      {
        type: 'number',
        name: 'timeout',
        message: 'SSH timeout (seconds):',
        default: config.ssh.timeout || 30,
        validate: (input: number) => {
          if (!input || input < 5 || input > 300) {
            return 'Timeout must be between 5 and 300 seconds';
          }
          return true;
        },
      },
    ]);

    if (sshConfig.keyPath === 'custom') {
      const { customKeyPath } = await inquirer.prompt([
        {
          type: 'input',
          name: 'customKeyPath',
          message: 'Enter SSH private key path:',
          default: config.ssh.keyPath,
          validate: (input: string) => {
            if (!input.trim()) return 'SSH key path is required';
            return true;
          },
        },
      ]);
      sshConfig.keyPath = customKeyPath;
    }

    config.ssh = sshConfig;
    await this.configManager.save(config);
    console.log(chalk.green('✅ SSH configuration saved successfully!'));
  }

  private async editNodeConfiguration(): Promise<void> {
    const config = await this.configManager.load();

    const { nodeType } = await inquirer.prompt([
      {
        type: 'list',
        name: 'nodeType',
        message: 'Which node would you like to edit?',
        choices: [
          { name: '🟢 Primary Node', value: 'primary' },
          { name: '🟡 Backup Node', value: 'backup' },
          { name: '🔙 Back', value: 'back' },
        ],
      },
    ]);

    if (nodeType === 'back') return;

    const currentNode = config.nodes?.[nodeType as 'primary' | 'backup'];

    const nodeConfig = await inquirer.prompt([
      {
        type: 'input',
        name: 'label',
        message: 'Node label:',
        default: currentNode?.label || `${nodeType} validator`,
      },
      {
        type: 'input',
        name: 'host',
        message: 'Host (IP or hostname):',
        default: currentNode?.host,
        validate: (input: string) => {
          if (!input.trim()) return 'Host is required';
          return true;
        },
      },
      {
        type: 'number',
        name: 'port',
        message: 'SSH port:',
        default: currentNode?.port || 22,
        validate: (input: number) => {
          if (!input || input < 1 || input > 65535) {
            return 'Port must be between 1 and 65535';
          }
          return true;
        },
      },
      {
        type: 'input',
        name: 'user',
        message: 'SSH user:',
        default: currentNode?.user || 'solana',
        validate: (input: string) => {
          if (!input.trim()) return 'User is required';
          return true;
        },
      },
    ]);

    // Get path configuration
    const pathConfig = await inquirer.prompt([
      {
        type: 'input',
        name: 'fundedIdentity',
        message: 'Funded identity keypair path:',
        default:
          currentNode?.paths?.fundedIdentity ||
          '/home/solana/funded-validator-keypair.json',
      },
      {
        type: 'input',
        name: 'unfundedIdentity',
        message: 'Unfunded identity keypair path:',
        default:
          currentNode?.paths?.unfundedIdentity ||
          '/home/solana/unfunded-validator-keypair.json',
      },
      {
        type: 'input',
        name: 'ledger',
        message: 'Ledger directory path:',
        default: currentNode?.paths?.ledger || '/mnt/ledger',
      },
      {
        type: 'input',
        name: 'tower',
        message: 'Tower file path:',
        default: currentNode?.paths?.tower || '/mnt/ledger/tower-1_9-*.bin',
      },
      {
        type: 'input',
        name: 'solanaCliPath',
        message: 'Solana CLI path:',
        default:
          currentNode?.paths?.solanaCliPath ||
          '/home/solana/.local/share/solana/install/active_release/bin/solana',
      },
    ]);

    const newNodeConfig: NodeConfig = {
      ...nodeConfig,
      paths: pathConfig,
    };

    // Update configuration
    if (!config.nodes) config.nodes = {} as any;
    (config.nodes as any)[nodeType] = newNodeConfig;

    await this.configManager.save(config);
    console.log(
      chalk.green(`✅ ${nodeType} node configuration saved successfully!`)
    );
  }

  private async editRPCConfiguration(): Promise<void> {
    const config = await this.configManager.load();

    const rpcConfig = await inquirer.prompt([
      {
        type: 'input',
        name: 'endpoint',
        message: 'RPC endpoint URL:',
        default: config.rpc?.endpoint || 'https://api.mainnet-beta.solana.com',
        validate: (input: string) => {
          if (!input.trim()) return 'RPC endpoint is required';
          try {
            new URL(input);
            return true;
          } catch {
            return 'Please enter a valid URL';
          }
        },
      },
      {
        type: 'number',
        name: 'timeout',
        message: 'Request timeout (ms):',
        default: config.rpc?.timeout || 30000,
        validate: (input: number) => {
          if (!input || input < 1000 || input > 120000) {
            return 'Timeout must be between 1000ms and 120000ms';
          }
          return true;
        },
      },
    ]);

    config.rpc = rpcConfig;
    await this.configManager.save(config);
    console.log(chalk.green('✅ RPC configuration saved successfully!'));
  }

  private async editMonitoringConfiguration(): Promise<void> {
    const config = await this.configManager.load();

    const monitoringConfig = await inquirer.prompt([
      {
        type: 'number',
        name: 'interval',
        message: 'Monitoring interval (ms):',
        default: config.monitoring?.interval || 5000,
        validate: (input: number) => {
          if (!input || input < 1000 || input > 60000) {
            return 'Interval must be between 1000ms and 60000ms';
          }
          return true;
        },
      },
      {
        type: 'number',
        name: 'healthThreshold',
        message: 'Health threshold (vote distance):',
        default: config.monitoring?.healthThreshold || 100,
        validate: (input: number) => {
          if (!input || input < 1 || input > 1000) {
            return 'Health threshold must be between 1 and 1000';
          }
          return true;
        },
      },
      {
        type: 'number',
        name: 'readinessThreshold',
        message: 'Readiness threshold (slots behind):',
        default: config.monitoring?.readinessThreshold || 50,
        validate: (input: number) => {
          if (!input || input < 1 || input > 500) {
            return 'Readiness threshold must be between 1 and 500';
          }
          return true;
        },
      },
      {
        type: 'confirm',
        name: 'enableMetrics',
        message: 'Enable metrics collection?',
        default: config.monitoring?.enableMetrics ?? true,
      },
    ]);

    config.monitoring = monitoringConfig;
    await this.configManager.save(config);
    console.log(chalk.green('✅ Monitoring configuration saved successfully!'));
  }

  private async editSecurityConfiguration(): Promise<void> {
    const config = await this.configManager.load();

    const securityConfig = await inquirer.prompt([
      {
        type: 'confirm',
        name: 'confirmSwitches',
        message: 'Require confirmation for validator switches?',
        default: config.security?.confirmSwitches ?? true,
      },
      {
        type: 'number',
        name: 'maxRetries',
        message: 'Maximum retry attempts:',
        default: config.security?.maxRetries || 3,
        validate: (input: number) => {
          if (!input || input < 1 || input > 10) {
            return 'Max retries must be between 1 and 10';
          }
          return true;
        },
      },
    ]);

    config.security = securityConfig;
    await this.configManager.save(config);
    console.log(chalk.green('✅ Security configuration saved successfully!'));
  }

  private async editDisplayConfiguration(): Promise<void> {
    const config = await this.configManager.load();

    const displayConfig = await inquirer.prompt([
      {
        type: 'confirm',
        name: 'compact',
        message: 'Use compact display mode?',
        default: config.display?.compact ?? true,
      },
      {
        type: 'confirm',
        name: 'showTechnicalDetails',
        message: 'Show technical details in output?',
        default: config.display?.showTechnicalDetails ?? false,
      },
    ]);

    // Force theme to dark as per setup simplification
    config.display = {
      theme: 'dark',
      ...displayConfig,
    };
    await this.configManager.save(config);
    console.log(chalk.green('✅ Display configuration saved successfully!'));
  }

  private async testConnections(): Promise<void> {
    const spinner = ora('Testing connections...').start();

    try {
      const config = await this.configManager.load();

      if (!config.nodes?.primary && !config.nodes?.backup) {
        spinner.fail('No nodes configured');
        console.log(
          chalk.yellow('⚠️ No nodes are configured. Run setup first.')
        );
        return;
      }

      const results: Array<{ node: string; success: boolean; error?: string }> =
        [];

      // Test primary node
      if (config.nodes.primary) {
        spinner.text = 'Testing primary node connection...';
        try {
          const result = await this.sshDetector.testConnection(
            config.nodes.primary.host,
            config.nodes.primary.port,
            config.nodes.primary.user,
            config.ssh.keyPath
          );
          const resultObj: any = {
            node: `Primary (${config.nodes.primary.label})`,
            success: result.success,
          };
          if (result.error) {
            resultObj.error = result.error;
          }
          results.push(resultObj);
        } catch (error) {
          results.push({
            node: `Primary (${config.nodes.primary.label})`,
            success: false,
            error: (error as Error).message,
          });
        }
      }

      // Test backup node
      if (config.nodes.backup) {
        spinner.text = 'Testing backup node connection...';
        try {
          const result = await this.sshDetector.testConnection(
            config.nodes.backup.host,
            config.nodes.backup.port,
            config.nodes.backup.user,
            config.ssh.keyPath
          );
          const resultObj: any = {
            node: `Backup (${config.nodes.backup.label})`,
            success: result.success,
          };
          if (result.error) {
            resultObj.error = result.error;
          }
          results.push(resultObj);
        } catch (error) {
          results.push({
            node: `Backup (${config.nodes.backup.label})`,
            success: false,
            error: (error as Error).message,
          });
        }
      }

      spinner.succeed('Connection tests completed');

      console.log(chalk.cyan('\n🔗 Connection Test Results\n'));

      const resultsTable = new Table({
        head: [chalk.cyan('Node'), chalk.cyan('Status'), chalk.cyan('Details')],
        style: { head: [], border: [] },
      });

      results.forEach(result => {
        resultsTable.push([
          result.node,
          result.success ? chalk.green('✅ Connected') : chalk.red('❌ Failed'),
          result.error
            ? chalk.red(result.error)
            : chalk.green('Connection successful'),
        ]);
      });

      console.log(resultsTable.toString());
    } catch (error) {
      spinner.fail('Connection test failed');
      throw error;
    }
  }

  private async exportConfiguration(): Promise<void> {
    const spinner = ora('Exporting configuration...').start();

    try {
      const config = await this.configManager.load();

      // Remove sensitive information for export
      const exportConfig = JSON.parse(JSON.stringify(config));

      // Optionally redact SSH key paths
      const { includeSensitive } = await inquirer.prompt([
        {
          type: 'confirm',
          name: 'includeSensitive',
          message:
            'Include SSH key paths in export? (Not recommended for sharing)',
          default: false,
        },
      ]);

      if (!includeSensitive) {
        if (exportConfig.nodes?.primary?.keyPath) {
          exportConfig.nodes.primary.keyPath = '[REDACTED]';
        }
        if (exportConfig.nodes?.backup?.keyPath) {
          exportConfig.nodes.backup.keyPath = '[REDACTED]';
        }
      }

      spinner.succeed('Configuration exported');

      console.log(chalk.cyan('\n📤 Exported Configuration\n'));
      console.log(JSON.stringify(exportConfig, null, 2));
    } catch (error) {
      spinner.fail('Export failed');
      throw error;
    }
  }

  private async reloadConfiguration(): Promise<void> {
    const spinner = ora('Reloading configuration...').start();

    try {
      await this.configManager.load();
      spinner.succeed('Configuration reloaded successfully');
      console.log(chalk.green('✅ Configuration reloaded from disk'));
    } catch (error) {
      spinner.fail('Failed to reload configuration');
      throw error;
    }
  }

  private async validateConfiguration(): Promise<void> {
    const spinner = ora('Validating configuration...').start();

    try {
      const config = await this.configManager.load();
      const result = Validator.validateConfig(config);

      spinner.succeed('Validation completed');

      console.log(chalk.cyan('\n🧹 Configuration Validation Results\n'));

      if (result.errors.length === 0) {
        console.log(chalk.green('✅ Configuration is valid!'));
      } else {
        console.log(chalk.red(`❌ Found ${result.errors.length} error(s):`));
        result.errors.forEach((error: any) => {
          console.log(chalk.red(`  • ${error.message || error}`));
        });
      }

      if (result.warnings.length > 0) {
        console.log(
          chalk.yellow(`\n⚠️ Found ${result.warnings.length} warning(s):`)
        );
        result.warnings.forEach((warning: any) => {
          console.log(chalk.yellow(`  • ${warning.message || warning}`));
        });
      }
    } catch (error) {
      spinner.fail('Validation failed');
      throw error;
    }
  }
}

const handler = new ConfigCommandHandler();

export const configCommand = new Command('config')
  .description('Manage configuration settings')
  .option('-l, --list', 'list current configuration')
  .option('-e, --edit', 'edit configuration file')
  .option('-t, --test', 'test connections to configured nodes')
  .option('--export', 'export configuration to stdout')
  .action(async (options: CLIOptions) => {
    await handler.handleCommand(options);
  });
