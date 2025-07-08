import { Command } from 'commander';
import inquirer from 'inquirer';
import chalk from 'chalk';
import ora from 'ora';
import figlet from 'figlet';
import type { CLIOptions } from '../types/index.js';
import { ConfigManager } from '../utils/config-manager.js';
import { SSHKeyDetector } from '../utils/ssh-key-detector.js';
import { Validator } from '../utils/validator.js';
import { Logger } from '../utils/logger.js';
import type { Config, NodeConfig } from '../types/config.js';

class SetupWizard {
  private configManager: ConfigManager;
  private sshDetector: SSHKeyDetector;
  private logger: Logger;

  constructor() {
    this.configManager = new ConfigManager();
    this.sshDetector = new SSHKeyDetector();
    this.logger = new Logger();
  }

  async runSetup(options: CLIOptions): Promise<void> {
    try {
      await this.displayWelcome();

      // Check if configuration already exists
      const configExists = await this.configManager.exists();
      if (configExists && !options.force) {
        const { proceed } = await inquirer.prompt([
          {
            type: 'confirm',
            name: 'proceed',
            message:
              'Configuration already exists. Do you want to overwrite it?',
            default: false,
          },
        ]);

        if (!proceed) {
          console.log(
            chalk.yellow(
              '⚠️ Setup cancelled. Use --force to overwrite existing configuration.'
            )
          );
          return;
        }
      }

      console.log(chalk.cyan('\n🚀 Starting Solana Validator Switch Setup\n'));
      console.log(chalk.gray('This setup will configure:'));
      console.log(chalk.gray('  1. SSH connection settings'));
      console.log(chalk.gray('  2. Primary and backup validator nodes'));
      console.log(chalk.gray('  3. RPC endpoint'));
      console.log(
        chalk.gray('  4. Default monitoring, security, and display settings\n')
      );

      // Detect SSH keys
      const sshKeysResult = await this.detectSSHKeys();

      // SSH configuration (global for CLI machine)
      const sshConfig = await this.collectSSHConfiguration(sshKeysResult.keys);

      // Basic configuration
      const basicConfig = await this.collectBasicConfiguration();

      // Node configuration (without SSH keys - they're global now)
      const nodesConfig = await this.collectNodesConfiguration();

      // RPC configuration
      const rpcConfig = await this.collectRPCConfiguration();

      // Use simple default configurations
      const monitoringConfig = this.getDefaultMonitoringConfig();
      const securityConfig = this.getDefaultSecurityConfig();
      const displayConfig = this.getDefaultDisplayConfig();

      // Build final configuration
      const config: Config = {
        version: basicConfig.version,
        ssh: sshConfig,
        nodes: nodesConfig,
        rpc: { ...rpcConfig, retries: 3 },
        monitoring: monitoringConfig,
        security: securityConfig,
        display: displayConfig,
      };

      // Validate configuration
      await this.validateAndSaveConfiguration(config);

      // Test connections
      await this.testInitialConnections(config);

      await this.displayCompletion();
    } catch (error) {
      this.logger.error('Setup failed', { error: (error as Error).message });
      console.error(chalk.red('❌ Setup failed:'), (error as Error).message);
      process.exit(1);
    }
  }

  private async displayWelcome(): Promise<void> {
    console.clear();

    // Display ASCII art banner
    try {
      const banner = figlet.textSync('SVS Setup', {
        font: 'Small',
        horizontalLayout: 'default',
        verticalLayout: 'default',
      });
      console.log(chalk.cyan(banner));
    } catch {
      console.log(chalk.cyan('🚀 Solana Validator Switch Setup'));
    }

    console.log(
      chalk.gray('Professional-grade validator switching for Solana\n')
    );

    console.log(chalk.yellow('⚠️  Important Security Notes:'));
    console.log(
      chalk.yellow('   • This tool stores SSH key file paths in configuration')
    );
    console.log(
      chalk.yellow('   • SSH private keys remain in your ~/.ssh/ directory')
    );
    console.log(
      chalk.yellow(
        '   • No passwords or key contents are stored in config files'
      )
    );
    console.log(
      chalk.yellow('   • All connections use your existing SSH key files')
    );
    console.log(
      chalk.yellow(
        '   • Configuration files contain file paths and hostnames\n'
      )
    );

    const { readyToProceed } = await inquirer.prompt([
      {
        type: 'confirm',
        name: 'readyToProceed',
        message: 'Ready to begin setup?',
        default: true,
      },
    ]);

    if (!readyToProceed) {
      console.log(chalk.yellow('Setup cancelled.'));
      process.exit(0);
    }
  }

  private async detectSSHKeys(): Promise<{ keys: any[] }> {
    const spinner = ora('Detecting SSH keys...').start();

    try {
      const sshKeysResult = await this.sshDetector.detectKeys();

      if (sshKeysResult.keys.length === 0) {
        spinner.fail('No SSH keys found');
        console.log(chalk.red('\n❌ No SSH keys detected in ~/.ssh/'));
        console.log(chalk.yellow('Please generate SSH keys first:\n'));
        console.log(
          chalk.gray('  ssh-keygen -t ed25519 -C "your_email@example.com"')
        );
        console.log(chalk.gray('  ssh-copy-id user@validator-host\n'));
        process.exit(1);
      }

      spinner.succeed(`Found ${sshKeysResult.keys.length} SSH key(s)`);

      if (sshKeysResult.warnings.length > 0) {
        console.log(chalk.yellow('\n⚠️ SSH Key Warnings:'));
        sshKeysResult.warnings.forEach(warning => {
          console.log(chalk.yellow(`  • ${warning}`));
        });
      }

      // Show detected keys
      console.log(chalk.cyan('\n🔑 Detected SSH Keys:'));
      sshKeysResult.keys.forEach((key, index) => {
        const status = key.valid ? chalk.green('✅') : chalk.red('❌');
        const type = key.type.toUpperCase().padEnd(8);
        const bits = key.bits ? `${key.bits} bits` : '';
        const comment = key.comment ? `(${key.comment})` : '';
        console.log(
          `  ${index + 1}. ${status} ${type} ${bits} ${key.path} ${comment}`
        );
      });

      return sshKeysResult;
    } catch (error) {
      spinner.fail('Failed to detect SSH keys');
      throw error;
    }
  }

  private async collectSSHConfiguration(sshKeys: any[]): Promise<any> {
    console.log(chalk.cyan('\n🔑 SSH Configuration\n'));
    console.log(
      chalk.gray('Configure SSH access for connecting to validator nodes.\n')
    );

    // Prepare SSH key choices
    const keyChoices = sshKeys.map(key => ({
      name: `${key.type.toUpperCase()} - ${key.path} ${key.comment ? '(' + key.comment + ')' : ''}`,
      value: key.path,
    }));
    keyChoices.push({ name: '📝 Enter custom path', value: 'custom' });

    // Get recommended key
    const recommendedKey = this.sshDetector.getRecommendedKey(sshKeys);
    const defaultKeyPath =
      recommendedKey?.path || (sshKeys.length > 0 ? sshKeys[0].path : '');

    if (recommendedKey) {
      console.log(
        chalk.green(`✨ Recommended SSH key: ${recommendedKey.path}`)
      );
    }

    const sshConfig = await inquirer.prompt([
      {
        type: 'list',
        name: 'keyPath',
        message: 'SSH private key for validator connections:',
        choices: keyChoices,
        default: defaultKeyPath,
      },
      {
        type: 'number',
        name: 'timeout',
        message: 'SSH connection timeout (seconds):',
        default: 30,
        validate: (input: number) => {
          if (!input || input < 5 || input > 300) {
            return 'Timeout must be between 5 and 300 seconds';
          }
          return true;
        },
      },
    ]);

    // Handle custom key path
    if (sshConfig.keyPath === 'custom') {
      const { customKeyPath } = await inquirer.prompt([
        {
          type: 'input',
          name: 'customKeyPath',
          message: 'Enter SSH private key path:',
          validate: (input: string) => {
            if (!input.trim()) return 'SSH key path is required';
            return true;
          },
        },
      ]);
      sshConfig.keyPath = customKeyPath;
    }

    return sshConfig;
  }

  private async collectBasicConfiguration(): Promise<{ version: string }> {
    console.log(chalk.cyan('\n📋 Basic Configuration\n'));

    return {
      version: '1.0.0',
    };
  }

  private async collectNodesConfiguration(): Promise<{
    primary: NodeConfig;
    backup: NodeConfig;
  }> {
    console.log(chalk.cyan('\n🖥️ Node Configuration\n'));
    console.log(
      chalk.gray('Configure your primary and backup validator nodes.\n')
    );

    // Configure primary node
    console.log(chalk.green('\n🟢 Primary Validator Node'));
    const primaryNode = await this.configureNode('primary');

    // Configure backup node
    console.log(chalk.yellow('\n🟡 Backup Validator Node'));
    const backupNode = await this.configureNode('backup');

    return {
      primary: primaryNode,
      backup: backupNode,
    };
  }

  private async configureNode(nodeType: string): Promise<NodeConfig> {
    const nodeConfig = await inquirer.prompt([
      {
        type: 'input',
        name: 'label',
        message: `${nodeType} node label:`,
        default: `${nodeType} validator`,
        validate: (input: string) => {
          if (!input.trim()) return 'Label is required';
          return true;
        },
      },
      {
        type: 'input',
        name: 'host',
        message: `${nodeType} node host (IP or hostname):`,
        validate: (input: string) => {
          if (!input.trim()) return 'Host is required';
          // Basic IP/hostname validation
          const ipRegex = /^(?:[0-9]{1,3}\.){3}[0-9]{1,3}$/;
          const hostnameRegex =
            /^[a-zA-Z0-9][a-zA-Z0-9-]{0,61}[a-zA-Z0-9]?(?:\.[a-zA-Z0-9][a-zA-Z0-9-]{0,61}[a-zA-Z0-9]?)*$/;

          if (!ipRegex.test(input) && !hostnameRegex.test(input)) {
            return 'Please enter a valid IP address or hostname';
          }
          return true;
        },
      },
      {
        type: 'number',
        name: 'port',
        message: `${nodeType} node SSH port:`,
        default: 22,
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
        message: `${nodeType} node SSH user:`,
        default: 'solana',
        validate: (input: string) => {
          if (!input.trim()) return 'User is required';
          if (!/^[a-z_][a-z0-9_-]*$/.test(input)) {
            return 'Please enter a valid username';
          }
          return true;
        },
      },
    ]);

    // Collect validator paths
    console.log(chalk.gray(`\n📁 ${nodeType} node file paths:`));

    const pathConfig = await inquirer.prompt([
      {
        type: 'input',
        name: 'fundedIdentity',
        message: 'Funded identity keypair path:',
        default: '/home/solana/funded-validator-keypair.json',
        validate: (input: string) => {
          if (!input.trim()) return 'Funded identity path is required';
          return true;
        },
      },
      {
        type: 'input',
        name: 'unfundedIdentity',
        message: 'Unfunded identity keypair path:',
        default: '/home/solana/unfunded-validator-keypair.json',
        validate: (input: string) => {
          if (!input.trim()) return 'Unfunded identity path is required';
          return true;
        },
      },
      {
        type: 'input',
        name: 'voteKeypair',
        message: 'Vote account keypair path:',
        default: '/home/solana/vote-account-keypair.json',
        validate: (input: string) => {
          if (!input.trim()) return 'Vote keypair path is required';
          return true;
        },
      },
      {
        type: 'input',
        name: 'ledger',
        message: 'Ledger directory path:',
        default: '/mnt/ledger',
        validate: (input: string) => {
          if (!input.trim()) return 'Ledger path is required';
          return true;
        },
      },
      {
        type: 'input',
        name: 'tower',
        message: 'Tower file path (supports wildcards):',
        default: '/mnt/ledger/tower-1_9-*.bin',
        validate: (input: string) => {
          if (!input.trim()) return 'Tower path is required';
          return true;
        },
      },
      {
        type: 'input',
        name: 'solanaCliPath',
        message: 'Solana CLI binary path:',
        default:
          '/home/solana/.local/share/solana/install/active_release/bin/solana',
        validate: (input: string) => {
          if (!input.trim()) return 'Solana CLI path is required';
          return true;
        },
      },
    ]);

    return {
      ...nodeConfig,
      paths: pathConfig,
    };
  }

  private async collectRPCConfiguration(): Promise<{
    endpoint: string;
    timeout: number;
  }> {
    console.log(chalk.cyan('\n🌐 RPC Configuration\n'));

    const rpcConfig = await inquirer.prompt([
      {
        type: 'list',
        name: 'endpoint',
        message: 'Solana RPC endpoint:',
        choices: [
          {
            name: 'Mainnet Beta (Official)',
            value: 'https://api.mainnet-beta.solana.com',
          },
          {
            name: 'Testnet (Official)',
            value: 'https://api.testnet.solana.com',
          },
          { name: '📝 Custom endpoint', value: 'custom' },
        ],
        default: 'https://api.mainnet-beta.solana.com',
      },
      {
        type: 'number',
        name: 'timeout',
        message: 'RPC request timeout (ms):',
        default: 30000,
        validate: (input: number) => {
          if (!input || input < 1000 || input > 120000) {
            return 'Timeout must be between 1000ms and 120000ms';
          }
          return true;
        },
      },
    ]);

    // Handle custom endpoint
    if (rpcConfig.endpoint === 'custom') {
      const { customEndpoint } = await inquirer.prompt([
        {
          type: 'input',
          name: 'customEndpoint',
          message: 'Enter custom RPC endpoint:',
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
      ]);
      rpcConfig.endpoint = customEndpoint;
    }

    return rpcConfig;
  }

  private getDefaultMonitoringConfig(): any {
    return {
      interval: 5000,
      healthThreshold: 100,
      readinessThreshold: 50,
      enableMetrics: true,
      metricsRetention: 7,
    };
  }

  private getDefaultSecurityConfig(): any {
    return {
      confirmSwitches: true,
      maxRetries: 3,
    };
  }

  private getDefaultDisplayConfig(): any {
    return {
      theme: 'dark',
      compact: true,
      showTechnicalDetails: false,
    };
  }

  private async validateAndSaveConfiguration(config: Config): Promise<void> {
    const spinner = ora('Validating configuration...').start();

    try {
      // Validate configuration
      const result = Validator.validateConfig(config);

      if (result.errors.length > 0) {
        spinner.fail('Configuration validation failed');
        console.log(chalk.red('\n❌ Configuration errors:'));
        result.errors.forEach((error: any) => {
          console.log(chalk.red(`  • ${error.message || error}`));
        });
        throw new Error('Configuration validation failed');
      }

      if (result.warnings.length > 0) {
        spinner.text = 'Configuration has warnings...';
        console.log(chalk.yellow('\n⚠️ Configuration warnings:'));
        result.warnings.forEach((warning: any) => {
          console.log(chalk.yellow(`  • ${warning.message || warning}`));
        });

        const { continueWithWarnings } = await inquirer.prompt([
          {
            type: 'confirm',
            name: 'continueWithWarnings',
            message: 'Continue with warnings?',
            default: true,
          },
        ]);

        if (!continueWithWarnings) {
          spinner.fail('Setup cancelled due to warnings');
          return;
        }
      }

      // Save configuration
      spinner.text = 'Saving configuration...';
      await this.configManager.save(config);

      spinner.succeed('Configuration saved successfully');
      console.log(
        chalk.green(
          `✅ Configuration saved to: ${this.configManager.getConfigPath()}`
        )
      );
    } catch (error) {
      spinner.fail('Failed to save configuration');
      throw error;
    }
  }

  private async testInitialConnections(config: Config): Promise<void> {
    const { testConnections } = await inquirer.prompt([
      {
        type: 'confirm',
        name: 'testConnections',
        message: 'Test SSH connections to configured nodes?',
        default: true,
      },
    ]);

    if (!testConnections) return;

    const spinner = ora('Testing connections...').start();

    try {
      const tests = [];

      // Test primary node
      if (config.nodes.primary) {
        spinner.text = 'Testing primary node connection...';
        tests.push(
          this.sshDetector
            .testConnection(
              config.nodes.primary.host,
              config.nodes.primary.port,
              config.nodes.primary.user,
              config.ssh.keyPath
            )
            .then((result: any) => ({ node: 'Primary', ...result }))
        );
      }

      // Test backup node
      if (config.nodes.backup) {
        spinner.text = 'Testing backup node connection...';
        tests.push(
          this.sshDetector
            .testConnection(
              config.nodes.backup.host,
              config.nodes.backup.port,
              config.nodes.backup.user,
              config.ssh.keyPath
            )
            .then((result: any) => ({ node: 'Backup', ...result }))
        );
      }

      const results = await Promise.all(tests);

      const failedConnections = results.filter((r: any) => !r.success);

      if (failedConnections.length === 0) {
        spinner.succeed('All connections successful');
        console.log(chalk.green('✅ All SSH connections tested successfully!'));
      } else {
        spinner.fail('Some connections failed');
        console.log(chalk.red('\n❌ Connection failures:'));
        failedConnections.forEach((result: any) => {
          console.log(chalk.red(`  • ${result.node}: ${result.error}`));
        });
        console.log(
          chalk.yellow(
            '\n⚠️ Please check your SSH configuration and try again.'
          )
        );
      }
    } catch (error) {
      spinner.fail('Connection test failed');
      console.log(
        chalk.yellow(
          '⚠️ Connection test failed. You can test connections later with: svs config --test'
        )
      );
    }
  }

  private async displayCompletion(): Promise<void> {
    console.log(chalk.green('\n✨ Setup Complete! ✨\n'));

    console.log(chalk.cyan('Next steps:'));
    console.log(
      chalk.gray('  1. Test your configuration: ') +
        chalk.white('svs config --test')
    );
    console.log(
      chalk.gray('  2. Check validator status: ') + chalk.white('svs status')
    );
    console.log(
      chalk.gray('  3. Monitor your validators: ') + chalk.white('svs monitor')
    );
    console.log(
      chalk.gray('  4. Perform a switch: ') + chalk.white('svs switch')
    );

    console.log(chalk.cyan('\nDocumentation:'));
    console.log(chalk.gray('  • Help: ') + chalk.white('svs --help'));
    console.log(
      chalk.gray('  • Config help: ') + chalk.white('svs config --help')
    );
    console.log(
      chalk.gray('  • Switch help: ') + chalk.white('svs switch --help')
    );

    console.log(chalk.green('\n🚀 Happy validating!\n'));
  }
}

const wizard = new SetupWizard();

export const setupCommand = new Command('setup')
  .description('Interactive setup wizard for configuring validator nodes')
  .option('-f, --force', 'force setup even if configuration exists')
  .action(async (options: CLIOptions) => {
    await wizard.runSetup(options);
  });
