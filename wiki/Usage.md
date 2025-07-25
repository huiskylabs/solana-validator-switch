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
- Active/Standby status based on identity
- Current validator identity
- SSH connectivity status
- RPC node health via getHealth
- Vote status with slot info
- Alert configuration
- Swap readiness

### Interactive Status Mode

When in status view, use these keyboard shortcuts:
- `(Q)uit` - Exit the status view
- `(R)efresh` - Manually refresh all data immediately
- `(S)witch` - Initiate validator switch
- Auto-refresh occurs every 10 seconds with countdown timer in footer

## Switch Operation

1. **Pre-flight checks** - Verifies both nodes are ready
2. **Active → Unfunded** - Switches active node to unfunded identity
3. **Tower transfer** - Copies tower file to standby
4. **Standby → Funded** - Switches standby to funded identity
5. **Verification** - Confirms new active is voting

Total time: ~1 second average

## Keyboard Shortcuts

### Main Menu
- `Enter` - Select menu item
- Arrow keys - Navigate up/down
- `q` or `Esc` - Quit

### Status View
- `Q` - Quit to main menu
- `R` - Refresh all data immediately
- `S` - Start switch process
- `Esc` - Exit status view

### Switch Confirmation
- `y` - Confirm and proceed with switch
- `q` or `Esc` - Cancel switch operation