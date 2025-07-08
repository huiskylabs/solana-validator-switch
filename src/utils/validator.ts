import type { ValidationSchema } from '../types/config.js';

// Validation result interface
export interface ValidationResult {
  valid: boolean;
  errors: ValidationError[];
  warnings: ValidationWarning[];
}

export interface ValidationError {
  field: string;
  message: string;
  code: string;
  value?: unknown;
}

export interface ValidationWarning {
  field: string;
  message: string;
  suggestion?: string;
}

// Re-export ValidationSchema from config types

export class Validator {
  /**
   * Validate a complete configuration object
   */
  static validateConfig(config: unknown): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    // Type check
    if (!config || typeof config !== 'object') {
      errors.push({
        field: 'root',
        message: 'Configuration must be an object',
        code: 'INVALID_TYPE',
      });
      return { valid: false, errors, warnings };
    }

    const cfg = config as Record<string, unknown>;

    // Required fields
    const requiredFields = ['version', 'nodes', 'rpc'];
    for (const field of requiredFields) {
      if (!(field in cfg)) {
        errors.push({
          field,
          message: `Required field '${field}' is missing`,
          code: 'REQUIRED_FIELD_MISSING',
        });
      }
    }

    // Validate version
    if ('version' in cfg) {
      const versionResult = this.validateVersion(cfg.version);
      errors.push(...versionResult.errors);
      warnings.push(...versionResult.warnings);
    }

    // Validate nodes
    if ('nodes' in cfg && typeof cfg.nodes === 'object' && cfg.nodes) {
      const nodesResult = this.validateNodes(cfg.nodes);
      errors.push(...nodesResult.errors);
      warnings.push(...warnings);
    }

    // Validate RPC configuration
    if ('rpc' in cfg) {
      const rpcResult = this.validateRPC(cfg.rpc);
      errors.push(...rpcResult.errors);
      warnings.push(...rpcResult.warnings);
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  /**
   * Validate node configuration
   */
  static validateNodeConfig(node: unknown, label = 'node'): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    if (!node || typeof node !== 'object') {
      errors.push({
        field: label,
        message: 'Node configuration must be an object',
        code: 'INVALID_TYPE',
      });
      return { valid: false, errors, warnings };
    }

    const nodeConfig = node as Record<string, unknown>;

    // Required fields
    const requiredFields = [
      'label',
      'host',
      'port',
      'user',
      'keyPath',
      'paths',
    ];
    for (const field of requiredFields) {
      if (!(field in nodeConfig)) {
        errors.push({
          field: `${label}.${field}`,
          message: `Required field '${field}' is missing`,
          code: 'REQUIRED_FIELD_MISSING',
        });
      }
    }

    // Validate host
    if ('host' in nodeConfig) {
      const hostResult = this.validateHost(nodeConfig.host, `${label}.host`);
      errors.push(...hostResult.errors);
      warnings.push(...hostResult.warnings);
    }

    // Validate port
    if ('port' in nodeConfig) {
      const portResult = this.validatePort(nodeConfig.port, `${label}.port`);
      errors.push(...portResult.errors);
      warnings.push(...portResult.warnings);
    }

    // Validate paths
    if ('paths' in nodeConfig) {
      const pathsResult = this.validatePaths(
        nodeConfig.paths,
        `${label}.paths`
      );
      errors.push(...pathsResult.errors);
      warnings.push(...pathsResult.warnings);
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  /**
   * Validate environment variables
   */
  static validateEnvironment(
    env: Record<string, string | undefined>
  ): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    // Define environment variable schema
    const envSchema: Record<string, ValidationSchema> = {
      SVS_CONFIG_PATH: {
        patterns: { path: /^[^\0]+$/ },
      },
      SVS_SSH_TIMEOUT: {
        types: { value: 'number' },
        ranges: { value: [1, 300] },
      },
      SVS_LOG_LEVEL: {
        custom: {
          value: val =>
            ['debug', 'info', 'warn', 'error'].includes(val as string),
        },
      },
      SVS_NO_COLOR: {
        custom: {
          value: val => ['true', 'false', '1', '0'].includes(val as string),
        },
      },
      SVS_REFRESH_INTERVAL: {
        types: { value: 'number' },
        ranges: { value: [1, 60] },
      },
      SVS_RPC_ENDPOINT: {
        patterns: { value: /^https?:\/\/.+/ },
      },
      SVS_MAX_RETRIES: {
        types: { value: 'number' },
        ranges: { value: [1, 10] },
      },
      SVS_THEME: {
        custom: {
          value: val => ['dark', 'light', 'auto'].includes(val as string),
        },
      },
      SVS_COMPACT_MODE: {
        custom: {
          value: val => ['true', 'false', '1', '0'].includes(val as string),
        },
      },
    };

    // Validate each environment variable
    for (const [key, value] of Object.entries(env)) {
      if (value === undefined) continue;

      const schema = envSchema[key];
      if (!schema) {
        warnings.push({
          field: key,
          message: `Unknown environment variable '${key}'`,
          suggestion: 'Remove if not needed',
        });
        continue;
      }

      const result = this.validateWithSchema(value, schema, key);
      errors.push(...result.errors);
      warnings.push(...result.warnings);
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  /**
   * Validate SSH key
   */
  static validateSSHKey(sshKey: unknown): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    if (!sshKey || typeof sshKey !== 'object') {
      errors.push({
        field: 'sshKey',
        message: 'SSH key must be an object',
        code: 'INVALID_TYPE',
      });
      return { valid: false, errors, warnings };
    }

    const key = sshKey as Record<string, unknown>;

    // Required fields
    if (!key.path || typeof key.path !== 'string') {
      errors.push({
        field: 'sshKey.path',
        message: 'SSH key path is required and must be a string',
        code: 'REQUIRED_FIELD_MISSING',
      });
    }

    if (!key.type || typeof key.type !== 'string') {
      errors.push({
        field: 'sshKey.type',
        message: 'SSH key type is required',
        code: 'REQUIRED_FIELD_MISSING',
      });
    } else {
      const validTypes = ['rsa', 'ed25519', 'ecdsa', 'dsa'];
      if (!validTypes.includes(key.type)) {
        errors.push({
          field: 'sshKey.type',
          message: `Invalid SSH key type '${key.type}'. Must be one of: ${validTypes.join(', ')}`,
          code: 'INVALID_VALUE',
        });
      }
    }

    // Validate fingerprint format
    if (key.fingerprint && typeof key.fingerprint === 'string') {
      const fingerprintPattern = /^([a-fA-F0-9]{2}:){15}[a-fA-F0-9]{2}$/;
      if (!fingerprintPattern.test(key.fingerprint)) {
        warnings.push({
          field: 'sshKey.fingerprint',
          message: 'SSH key fingerprint format may be invalid',
          suggestion: 'Ensure fingerprint follows standard SHA256 format',
        });
      }
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  /**
   * Validate host (IP address or hostname)
   */
  private static validateHost(host: unknown, field: string): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    if (typeof host !== 'string') {
      errors.push({
        field,
        message: 'Host must be a string',
        code: 'INVALID_TYPE',
        value: host,
      });
      return { valid: false, errors, warnings };
    }

    if (!host.trim()) {
      errors.push({
        field,
        message: 'Host cannot be empty',
        code: 'REQUIRED_FIELD_MISSING',
      });
      return { valid: false, errors, warnings };
    }

    // Check if it's an IP address
    const ipv4Pattern = /^(\d{1,3}\.){3}\d{1,3}$/;
    const ipv6Pattern = /^([0-9a-fA-F]{1,4}:){7}[0-9a-fA-F]{1,4}$/;

    if (ipv4Pattern.test(host)) {
      // Validate IPv4 ranges
      const parts = host.split('.').map(Number);
      if (parts.some(part => part < 0 || part > 255)) {
        errors.push({
          field,
          message: 'Invalid IPv4 address',
          code: 'INVALID_VALUE',
          value: host,
        });
      }
    } else if (!ipv6Pattern.test(host)) {
      // Validate hostname
      const hostnamePattern =
        /^[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(\.[a-zA-Z0-9]([a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)*$/;
      if (!hostnamePattern.test(host)) {
        errors.push({
          field,
          message: 'Invalid hostname format',
          code: 'INVALID_VALUE',
          value: host,
        });
      }
    }

    // Warn about localhost
    if (host === 'localhost' || host === '127.0.0.1') {
      warnings.push({
        field,
        message: 'Using localhost may cause issues in production',
        suggestion: 'Consider using the actual hostname or IP address',
      });
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  /**
   * Validate port number
   */
  private static validatePort(port: unknown, field: string): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    if (typeof port !== 'number') {
      errors.push({
        field,
        message: 'Port must be a number',
        code: 'INVALID_TYPE',
        value: port,
      });
      return { valid: false, errors, warnings };
    }

    if (!Number.isInteger(port) || port < 1 || port > 65535) {
      errors.push({
        field,
        message: 'Port must be an integer between 1 and 65535',
        code: 'INVALID_RANGE',
        value: port,
      });
    }

    // Warn about common ports
    if (port < 1024) {
      warnings.push({
        field,
        message: 'Using privileged port (< 1024)',
        suggestion: 'Ensure you have appropriate permissions',
      });
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  /**
   * Validate version string
   */
  private static validateVersion(version: unknown): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    if (typeof version !== 'string') {
      errors.push({
        field: 'version',
        message: 'Version must be a string',
        code: 'INVALID_TYPE',
        value: version,
      });
      return { valid: false, errors, warnings };
    }

    const semverPattern =
      /^\d+\.\d+\.\d+(-[a-zA-Z0-9.-]+)?(\+[a-zA-Z0-9.-]+)?$/;
    if (!semverPattern.test(version)) {
      warnings.push({
        field: 'version',
        message: 'Version does not follow semantic versioning',
        suggestion: 'Use format: major.minor.patch',
      });
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  /**
   * Validate nodes configuration
   */
  private static validateNodes(nodes: unknown): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    if (!nodes || typeof nodes !== 'object') {
      errors.push({
        field: 'nodes',
        message: 'Nodes configuration must be an object',
        code: 'INVALID_TYPE',
      });
      return { valid: false, errors, warnings };
    }

    const nodeConfig = nodes as Record<string, unknown>;

    // Check for required node types
    if (!nodeConfig.primary) {
      errors.push({
        field: 'nodes.primary',
        message: 'Primary node configuration is required',
        code: 'REQUIRED_FIELD_MISSING',
      });
    } else {
      const primaryResult = this.validateNodeConfig(
        nodeConfig.primary,
        'primary'
      );
      errors.push(...primaryResult.errors);
      warnings.push(...primaryResult.warnings);
    }

    if (!nodeConfig.backup) {
      errors.push({
        field: 'nodes.backup',
        message: 'Backup node configuration is required',
        code: 'REQUIRED_FIELD_MISSING',
      });
    } else {
      const backupResult = this.validateNodeConfig(nodeConfig.backup, 'backup');
      errors.push(...backupResult.errors);
      warnings.push(...backupResult.warnings);
    }

    // Check for duplicate hosts
    if (nodeConfig.primary && nodeConfig.backup) {
      const primary = nodeConfig.primary as { host?: string };
      const backup = nodeConfig.backup as { host?: string };

      if (primary.host && backup.host && primary.host === backup.host) {
        warnings.push({
          field: 'nodes',
          message: 'Primary and backup nodes have the same host',
          suggestion: 'Consider using different hosts for redundancy',
        });
      }
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  /**
   * Validate RPC configuration
   */
  private static validateRPC(rpc: unknown): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    if (!rpc || typeof rpc !== 'object') {
      errors.push({
        field: 'rpc',
        message: 'RPC configuration must be an object',
        code: 'INVALID_TYPE',
      });
      return { valid: false, errors, warnings };
    }

    const rpcConfig = rpc as Record<string, unknown>;

    // Validate endpoint
    if (!rpcConfig.endpoint) {
      errors.push({
        field: 'rpc.endpoint',
        message: 'RPC endpoint is required',
        code: 'REQUIRED_FIELD_MISSING',
      });
    } else if (typeof rpcConfig.endpoint !== 'string') {
      errors.push({
        field: 'rpc.endpoint',
        message: 'RPC endpoint must be a string',
        code: 'INVALID_TYPE',
      });
    } else {
      const urlPattern = /^https?:\/\/.+/;
      if (!urlPattern.test(rpcConfig.endpoint)) {
        errors.push({
          field: 'rpc.endpoint',
          message: 'RPC endpoint must be a valid HTTP/HTTPS URL',
          code: 'INVALID_FORMAT',
        });
      }
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  /**
   * Validate paths configuration
   */
  private static validatePaths(
    paths: unknown,
    field: string
  ): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    if (!paths || typeof paths !== 'object') {
      errors.push({
        field,
        message: 'Paths configuration must be an object',
        code: 'INVALID_TYPE',
      });
      return { valid: false, errors, warnings };
    }

    const pathsConfig = paths as Record<string, unknown>;
    const requiredPaths = [
      'fundedIdentity',
      'unfundedIdentity',
      'ledger',
      'tower',
      'solanaCliPath',
    ];

    for (const pathName of requiredPaths) {
      if (!pathsConfig[pathName]) {
        errors.push({
          field: `${field}.${pathName}`,
          message: `Required path '${pathName}' is missing`,
          code: 'REQUIRED_FIELD_MISSING',
        });
      } else if (typeof pathsConfig[pathName] !== 'string') {
        errors.push({
          field: `${field}.${pathName}`,
          message: `Path '${pathName}' must be a string`,
          code: 'INVALID_TYPE',
        });
      }
    }

    return { valid: errors.length === 0, errors, warnings };
  }

  /**
   * Validate using a schema
   */
  private static validateWithSchema(
    value: unknown,
    schema: ValidationSchema,
    field: string
  ): ValidationResult {
    const errors: ValidationError[] = [];
    const warnings: ValidationWarning[] = [];

    // Type validation
    if (schema.types) {
      for (const [key, expectedType] of Object.entries(schema.types)) {
        if (key === 'value') {
          if (expectedType === 'number') {
            const num = Number(value);
            if (isNaN(num)) {
              errors.push({
                field,
                message: `Expected number, got ${typeof value}`,
                code: 'INVALID_TYPE',
                value,
              });
            }
          }
        }
      }
    }

    // Range validation
    if (schema.ranges) {
      for (const [key, [min, max]] of Object.entries(schema.ranges)) {
        if (key === 'value') {
          const num = Number(value);
          if (!isNaN(num) && (num < min || num > max)) {
            errors.push({
              field,
              message: `Value must be between ${min} and ${max}`,
              code: 'INVALID_RANGE',
              value,
            });
          }
        }
      }
    }

    // Pattern validation
    if (schema.patterns) {
      for (const [key, pattern] of Object.entries(schema.patterns)) {
        if (key === 'value' && typeof value === 'string') {
          if (!pattern.test(value)) {
            errors.push({
              field,
              message: `Value does not match required pattern`,
              code: 'INVALID_FORMAT',
              value,
            });
          }
        }
      }
    }

    // Custom validation
    if (schema.custom) {
      for (const [key, validator] of Object.entries(schema.custom)) {
        if (key === 'value' && !validator(value)) {
          errors.push({
            field,
            message: `Value failed custom validation`,
            code: 'INVALID_VALUE',
            value,
          });
        }
      }
    }

    return { valid: errors.length === 0, errors, warnings };
  }
}
