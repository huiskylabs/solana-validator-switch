# Solana Validator Switch CLI

Professional-grade CLI tool for ultra-fast Solana validator switching with zero credential storage.

## 🎯 Project Status

**Milestone 1 Complete ✅**

### ✅ Completed Features

- **TypeScript Foundation**: Full TypeScript project with strict mode enabled
- **CLI Framework**: Commander.js-based CLI with comprehensive command structure
- **Project Structure**: Organized source code structure with proper separation of concerns
- **Build System**: Complete build pipeline with TypeScript compilation
- **Development Tools**: ESLint, Prettier, Jest testing framework configured
- **Version Control**: Comprehensive .gitignore for clean repository management
- **Binary Executables**: Both `svs` and `solana-validator-switch` commands available
- **Error Handling**: Comprehensive error handling framework with typed errors
- **Logging System**: Professional logging with Winston and colored CLI output

### 🛠️ Technical Architecture

- **Language**: TypeScript with ES2022 targeting
- **Module System**: ES Modules (ESM)
- **CLI Framework**: Commander.js for command parsing and routing
- **Build Tool**: TypeScript compiler (tsc)
- **Testing**: Jest with TypeScript support
- **Linting**: ESLint with TypeScript rules
- **Formatting**: Prettier for consistent code style

### 📁 Project Structure

```
solana-validator-switch/
├── src/                          # TypeScript source files
│   ├── commands/                 # CLI command handlers
│   │   ├── config.ts            # Configuration management
│   │   ├── health.ts            # Health monitoring
│   │   ├── monitor.ts           # Interactive dashboard
│   │   ├── setup.ts             # Interactive setup wizard
│   │   ├── status.ts            # Quick status check
│   │   ├── switch.ts            # Validator switching
│   │   └── version.ts           # Version information
│   ├── types/                   # TypeScript type definitions
│   │   └── index.ts             # Core interfaces and types
│   ├── utils/                   # Utility functions
│   │   ├── error-handler.ts     # Error handling framework
│   │   └── logger.ts            # Logging utilities
│   └── index.ts                 # Main CLI entry point
├── bin/                         # Executable binaries
│   ├── svs.js                   # Short name executable
│   └── solana-validator-switch.js # Full name executable
├── dist/                        # Compiled JavaScript output
├── tests/                       # Test files
└── docs/                        # Documentation
```

### 🚀 Available Commands

```bash
# Global installation (when published)
npm install -g solana-validator-switch

# Local development
npm run dev

# Available commands
svs --help                       # Show help
svs setup                        # Interactive setup wizard
svs config                       # Manage configuration
svs monitor                      # Interactive monitoring dashboard
svs status                       # Quick status check
svs switch                       # Switch validators
svs health                       # Detailed health report
svs version                      # Show version information
```

### 🔧 Development Commands

```bash
# Build the project
npm run build

# Development mode with hot reload
npm run dev

# Run tests
npm run test

# Lint code
npm run lint

# Format code
npm run lint:fix

# Clean build artifacts
npm run clean
```

### 📋 Type Definitions

The project includes comprehensive TypeScript interfaces for:

- **Configuration Management**: `Config`, `NodeConfig`, `MonitoringConfig`
- **SSH Operations**: `SSHConnection`, `SSHPoolConfig`, `SSHKey`
- **Health Monitoring**: `HealthStatus`, `MonitoringData`, `SystemResources`
- **Validator Operations**: `SwitchState`, `SwitchPlan`, `ValidatorClient`
- **Error Handling**: `SwitchError`, `ErrorSeverity`, `RecoveryPlan`
- **CLI Operations**: `CLIOptions`, `LogEntry`, `LoggerConfig`

### 🧪 Testing

- **Jest**: Configured with TypeScript support
- **Test Structure**: Unit tests for all core functions
- **Coverage**: Coverage reporting enabled
- **Mocking**: External dependencies properly mocked

### 📦 Dependencies

**Runtime Dependencies:**

- `commander`: CLI framework
- `inquirer`: Interactive prompts
- `ssh2` & `node-ssh`: SSH connectivity
- `winston`: Logging framework
- `chalk`: Terminal colors
- `ora`: Loading spinners
- `cli-table3`: Table formatting
- `boxen`: Terminal boxes
- `conf`: Configuration management
- `blessed`: Terminal UI components

**Development Dependencies:**

- `typescript`: TypeScript compiler
- `jest` & `ts-jest`: Testing framework
- `eslint`: Code linting
- `prettier`: Code formatting
- `ts-node`: TypeScript execution

### 🎯 Next Steps (Milestone 2)

- [ ] Configuration management system
- [ ] SSH connection handling
- [ ] Interactive setup wizard
- [ ] Basic validator detection
- [ ] Connection testing framework

### 🔨 Build Status

- ✅ TypeScript compilation successful
- ✅ ESLint passing
- ✅ CLI commands functional
- ✅ Binary executables working
- ✅ ES modules properly configured

### 📄 License

MIT License

### 🤝 Contributing

This project follows TypeScript best practices with strict typing enabled. All contributions should include proper type definitions and pass the existing linting rules.

---

**Status**: Milestone 1 Complete - Ready for Milestone 2 development
