# Security Policy

## Supported Versions

We release patches for security vulnerabilities. Which versions are eligible for receiving such patches depends on the CVSS v3.0 Rating:

| Version | Supported          |
| ------- | ------------------ |
| 1.2.x   | :white_check_mark: |
| 1.1.x   | :white_check_mark: |
| 1.0.x   | :x:                |
| < 1.0   | :x:                |

## Reporting a Vulnerability

The Solana Validator Switch team takes security bugs seriously. We appreciate your efforts to responsibly disclose your findings, and will make every effort to acknowledge your contributions.

To report a security vulnerability, please use one of the following methods:

### 1. GitHub Security Advisory (Preferred)

Create a security advisory on GitHub: [Report a vulnerability](https://github.com/huiskylabs/solana-validator-switch/security/advisories/new)

### 2. Email

Send an email to **security@huiskylabs.xyz** with:

- Type of issue (e.g., buffer overflow, SQL injection, cross-site scripting, etc.)
- Full paths of source file(s) related to the manifestation of the issue
- Location of affected source code (tag/branch/commit or direct URL)
- Step-by-step instructions to reproduce the issue
- Proof-of-concept or exploit code (if possible)
- Impact of the issue, including how an attacker might exploit it

### Response Timeline

- **Initial Response**: Within 48 hours
- **Status Update**: Within 5 business days
- **Resolution Target**: 
  - Critical: 7 days
  - High: 14 days
  - Medium: 30 days
  - Low: 60 days

## Security Best Practices for Users

### SSH Key Management

1. **Use Strong SSH Keys**
   - Generate keys with: `ssh-keygen -t ed25519 -a 100`
   - Use passphrases for additional security
   - Store keys in `~/.ssh/` with permissions 600

2. **SSH Agent Configuration**
   - Use `ssh-add` to manage keys securely
   - Configure agent forwarding carefully
   - Set appropriate timeout values

### Configuration Security

1. **File Permissions**
   ```bash
   chmod 600 ~/.solana-validator-switch/config.yaml
   ```

2. **Sensitive Data**
   - Never commit configuration files with actual values
   - Use environment variables for sensitive data when possible
   - Rotate Telegram bot tokens regularly

3. **Network Security**
   - Use SSH jump hosts for additional security
   - Implement fail2ban on validator nodes
   - Monitor SSH logs for unauthorized access attempts

### Operational Security

1. **Principle of Least Privilege**
   - Create dedicated user accounts for SVS operations
   - Limit sudo access to necessary commands only
   - Use SSH keys instead of passwords

2. **Monitoring and Alerting**
   - Enable Telegram alerts for delinquency detection
   - Monitor system logs for anomalies
   - Set up alerts for failed switch attempts

3. **Regular Updates**
   - Keep SVS updated to the latest version
   - Update system packages regularly
   - Monitor security advisories

## Security Features

### Built-in Security Measures

- **No Credential Storage**: SSH keys referenced by path only
- **Memory-Only Sessions**: No sensitive data persisted to disk
- **Secure File Operations**: Atomic operations with proper permissions
- **Input Validation**: All user inputs sanitized and validated
- **Error Handling**: Secure error messages without information leakage

### Network Security

- **SSH-Only Communication**: No custom network protocols
- **Local Execution**: All operations run locally
- **No External Dependencies**: No third-party services required
- **TLS for Telegram**: All alert communications use HTTPS

## Disclosure Policy

When we receive a security bug report, we will:

1. Confirm the problem and determine affected versions
2. Audit code to find similar problems
3. Prepare fixes for all supported versions
4. Release patches as soon as possible

We request that:

- You give us reasonable time to address the issue before public disclosure
- You make good faith efforts to avoid privacy violations and data destruction
- You provide sufficient information to reproduce the issue

## Security Acknowledgments

We maintain a list of security researchers who have responsibly disclosed vulnerabilities:

- *Your name could be here!*

## Contact

- Security Email: security@huiskylabs.xyz
- PGP Key: [Available on request]
- GitHub Security: [Security Advisories](https://github.com/huiskylabs/solana-validator-switch/security)

---

*This security policy is adapted from the [Rust Security Policy](https://www.rust-lang.org/policies/security) and follows industry best practices.*