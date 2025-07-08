import { Command } from 'commander';
import type { CLIOptions } from '../types/index.js';

export const monitorCommand = new Command('monitor')
  .description('Interactive monitoring dashboard')
  .option('-i, --interval <seconds>', 'refresh interval in seconds', '2')
  .option('-c, --compact', 'use compact display mode')
  .action(async (options: CLIOptions) => {
    console.log('📊 Monitor command - Not implemented yet');
    console.log('This will be the interactive monitoring dashboard');

    const interval = parseInt(options.interval || '2', 10);
    console.log(`Refresh interval: ${interval} seconds`);

    if (options.compact) {
      console.log('Compact mode enabled');
    }
  });
