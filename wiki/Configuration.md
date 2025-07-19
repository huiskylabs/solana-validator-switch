# Configuration

## Basic Configuration

Edit `~/.solana-validator-switch/config.yaml`:

```yaml
validators:
  - votePubkey: YOUR_VOTE_ACCOUNT_PUBKEY
    identityPubkey: YOUR_VALIDATOR_IDENTITY_PUBKEY
    rpc: https://api.mainnet-beta.solana.com
    nodes:
      - label: validator-1
        host: 10.0.0.1
        user: solana
        paths:
          fundedIdentity: /home/solana/funded-validator-keypair.json
          unfundedIdentity: /home/solana/unfunded-validator-keypair.json
          voteKeypair: /home/solana/vote-account-keypair.json
      
      - label: validator-2
        host: 10.0.0.2
        user: solana
        paths:
          fundedIdentity: /home/solana/funded-validator-keypair.json
          unfundedIdentity: /home/solana/unfunded-validator-keypair.json
          voteKeypair: /home/solana/vote-account-keypair.json
```

## Telegram Alerts (Optional)

```yaml
alert_config:
  enabled: true
  delinquency_threshold_seconds: 30
  telegram:
    bot_token: "YOUR_BOT_TOKEN"
    chat_id: "YOUR_CHAT_ID"
```

## SSH Requirements

- Key-based authentication required
- Common key locations auto-detected: `~/.ssh/id_rsa`, `~/.ssh/id_ed25519`
- Test SSH access: `ssh user@host`