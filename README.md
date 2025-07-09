# Solana Validator Switch CLI

Professional-grade CLI tool for ultra-fast Solana validator switching with runtime node status detection, built in Rust.

> **Built by validators, for validators** - Stop losing sleep over manual switches. Average switch time:  ~1 seconds.

## Installation & Usage

```bash
# Build and run
cargo build --release
./target/release/svs

# Available commands
svs                    # Interactive menu
svs status             # Check validator status
svs switch             # Switch validators
svs switch --dry-run   # Preview switch
```

## Configuration

Copy the example config and fill in your details:

```bash
# Create config directory and copy example
mkdir -p ~/.solana-validator-switch
cp config.yaml.example ~/.solana-validator-switch/config.yaml
# Edit with your validator details
nano ~/.solana-validator-switch/config.yaml
```

Example configuration:

```yaml
version: "1.0.0"
validators:
  - votePubkey: "YourVotePubkey..."
    identityPubkey: "YourIdentityPubkey..."
    rpc: "https://api.mainnet-beta.solana.com"
    localSshKeyPath: "~/.ssh/id_rsa"
    nodes:
      - label: "Node 1"
        host: "1.2.3.4"
        port: 22
        user: "solana"
        paths:
          fundedIdentity: "/home/solana/keypairs/funded-validator-keypair.json"
          unfundedIdentity: "/home/solana/keypairs/unfunded-validator-keypair.json"
          voteKeypair: "/home/solana/keypairs/vote-account-keypair.json"
          ledger: "/mnt/solana_ledger"
          tower: "/mnt/solana_ledger/tower-1_9-*.bin"
          solanaCliPath: "/home/solana/.local/share/solana/install/active_release/bin/solana"
```

## Key Features

- **Ultra-Fast Switching**: Complete validator failover under 1 seconds
- **Runtime Status Detection**: Automatic active/standby node detection using validator monitor
- **SSH Connection Pooling**: Persistent connections for ultra-fast operations
- **Universal Support**: Works with Firedancer, Agave, Solana, and Jito validators
- **Instant Status Commands**: Uses cached startup data for immediate responses
- **Production Ready**: Built for 24/7 validator operations with robust error handling

## Security

- **No credential storage**: SSH private keys never leave your `~/.ssh/` directory
- **Path-only configuration**: Only file paths and hostnames stored in config files
- **No network exposure**: Tool operates through SSH connections only
- **Local execution**: All operations run locally, no external services
- **Secure defaults**: Conservative security settings, requires explicit SSH key paths

## Development

```bash
cargo build          # Build project
cargo run            # Run in development
cargo test           # Run tests
cargo fmt            # Format code
cargo clippy         # Lint code
```

## Why SVS?

Built by [huisky staking](https://huisky.xyz/), an active validator who needed reliable switching tools. Open-source, transparent, and focused on what validators actually need: fast, secure, and reliable operations.

## License

MIT License