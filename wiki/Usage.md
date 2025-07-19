# Usage

## Interactive Mode (Recommended)

```bash
svs
```

Navigate the menu to:
- Check status
- Perform switch
- Test alerts

## Command Line Mode

```bash
svs status              # Check validator status
svs switch              # Perform validator switch
svs switch --dry-run    # Preview switch without executing
svs test-alert          # Test Telegram alerts
```

## Status Display

The status command shows:
- Validator type and version
- Active/Standby status
- Vote status with slot info
- Catchup status with countdown
- Alert configuration
- Swap readiness

## Switch Operation

1. **Pre-flight checks** - Verifies both nodes are ready
2. **Active → Unfunded** - Switches active node to unfunded identity
3. **Tower transfer** - Copies tower file to standby
4. **Standby → Funded** - Switches standby to funded identity
5. **Verification** - Confirms new active is voting

Total time: ~1 second average

## Keyboard Shortcuts

- `q` or `Esc` - Quit
- `Enter` - Select menu item
- Arrow keys - Navigate