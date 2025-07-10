# Release Process

This document describes the release process for Solana Validator Switch CLI.

## Prerequisites

- Ensure all tests pass: `cargo test`
- Ensure code is formatted: `cargo fmt`
- Ensure no clippy warnings: `cargo clippy`
- Update CHANGELOG.md with release notes

## Creating a Release

1. **Update version and create tag:**
   ```bash
   ./scripts/create-release.sh 1.0.1
   ```

2. **Push changes and tag:**
   ```bash
   git push origin main
   git push origin v1.0.1
   ```

3. **Monitor GitHub Actions:**
   - Go to the [Actions tab](https://github.com/huiskylabs/solana-validator-switch/actions)
   - Watch the release workflow build binaries for all platforms

4. **Verify release:**
   - Check the [releases page](https://github.com/huiskylabs/solana-validator-switch/releases)
   - Ensure all binaries are uploaded
   - Test download links

## Platform Support

The release workflow builds binaries for:
- Linux x86_64
- Linux ARM64
- macOS x86_64 (Intel)
- macOS ARM64 (Apple Silicon)
- Windows x86_64

## Post-Release

1. Update documentation if needed
2. Announce release in relevant channels
3. Update Homebrew tap (if maintaining one separately)

## Troubleshooting

If the release workflow fails:
1. Check the workflow logs in GitHub Actions
2. Common issues:
   - Missing dependencies for cross-compilation
   - Cargo.toml version mismatch
   - Network issues during artifact upload

## Manual Release (Emergency)

If automated release fails, you can build manually:

```bash
# Build for current platform
cargo build --release

# Create archive
tar czf svs-$(rustc -vV | sed -n 's|host: ||p').tar.gz -C target/release svs

# Upload to GitHub releases manually
```