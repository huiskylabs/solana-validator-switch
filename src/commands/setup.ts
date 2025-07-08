import { Command } from 'commander';
import type { CLIOptions } from '../types/index.js';

export const setupCommand = new Command('setup')
  .description('Interactive setup wizard for configuring validator nodes')
  .option('-f, --force', 'force setup even if configuration exists')
  .action(async (options: CLIOptions) => {
    console.log('🔧 Setup command - Not implemented yet');
    console.log('This will be the interactive setup wizard');

    if (options.force) {
      console.log('Force mode enabled - will overwrite existing configuration');
    }
  });
