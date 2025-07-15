use anyhow::{anyhow, Result};
use std::process::Command;
use dirs::home_dir;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct SshKeyInfo {
    pub path: String,
    pub host: String,
    pub user: String,
}

/// Try to detect SSH key by using ssh -vv and parsing the output
pub async fn detect_ssh_key(host: &str, user: &str) -> Result<String> {
    // Always use verbose SSH to detect the actual key being used
    extract_key_from_verbose_ssh(host, user).await
}

/// Try an SSH connection with optional key path
async fn try_ssh_connection(host: &str, user: &str, key_path: Option<&str>) -> Result<bool> {
    let mut cmd = Command::new("ssh");
    cmd.arg("-o").arg("BatchMode=yes")
       .arg("-o").arg("ConnectTimeout=5")
       .arg("-o").arg("StrictHostKeyChecking=no")
       .arg("-o").arg("PasswordAuthentication=no");
    
    if let Some(key) = key_path {
        cmd.arg("-i").arg(key);
    }
    
    cmd.arg(format!("{}@{}", user, host))
       .arg("exit");
    
    let output = cmd.output()?;
    Ok(output.status.success())
}

/// Extract the working SSH key path from verbose SSH output
async fn extract_key_from_verbose_ssh(host: &str, user: &str) -> Result<String> {
    let output = Command::new("ssh")
        .arg("-vv")  // Double verbose is enough
        .arg("-o").arg("BatchMode=yes")
        .arg("-o").arg("ConnectTimeout=5")
        .arg("-o").arg("StrictHostKeyChecking=no")
        .arg("-o").arg("PasswordAuthentication=no")
        .arg(format!("{}@{}", user, host))
        .arg("exit")
        .output()?;
    
    let stderr = String::from_utf8_lossy(&output.stderr);
    
    // Look for patterns in SSH verbose output:
    // OpenSSH patterns:
    // "debug1: Offering public key: /path/to/key RSA SHA256:..."
    // "debug1: Server accepts key: /path/to/key RSA SHA256:..."
    // "debug1: Authentication succeeded (publickey)."
    // "debug1: Authenticating with public key \"/path/to/key\""
    // "debug1: Will attempt key: /path/to/key RSA SHA256:... explicit"
    
    let lines = stderr.lines().collect::<Vec<_>>();
    
    // First, check if authentication succeeded
    let auth_succeeded = lines.iter().any(|line| 
        line.contains("Authentication succeeded (publickey)") ||
        line.contains("Authenticated to")
    );
    
    if !auth_succeeded {
        // If auth failed, still try to find what key was attempted
        for line in &lines {
            if line.contains("Permission denied") {
                return Err(anyhow!("SSH authentication failed - permission denied"));
            }
        }
        return Err(anyhow!("SSH authentication failed"));
    }
    
    // Look for the accepted key by working backwards from authentication success
    let mut accepted_key: Option<String> = None;
    
    // Pattern 1: "Server accepts key:" (most reliable)
    for line in &lines {
        if line.contains("Server accepts key:") {
            if let Some(path) = extract_key_path_from_accepts_line(line) {
                accepted_key = Some(path);
                break;
            }
        }
    }
    
    // Pattern 2: "Authenticating with public key"
    if accepted_key.is_none() {
        for line in &lines {
            if line.contains("Authenticating with public key") {
                if let Some(path) = extract_key_path_from_auth_line(line) {
                    accepted_key = Some(path);
                    break;
                }
            }
        }
    }
    
    // Pattern 3: Look for successful "Offering" followed by no rejection
    if accepted_key.is_none() {
        for (i, line) in lines.iter().enumerate() {
            if line.contains("Offering public key:") || line.contains("Offering RSA public key:") {
                if let Some(path) = extract_path_from_offering_line(line) {
                    // Check if this key was not rejected
                    let mut was_rejected = false;
                    for j in i+1..lines.len().min(i+10) {
                        if lines[j].contains("Server accepts key") {
                            accepted_key = Some(path.clone());
                            break;
                        }
                        if lines[j].contains("key_verify failed") || 
                           lines[j].contains("send_pubkey_test: no mutual signature") {
                            was_rejected = true;
                            break;
                        }
                    }
                    if accepted_key.is_some() {
                        break;
                    }
                    if !was_rejected && auth_succeeded {
                        // If auth succeeded and this key wasn't rejected, it's likely the one
                        accepted_key = Some(path);
                    }
                }
            }
        }
    }
    
    // Pattern 4: SSH agent keys
    if accepted_key.is_none() {
        for line in &lines {
            if line.contains("Will attempt key:") && line.contains("agent") {
                if let Some(path) = extract_agent_key_comment(line) {
                    accepted_key = Some(format!("ssh-agent: {}", path));
                    break;
                }
            }
        }
    }
    
    accepted_key.ok_or_else(|| anyhow!(
        "Could not determine SSH key from verbose output. SSH succeeded but key path not found."
    ))
}

/// Extract key path from "Server accepts key:" line
fn extract_key_path_from_accepts_line(line: &str) -> Option<String> {
    // Pattern: "debug1: Server accepts key: /path/to/key RSA SHA256:..."
    if let Some(start) = line.find("Server accepts key:") {
        let after = &line[start + 19..].trim();
        if let Some(space_idx) = after.find(' ') {
            let path = after[..space_idx].trim();
            if path.starts_with('/') || path.starts_with("~") {
                return expand_tilde(path).ok();
            }
        }
    }
    None
}

/// Extract key path from "Authenticating with public key" line
fn extract_key_path_from_auth_line(line: &str) -> Option<String> {
    // Pattern: "debug1: Authenticating with public key \"/path/to/key\""
    if let Some(start) = line.find("Authenticating with public key") {
        let after = &line[start + 30..].trim();
        // Remove quotes if present
        let path = after.trim_matches('"').trim();
        if path.starts_with('/') || path.starts_with("~") {
            return expand_tilde(path).ok();
        }
    }
    None
}

/// Extract path from "Offering public key:" line
fn extract_path_from_offering_line(line: &str) -> Option<String> {
    // Pattern: "debug1: Offering public key: /path/to/key RSA SHA256:..."
    // or: "debug1: Offering RSA public key: /path/to/key"
    if let Some(start) = line.find("public key:") {
        let after = &line[start + 11..].trim();
        // Handle both patterns with and without key type after path
        let path = if let Some(space_idx) = after.find(' ') {
            after[..space_idx].trim()
        } else {
            after
        };
        if path.starts_with('/') || path.starts_with("~") {
            return expand_tilde(path).ok();
        }
    }
    None
}

/// Extract agent key comment
fn extract_agent_key_comment(line: &str) -> Option<String> {
    // Pattern: "debug1: Will attempt key: user@host RSA SHA256:... agent"
    if let Some(start) = line.find("Will attempt key:") {
        let after = &line[start + 17..].trim();
        if let Some(end) = after.find(" agent") {
            let key_info = after[..end].trim();
            // Just return the comment part before the key type
            if let Some(space) = key_info.rfind(' ') {
                return Some(key_info[..space].trim().to_string());
            }
        }
    }
    None
}

/// Extract path from identity file line
fn extract_identity_file_path(line: &str) -> Option<String> {
    // Pattern: "debug1: identity file /Users/username/.ssh/id_rsa type 0"
    if let Some(start) = line.find("identity file") {
        let after = &line[start + 13..].trim();
        if let Some(type_idx) = after.find(" type") {
            let path = after[..type_idx].trim();
            if path.starts_with('/') || path.starts_with("~") {
                return Some(path.to_string());
            }
        }
    }
    None
}

/// Expand tilde (~) to home directory
fn expand_tilde(path: &str) -> Result<String> {
    if path.starts_with("~/") {
        let home = home_dir().ok_or_else(|| anyhow!("Could not determine home directory"))?;
        Ok(path.replacen("~", &home.to_string_lossy(), 1))
    } else {
        Ok(path.to_string())
    }
}

/// Try to auto-detect SSH keys for all nodes in the config
#[allow(dead_code)]
pub async fn auto_detect_ssh_keys(nodes: &[(String, String)]) -> Vec<Result<SshKeyInfo>> {
    let mut results = Vec::new();
    
    for (host, user) in nodes {
        match detect_ssh_key(host, user).await {
            Ok(key_path) => {
                results.push(Ok(SshKeyInfo {
                    path: key_path,
                    host: host.clone(),
                    user: user.clone(),
                }));
            }
            Err(e) => {
                results.push(Err(e));
            }
        }
    }
    
    results
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_extract_path_from_offering_line() {
        let line = "debug1: Offering public key: /Users/test/.ssh/id_rsa RSA SHA256:abcd";
        assert_eq!(
            extract_path_from_offering_line(line),
            Some("/Users/test/.ssh/id_rsa".to_string())
        );
    }
    
    #[test]
    fn test_extract_key_path_from_accepts_line() {
        let line = "debug1: Server accepts key: /Users/test/.ssh/id_ed25519 ED25519 SHA256:xyz";
        assert_eq!(
            extract_key_path_from_accepts_line(line),
            Some("/Users/test/.ssh/id_ed25519".to_string())
        );
    }
    
    #[test]
    fn test_extract_key_path_from_auth_line() {
        let line = "debug1: Authenticating with public key \"/Users/test/.ssh/id_rsa\"";
        assert_eq!(
            extract_key_path_from_auth_line(line),
            Some("/Users/test/.ssh/id_rsa".to_string())
        );
    }
    
    #[test]
    fn test_extract_identity_file_path() {
        let line = "debug1: identity file /Users/test/.ssh/id_rsa type 0";
        assert_eq!(
            extract_identity_file_path(line),
            Some("/Users/test/.ssh/id_rsa".to_string())
        );
    }
}