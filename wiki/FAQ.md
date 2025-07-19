# FAQ

## General

**Q: What validators does SVS support?**
A: Firedancer, Agave, Jito, and Solana validators. Auto-detected at runtime.

**Q: How fast is the switch?**
A: Average ~1 second for the identity switch. Full operation including verification: 30-45 seconds.

**Q: Does it work with multiple validators?**
A: Yes, configure multiple validator pairs in config.yaml.

## Troubleshooting

**Q: SSH connection failed**
A: Ensure key-based SSH works: `ssh user@host`. Check firewall rules.

**Q: Swap not ready**
A: Verify all keypair files exist and are readable. Check tower file exists in ledger.

**Q: Telegram alerts not working**
A: Run `svs test-alert`. Check bot token and chat ID are correct.

**Q: Status not updating after switch**
A: Fixed in v1.2.0. Update to latest version.

## Security

**Q: Are my keys safe?**
A: Yes. SVS only stores paths to keys, never the keys themselves.

**Q: What ports are needed?**
A: Only SSH (port 22 by default) to your validator nodes.

**Q: Can I use password authentication?**
A: No, only SSH key authentication is supported for security.