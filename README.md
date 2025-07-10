# Solana Validator Switch CLI

Professional-grade CLI tool for ultra-fast Solana validator switching with runtime node status detection, built in Rust.

> **Built by validators, for validators** - Stop losing sleep over manual switches. Get the fastest switch possible.

<p align="center">
  <strong>⭐ If this tool saves you time, please star the repo to help other validators discover it!</strong>
</p>

## Installation

### Quick Install (Recommended)

```bash
# Auto-detects your platform and installs the latest version
curl -sSL https://raw.githubusercontent.com/huiskylabs/solana-validator-switch/main/install.sh | bash

# After installation, 'svs' is available immediately
svs
```

### Alternative: Build from Source

<details>
<summary><sub>For developers or if the quick install doesn't work (requires Rust and Cargo)</sub></summary>

#### Clone and Run
```bash
git clone https://github.com/huiskylabs/solana-validator-switch
cd solana-validator-switch
cargo run --release
```

#### Install with Cargo
```bash
cargo install --git https://github.com/huiskylabs/solana-validator-switch

# Add to PATH if not already there
export PATH="$HOME/.cargo/bin:$PATH"
svs
```
</details>

## Usage

```bash
svs           # Opens interactive menu
svs --version # Show version
```


## Configuration

```bash
mkdir -p ~/.solana-validator-switch
cp config.yaml.example ~/.solana-validator-switch/config.yaml
nano ~/.solana-validator-switch/config.yaml
```

See [config.yaml.example](config.yaml.example) for the full configuration template.

## Key Features

- **Ultra-Fast Switching**: Get the fastest switch possible with optimized operations
- **Runtime Status Detection**: Automatic active/standby node detection using validator monitor
- **SSH Connection Pooling**: Persistent connections for ultra-fast operations
- **Universal Support**: Works with Firedancer, Agave, Solana, and Jito validators

## Security

- **No credential storage**: SSH private keys never leave your `~/.ssh/` directory
- **Path-only configuration**: Only file paths and hostnames stored in config files
- **No network exposure**: Tool operates through SSH connections only
- **Local execution**: All operations run locally, no external services

## Why SVS?

Built by [Huisky Labs](https://huisky.xyz/) validator team who needed reliable switching tools for our own operations. After countless manual switches and near-misses, we built what we wished existed.

- **Battle-tested**: Used in production by Huisky Labs validators
- **Community-driven**: We actively use and improve this tool daily
- **Open source**: Transparency and security through open development

### Support Development

If SVS saves you time and SOL, consider:
- ⭐ Starring this repo to help other validators find it
- 🗳️ Delegating to [Huisky Labs validators](https://huisky.xyz/) 
- 🐛 Reporting issues or contributing improvements

## License

MIT License

---

<div align="center">
Built with ❤️ by <a href="https://huisky.xyz/">Huisky Labs</a> • <a href="https://github.com/huiskylabs">GitHub</a> • <a href="https://twitter.com/huiskylabs">Twitter</a>
</div>