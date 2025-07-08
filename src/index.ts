#!/usr/bin/env node

import { Command } from 'commander';
import { setupCommand } from './commands/setup.js';
import { configCommand } from './commands/config.js';
import { monitorCommand } from './commands/monitor.js';
import { statusCommand } from './commands/status.js';
import { switchCommand } from './commands/switch.js';
import { healthCommand } from './commands/health.js';
import { versionCommand } from './commands/version.js';
import { ErrorHandler } from './utils/error-handler.js';
import { Logger } from './utils/logger.js';
import type { CLIOptions } from './types/index.js';
import inquirer from 'inquirer';

// Temporarily disable SSH key detector to avoid TypeScript errors
// import { SSHKeyDetector } from './utils/ssh-key-detector.js';

const program = new Command();

async function showInteractiveMenu(): Promise<void> {
  console.clear();
  console.log('🚀 Welcome to Solana Validator Switch CLI v1.0.0');
  console.log('Professional-grade validator switching from your terminal\n');
  
  const { action } = await inquirer.prompt([
    {
      type: 'list',
      name: 'action',
      message: 'What would you like to do?',
      choices: [
        {
          name: '🔧 Setup - Configure your validator nodes and SSH keys',
          value: 'setup',
        },
        {
          name: '📋 Status - Check current validator status',
          value: 'status',
        },
        {
          name: '🔄 Switch - Switch between validators',
          value: 'switch',
        },
        {
          name: '💚 Health - Detailed health check',
          value: 'health',
        },
        {
          name: '📊 Monitor - Real-time monitoring dashboard',
          value: 'monitor',
        },
        {
          name: '⚙️  Config - Manage configuration',
          value: 'config',
        },
        {
          name: '📌 Version - Show version information',
          value: 'version',
        },
        new inquirer.Separator(),
        {
          name: '❌ Exit',
          value: 'exit',
        },
      ],
    },
  ]);

  if (action === 'exit') {
    console.log('👋 Goodbye!');
    process.exit(0);
  }

  // Handle sub-menus for complex commands
  if (action === 'config') {
    await showConfigMenu();
  } else if (action === 'switch') {
    await showSwitchMenu();
  } else {
    // For simple commands, run directly
    const args = [process.argv[0]!, process.argv[1]!, action];
    await program.parseAsync(args);
  }
}

async function showConfigMenu(): Promise<void> {
  console.log('\n⚙️  Configuration Management\n');
  
  const { configAction } = await inquirer.prompt([
    {
      type: 'list',
      name: 'configAction',
      message: 'Select configuration action:',
      choices: [
        {
          name: '📋 List - Show current configuration',
          value: 'list',
        },
        {
          name: '✏️  Edit - Edit configuration interactively',
          value: 'edit',
        },
        {
          name: '🧪 Test - Test SSH connections',
          value: 'test',
        },
        {
          name: '📤 Export - Export configuration to file',
          value: 'export',
        },
        {
          name: '📥 Import - Import configuration from file',
          value: 'import',
        },
        new inquirer.Separator(),
        {
          name: '⬅️  Back to main menu',
          value: 'back',
        },
      ],
    },
  ]);

  if (configAction === 'back') {
    await showInteractiveMenu();
    return;
  }

  // Run the config command with the selected action
  const args = [process.argv[0]!, process.argv[1]!, 'config', `--${configAction}`];
  await program.parseAsync(args);
}

async function showSwitchMenu(): Promise<void> {
  console.log('\n🔄 Validator Switching\n');
  
  const { switchAction } = await inquirer.prompt([
    {
      type: 'list',
      name: 'switchAction',
      message: 'Select switching action:',
      choices: [
        {
          name: '🔄 Switch - Perform validator switch',
          value: 'switch',
        },
        {
          name: '🧪 Dry Run - Preview switch without executing',
          value: 'dry-run',
        },
        {
          name: '⚡ Force - Force switch (skip safety checks)',
          value: 'force',
        },
        {
          name: '📊 Status - Check switch readiness',
          value: 'status',
        },
        new inquirer.Separator(),
        {
          name: '⬅️  Back to main menu',
          value: 'back',
        },
      ],
    },
  ]);

  if (switchAction === 'back') {
    await showInteractiveMenu();
    return;
  }

  // Run the switch command with the selected action
  let args = [process.argv[0]!, process.argv[1]!, 'switch'];
  
  if (switchAction === 'dry-run') {
    args.push('--dry-run');
  } else if (switchAction === 'force') {
    args.push('--force');
  } else if (switchAction === 'status') {
    args.push('--status');
  }
  
  await program.parseAsync(args);
}

async function main(): Promise<void> {
  try {
    const logger = new Logger();
    const errorHandler = new ErrorHandler(logger);

    // Use errorHandler to prevent unused variable warning
    errorHandler;

    // Global CLI configuration
    program
      .name('svs')
      .description(
        'Professional-grade CLI tool for ultra-fast Solana validator switching'
      )
      .version('1.0.0')
      .option('-c, --config <path>', 'path to configuration file')
      .option('-v, --verbose', 'enable verbose output')
      .option('-q, --quiet', 'suppress output')
      .option('--no-color', 'disable colored output')
      .option(
        '--log-level <level>',
        'set log level (debug, info, warn, error)',
        'info'
      )
      .hook('preAction', thisCommand => {
        const options = thisCommand.opts<CLIOptions>();
        logger.setLevel(options.logLevel || 'info');
        logger.setColorize(!options.noColor);
      });

    // Register commands
    program.addCommand(setupCommand);
    program.addCommand(configCommand);
    program.addCommand(monitorCommand);
    program.addCommand(statusCommand);
    program.addCommand(switchCommand);
    program.addCommand(healthCommand);
    program.addCommand(versionCommand);

    // Default action - show help or launch interactive mode
    program.action(async () => {
      // If no arguments provided, show interactive menu
      if (process.argv.length === 2) {
        await showInteractiveMenu();
        return;
      }
    });

    // Parse command line arguments
    await program.parseAsync(process.argv);
  } catch (error) {
    const errorHandler = new ErrorHandler(new Logger());
    errorHandler.handle(error);
    process.exit(1);
  }
}

// Handle uncaught exceptions
process.on('uncaughtException', error => {
  console.error('Uncaught Exception:', error);
  process.exit(1);
});

process.on('unhandledRejection', (reason, promise) => {
  console.error('Unhandled Rejection at:', promise, 'reason:', reason);
  process.exit(1);
});

// Run the CLI
main().catch(error => {
  console.error('Fatal error:', error);
  process.exit(1);
});
