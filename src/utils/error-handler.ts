import type { Logger } from "./logger.js";
import type { SwitchError, ErrorSeverity } from "../types/index.js";

export class ErrorHandler {
  constructor(private logger: Logger) {}

  handle(error: unknown): void {
    if (error instanceof Error) {
      this.handleError(error);
    } else {
      this.handleUnknownError(error);
    }
  }

  private handleError(error: Error): void {
    if (this.isSwitchError(error)) {
      this.handleSwitchError(error);
    } else {
      this.handleGenericError(error);
    }
  }

  private handleSwitchError(error: SwitchError): void {
    this.logger.error(error.message, {
      code: error.code,
      severity: error.severity,
      recoverable: error.recoverable,
      timestamp: error.timestamp,
    });

    // Show user-friendly error message
    this.logger.errorMessage(error.message);

    if (error.suggestions.length > 0) {
      this.logger.info("Suggestions:");
      error.suggestions.forEach((suggestion) => {
        this.logger.info(`  • ${suggestion}`);
      });
    }
  }

  private handleGenericError(error: Error): void {
    this.logger.error(error.message, {
      stack: error.stack,
      name: error.name,
    });

    this.logger.errorMessage(`${error.name}: ${error.message}`);
  }

  private handleUnknownError(error: unknown): void {
    this.logger.error("Unknown error occurred", { error });
    this.logger.errorMessage("An unknown error occurred. Please try again.");
  }

  private isSwitchError(error: Error): error is SwitchError {
    return "code" in error && "severity" in error && "recoverable" in error;
  }

  createSwitchError(
    code: string,
    message: string,
    severity: ErrorSeverity,
    recoverable: boolean = false,
    suggestions: string[] = [],
  ): SwitchError {
    const error = new Error(message) as SwitchError;
    error.code = code;
    error.severity = severity;
    error.recoverable = recoverable;
    error.suggestions = suggestions;
    error.timestamp = Date.now();
    return error;
  }
}
