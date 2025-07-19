# Telegram Alerts

## Setup

### 1. Create Bot

- Message [@BotFather](https://t.me/botfather)
- Send `/newbot`
- Save the token

### 2. Get Chat ID

- Add bot to group or start chat
- Send a test message
- Visit: `https://api.telegram.org/bot<TOKEN>/getUpdates`
- Find `"chat":{"id":-123456789}`

### 3. Configure

```yaml
alert_config:
  enabled: true
  delinquency_threshold_seconds: 30
  telegram:
    bot_token: "123456:ABC-DEF1234ghIkl-zyx57W2v1u123ew11"
    chat_id: "-123456789"  # Negative for groups
```

### 4. Test

```bash
svs test-alert
```

## Alert Types

- **Delinquency Alert** - Validator stops voting > 30s
- **Catchup Failure** - Standby fails 3 consecutive checks
- **Switch Result** - Success/failure notifications

## Cooldowns

- 5-minute cooldown between same alerts
- Prevents notification spam
- Resets on recovery