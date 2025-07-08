import { Command } from 'commander';
import type { CLIOptions } from '../types/index.js';

export const statusCommand = new Command('status')
  .description('Quick status check of validator nodes')
  .option('-v, --verbose', 'show detailed status information')
  .option('-j, --json', 'output status in JSON format')
  .action(async (options: CLIOptions) => {
    console.log('📋 Status command - Not implemented yet');
    console.log('This will show quick status of validator nodes');

    if (options.verbose) {
      console.log('Verbose mode enabled');
    }

    if (options.json) {
      console.log('JSON output enabled');
    }
  });
