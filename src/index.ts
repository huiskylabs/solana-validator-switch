#!/usr/bin/env node

import { Command } from "commander";
import { setupCommand } from "./commands/setup.js";
import { configCommand } from "./commands/config.js";
import { monitorCommand } from "./commands/monitor.js";
import { statusCommand } from "./commands/status.js";
import { switchCommand } from "./commands/switch.js";
import { healthCommand } from "./commands/health.js";
import { versionCommand } from "./commands/version.js";
import { ErrorHandler } from "./utils/error-handler.js";
import { Logger } from "./utils/logger.js";
import type { CLIOptions } from "./types/index.js";

const program = new Command();

async function main(): Promise<void> {
  try {
    const logger = new Logger();
    const errorHandler = new ErrorHandler(logger);

    // Use errorHandler to prevent unused variable warning
    errorHandler;

    // Global CLI configuration
    program
      .name("svs")
      .description(
        "Professional-grade CLI tool for ultra-fast Solana validator switching",
      )
      .version("1.0.0")
      .option("-c, --config <path>", "path to configuration file")
      .option("-v, --verbose", "enable verbose output")
      .option("-q, --quiet", "suppress output")
      .option("--no-color", "disable colored output")
      .option(
        "--log-level <level>",
        "set log level (debug, info, warn, error)",
        "info",
      )
      .hook("preAction", (thisCommand) => {
        const options = thisCommand.opts<CLIOptions>();
        logger.setLevel(options.logLevel || "info");
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
    program.action(() => {
      // If no arguments provided, show welcome message and help
      if (process.argv.length === 2) {
        console.log("🚀 Welcome to Solana Validator Switch CLI v1.0.0");
        console.log(
          "Professional-grade validator switching from your terminal\n",
        );
        program.help();
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
process.on("uncaughtException", (error) => {
  console.error("Uncaught Exception:", error);
  process.exit(1);
});

process.on("unhandledRejection", (reason, promise) => {
  console.error("Unhandled Rejection at:", promise, "reason:", reason);
  process.exit(1);
});

// Run the CLI
main().catch((error) => {
  console.error("Fatal error:", error);
  process.exit(1);
});
