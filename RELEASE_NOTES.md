## 🚀 Solana Validator Switch v1.0.0

### ✨ Key Features
- **Ultra-fast switching**: Optimized for <1 second switch times
- **Multi-validator support**: Firedancer, Agave, and Solana validators
- **Intelligent SSH management**: Connection pooling and pre-warming
- **Automatic detection**: Discovers validator configs and paths
- **Tower file safety**: Secure transfer between nodes
- **Real-time monitoring**: Live validator status tracking

### ⚡ Performance Improvements
- Removed all artificial delays for instant execution
- SSH connection pre-warming for reduced latency
- TCP optimizations with keep-alive
- Persistent connection pooling

### 📦 Installation

```bash
# Download and extract
curl -L https://github.com/huiskylabs/solana-validator-switch/releases/download/v1.0.0/svs-v1.0.0-macos-amd64.tar.gz | tar -xz

# Move to PATH
sudo mv svs-macos-amd64 /usr/local/bin/svs

# Make executable
sudo chmod +x /usr/local/bin/svs
```

### 🛠️ Usage

```bash
# Initial setup
svs init

# Check validator status
svs status

# Switch validators (ultra-fast)
svs switch

# Dry run to test
svs switch --dry-run
```

### 📋 Requirements
- macOS (Linux binaries coming soon)
- SSH access to validator nodes
- Configured validator identities

### 🔗 Links
- [Documentation](https://github.com/huiskylabs/solana-validator-switch#readme)
- [Report Issues](https://github.com/huiskylabs/solana-validator-switch/issues)