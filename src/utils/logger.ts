import winston from "winston";
import chalk from "chalk";
import type { LoggerConfig, LogEntry } from "../types/index.js";

export class Logger {
  private winston: winston.Logger;
  private colorize: boolean = true;
  private level: string = "info";

  constructor(config?: LoggerConfig) {
    this.level = config?.level || "info";
    this.colorize = config?.colorize !== false;

    // Create Winston logger
    this.winston = winston.createLogger({
      level: this.level,
      format: winston.format.combine(
        winston.format.timestamp(),
        winston.format.errors({ stack: true }),
        winston.format.json(),
      ),
      transports: [
        new winston.transports.Console({
          format: winston.format.combine(
            winston.format.colorize(),
            winston.format.simple(),
          ),
        }),
      ],
    });

    // Add file transport if specified
    if (config?.file) {
      this.winston.add(
        new winston.transports.File({
          filename: config.file,
          maxsize: config.maxSize || 5242880, // 5MB
          maxFiles: config.maxFiles || 5,
          format: winston.format.combine(
            winston.format.timestamp(),
            winston.format.json(),
          ),
        }),
      );
    }
  }

  setLevel(level: string): void {
    this.level = level;
    this.winston.level = level;
  }

  setColorize(colorize: boolean): void {
    this.colorize = colorize;
  }

  debug(message: string, context?: Record<string, unknown>): void {
    this.winston.debug(message, context);
  }

  info(message: string, context?: Record<string, unknown>): void {
    this.winston.info(message, context);
  }

  warn(message: string, context?: Record<string, unknown>): void {
    this.winston.warn(message, context);
  }

  error(message: string, context?: Record<string, unknown>): void {
    this.winston.error(message, context);
  }

  // CLI-specific formatted output methods
  success(message: string): void {
    if (this.colorize) {
      console.log(chalk.green("✅ " + message));
    } else {
      console.log("✅ " + message);
    }
  }

  warning(message: string): void {
    if (this.colorize) {
      console.log(chalk.yellow("⚠️ " + message));
    } else {
      console.log("⚠️ " + message);
    }
  }

  errorMessage(message: string): void {
    if (this.colorize) {
      console.log(chalk.red("❌ " + message));
    } else {
      console.log("❌ " + message);
    }
  }

  step(message: string): void {
    if (this.colorize) {
      console.log(chalk.blue("🔍 " + message));
    } else {
      console.log("🔍 " + message);
    }
  }

  header(message: string): void {
    if (this.colorize) {
      console.log(chalk.bold.cyan(message));
    } else {
      console.log(message);
    }
  }

  box(message: string): void {
    const boxen = require("boxen");
    console.log(
      boxen(message, {
        padding: 1,
        margin: 1,
        borderStyle: "round",
        borderColor: this.colorize ? "cyan" : undefined,
      }),
    );
  }

  createLogEntry(
    level: "debug" | "info" | "warn" | "error",
    message: string,
    context?: Record<string, unknown>,
  ): LogEntry {
    return {
      timestamp: Date.now(),
      level,
      message,
      ...(context && { context }),
    };
  }
}
