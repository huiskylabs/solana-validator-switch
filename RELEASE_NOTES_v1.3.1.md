# Release Notes - v1.3.1

## Bug Fixes

### Improved Validator Executable Detection
- **Fixed version detection for validators running from PATH** - SVS now correctly detects validators that are executed without directory prefixes (e.g., just `agave-validator` instead of `/path/to/bin/agave-validator`)
- **More flexible Firedancer detection** - Improved fdctl executable path matching to support various installation methods
- **Removed restrictive path requirements** - No longer requires `bin/` or `release/` prefixes in executable paths

## What's Fixed

1. **Version Mismatch Issue**: Fixed the issue where SVS would show incorrect validator versions (e.g., showing 2.3.5 instead of 2.3.6) when the validator executable doesn't have a directory prefix in the process list

2. **Process Detection**: Improved the grep pattern to detect running validators regardless of how they were launched

3. **Firedancer Support**: Enhanced fdctl path detection to prevent "Firedancer fdctl executable path not found" errors

## Technical Details

- Changed executable detection from requiring specific path patterns like `bin/agave-validator` to accepting any path that ends with or contains the validator executable name
- Updated process detection grep patterns to be more inclusive
- Added debug logging for better troubleshooting of path detection issues

## Compatibility

This release maintains full backward compatibility with existing configurations and does not require any changes to your setup.