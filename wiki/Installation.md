# Installation

## Download Binary

```bash
# Linux
curl -L https://github.com/huiskylabs/solana-validator-switch/releases/latest/download/svs-linux-x86_64.tar.gz | tar -xz
sudo mv svs /usr/local/bin/

# macOS Intel
curl -L https://github.com/huiskylabs/solana-validator-switch/releases/latest/download/svs-macos-x86_64.tar.gz | tar -xz
sudo mv svs /usr/local/bin/

# macOS Apple Silicon
curl -L https://github.com/huiskylabs/solana-validator-switch/releases/latest/download/svs-macos-aarch64.tar.gz | tar -xz
sudo mv svs /usr/local/bin/
```

## Setup

```bash
# Create config directory
mkdir -p ~/.solana-validator-switch

# Download config template
curl -L https://raw.githubusercontent.com/huiskylabs/solana-validator-switch/main/config.example.yaml \
  -o ~/.solana-validator-switch/config.yaml

# Edit config
nano ~/.solana-validator-switch/config.yaml
```

## Verify

```bash
svs --version
```