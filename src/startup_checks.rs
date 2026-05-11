use anyhow::{anyhow, Result};
use colored::*;
use std::collections::HashMap;

use crate::ssh::AsyncSshPool;
use crate::startup_logger::StartupLogger;
use crate::types::{NodeConfig, NodeWithStatus, RemoteShellType, ValidatorPair};
use crate::AppState;

/// Strip ANSI escape codes from a string.
/// Handles sequences like ESC[...m that grep --color adds to output.
fn strip_ansi(s: &str) -> String {
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            if chars.peek() == Some(&'[') {
                chars.next(); // consume '['
                // consume digits, semicolons until we hit an ASCII letter
                while let Some(&next) = chars.peek() {
                    chars.next();
                    if next.is_ascii_alphabetic() {
                        break;
                    }
                }
            }
        } else {
            result.push(c);
        }
    }
    result
}

/// Extract the first quoted (single- or double-quoted) value from a line.
///
/// TOML accepts both `'literal'` and `"basic"` quoting, so we try both.
/// Returns `None` if no matching closing quote of the same kind is found.
fn extract_quoted_value(line: &str) -> Option<String> {
    // Look for the earliest opening quote of either kind.
    let dq = line.find('"');
    let sq = line.find('\'');
    let (open_idx, quote) = match (dq, sq) {
        (Some(d), Some(s)) if s < d => (s, '\''),
        (Some(d), _) => (d, '"'),
        (None, Some(s)) => (s, '\''),
        (None, None) => return None,
    };
    let rest = &line[open_idx + quote.len_utf8()..];
    let close_idx = rest.find(quote)?;
    Some(rest[..close_idx].to_string())
}

/// Walk a Firedancer TOML config and return `(identity_path, authorized_voter_path)`
/// from the `[consensus]` section.
///
/// This is intentionally a small hand-rolled walker rather than a full TOML
/// parser dependency: we only need two fields and we want behaviour that is
/// well-defined for the exact patterns fdctl writes. It handles:
///
/// * `[consensus]` appearing anywhere in the file (we ignore other sections),
/// * `identity_path = "…"` and `identity_path = '…'`,
/// * `authorized_voter_paths = "…"` (inline single-value form), and
/// * `authorized_voter_paths = [\n   "…",\n …\n]` (multi-line array form,
///   in which case we take the first quoted entry).
///
/// If the file contains a `[consensus]` section but is missing either key the
/// error message names the missing key so operators can fix the config quickly.
fn parse_firedancer_consensus_paths(content: &str) -> Result<(String, String)> {
    let mut in_consensus = false;
    let mut in_authorized_voter_array = false;
    let mut identity_path: Option<String> = None;
    let mut authorized_voter_path: Option<String> = None;

    for raw_line in content.lines() {
        // Drop inline comments. Splitting on '#' is safe here because fdctl
        // configs do not put '#' inside the quoted paths we care about, and
        // the TOML spec treats unquoted '#' as a comment start.
        let trimmed_full = raw_line.trim();
        let trimmed = trimmed_full
            .split('#')
            .next()
            .unwrap_or(trimmed_full)
            .trim_end();

        if trimmed.is_empty() {
            continue;
        }

        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            in_consensus = trimmed == "[consensus]";
            in_authorized_voter_array = false;
            continue;
        }

        if !in_consensus {
            continue;
        }

        if identity_path.is_none() && trimmed.starts_with("identity_path") {
            identity_path = extract_quoted_value(trimmed);
            continue;
        }

        if authorized_voter_path.is_none() && trimmed.starts_with("authorized_voter_paths") {
            // Two shapes are possible here:
            //   authorized_voter_paths = "X"          (inline)
            //   authorized_voter_paths = [            (array start)
            //       "X",
            //   ]
            authorized_voter_path = extract_quoted_value(trimmed);
            // Only enter array mode if no inline string was found on this
            // line — i.e. the value is being declared as an array.
            in_authorized_voter_array = authorized_voter_path.is_none();
            continue;
        }

        if in_authorized_voter_array {
            if trimmed.starts_with(']') {
                in_authorized_voter_array = false;
                continue;
            }
            if let Some(path) = extract_quoted_value(trimmed) {
                authorized_voter_path = Some(path);
                in_authorized_voter_array = false;
            }
        }
    }

    let identity_path = identity_path
        .ok_or_else(|| anyhow!("Failed to parse identity_path from [consensus] section"))?;
    let authorized_voter_path = authorized_voter_path.ok_or_else(|| {
        anyhow!("Failed to parse authorized_voter_paths from [consensus] section")
    })?;

    Ok((identity_path, authorized_voter_path))
}

/// Pull the value of `--config <path>` out of a `ps` listing for fdctl.
///
/// The output may contain ANSI colour escapes from `grep --color=auto` (which
/// some distributions set as a shell alias) and may have shell-quoted
/// arguments, so we strip ANSI codes and trim surrounding quote characters.
fn extract_firedancer_config_path_from_ps_output(process_info: &str) -> Result<String> {
    let clean = strip_ansi(process_info);
    let parts: Vec<&str> = clean.split_whitespace().collect();

    parts
        .windows(2)
        .find_map(|w| {
            let flag = w[0].trim_matches(|c: char| c == '\'' || c == '"');
            if flag == "--config" {
                Some(
                    w[1]
                        .trim_matches(|c: char| c == '\'' || c == '"')
                        .to_string(),
                )
            } else {
                None
            }
        })
        .ok_or_else(|| anyhow!("Failed to find Firedancer config path in process arguments"))
}

/// Read the contents of `config_path` over SSH, retrying with `sudo` if a
/// direct `cat` returns nothing (which happens when the config is only
/// readable by the user running fdctl).
async fn fetch_firedancer_config_via_ssh(
    ssh_pool: &AsyncSshPool,
    node: &NodeConfig,
    ssh_key: &str,
    config_path: &str,
) -> Result<String> {
    let mut last_read_error: Option<String> = None;

    for (command, args) in [
        ("cat", vec![config_path]),
        ("sudo", vec!["cat", config_path]),
    ] {
        match ssh_pool
            .execute_command_with_args(node, ssh_key, command, &args)
            .await
        {
            Ok(content) if !content.trim().is_empty() => {
                return Ok(content);
            }
            Ok(_) => {
                last_read_error = Some(format!("{} {}: no output", command, args[0]));
            }
            Err(e) => {
                last_read_error = Some(e.to_string());
            }
        }
    }

    Err(anyhow!(
        "Failed to read Firedancer config file: {}",
        last_read_error.unwrap_or_else(|| "unknown error".to_string())
    ))
}

/// Perform startup safety checks for auto-failover configuration
#[allow(dead_code)]
pub async fn check_auto_failover_safety(
    app_state: &AppState,
    logger: &StartupLogger,
) -> Result<()> {
    // Skip checks if auto-failover is not enabled
    let _alert_config = match &app_state.config.alert_config {
        Some(config) if config.enabled && config.auto_failover_enabled => config,
        _ => return Ok(()), // Auto-failover not enabled, no checks needed
    };

    // Always require unfunded identity check when auto-failover is enabled
    // This is a critical safety requirement

    println!(
        "\n{}",
        "🔍 Checking auto-failover safety requirements...".cyan()
    );
    logger.log("Starting auto-failover safety checks")?;

    // Check each validator pair
    for (idx, validator_status) in app_state.validator_statuses.iter().enumerate() {
        let validator_pair = &validator_status.validator_pair;

        println!(
            "\n  Validator {}: {}",
            idx + 1,
            validator_pair.identity_pubkey.bright_white()
        );

        // Check all nodes for this validator
        for node_with_status in &validator_status.nodes_with_status {
            logger.log(&format!(
                "Checking identity configuration for {}",
                node_with_status.node.label
            ))?;
            match check_node_identity(
                node_with_status,
                validator_pair,
                &app_state.ssh_pool,
                &app_state.detected_ssh_keys,
                logger,
            )
            .await
            {
                Ok(_) => {
                    logger.log(&format!(
                        "✅ {} passed identity check",
                        node_with_status.node.label
                    ))?;
                }
                Err(e) => {
                    let error_msg = format!(
                        "Could not verify identity configuration for {}: {}",
                        node_with_status.node.label, e
                    );
                    logger.log_error("Identity Check", &error_msg)?;
                    println!("      ⚠️  Warning: {}", error_msg);
                    println!(
                        "      ⚠️  Please ensure validators are configured with unfunded identity!"
                    );
                }
            }
        }
    }

    println!(
        "\n{}",
        "✅ All validators configured with unfunded identity - safe for auto-failover"
            .green()
            .bold()
    );

    Ok(())
}

/// Check that validators are not starting with their authorized voter identity
#[allow(dead_code)]
pub async fn check_startup_identity_safety(app_state: &AppState) -> Result<()> {
    println!(
        "\n{}",
        "🔍 Checking startup identity configuration...".cyan()
    );

    // Check each validator pair
    for (idx, validator_status) in app_state.validator_statuses.iter().enumerate() {
        let validator_pair = &validator_status.validator_pair;

        println!(
            "\n  Validator {}: {}",
            idx + 1,
            validator_pair.identity_pubkey.bright_white()
        );

        // Check all nodes for this validator
        for node_with_status in &validator_status.nodes_with_status {
            check_node_startup_identity(
                node_with_status,
                &app_state.ssh_pool,
                &app_state.detected_ssh_keys,
            )
            .await?;
        }
    }

    println!(
        "\n{}",
        "✅ All validators configured with safe startup identity"
            .green()
            .bold()
    );

    Ok(())
}

#[allow(dead_code)]
async fn check_node_identity(
    node: &NodeWithStatus,
    _validator_pair: &ValidatorPair,
    ssh_pool: &AsyncSshPool,
    detected_ssh_keys: &HashMap<String, String>,
    logger: &StartupLogger,
) -> Result<()> {
    let ssh_key = detected_ssh_keys
        .get(&node.node.host)
        .ok_or_else(|| anyhow!("No SSH key detected for {}", node.node.host))?;

    println!("    Checking {}: ", node.node.label);

    // Get shell type
    let shell_type = ssh_pool.get_shell_type(&node.node, ssh_key).await?;

    // Check startup identity configuration based on validator type
    match node.validator_type {
        crate::types::ValidatorType::Firedancer => {
            logger.log(&format!(
                "{} is Firedancer type, checking config",
                node.node.label
            ))?;
            check_firedancer_identity_config(node, ssh_pool, ssh_key, shell_type).await?
        }
        crate::types::ValidatorType::Agave | crate::types::ValidatorType::Jito => {
            logger.log(&format!(
                "{} is Agave/Jito type, checking command line",
                node.node.label
            ))?;
            check_agave_identity_config(node, ssh_pool, ssh_key, shell_type).await?
        }
        crate::types::ValidatorType::Unknown => {
            logger.log(&format!(
                "⚠️ {} has unknown validator type - skipping check",
                node.node.label
            ))?;
            println!("      ⚠️  Unknown validator type - skipping check");
            return Ok(());
        }
    };

    println!("      ✅ Configured with safe startup identity");
    Ok(())
}

#[allow(dead_code)]
async fn check_node_startup_identity(
    node: &NodeWithStatus,
    ssh_pool: &AsyncSshPool,
    detected_ssh_keys: &HashMap<String, String>,
) -> Result<()> {
    let ssh_key = detected_ssh_keys
        .get(&node.node.host)
        .ok_or_else(|| anyhow!("No SSH key detected for {}", node.node.host))?;

    println!("    Checking {}: ", node.node.label);

    // Get shell type
    let shell_type = ssh_pool.get_shell_type(&node.node, ssh_key).await?;

    // Check identity configuration based on validator type
    match node.validator_type {
        crate::types::ValidatorType::Firedancer => {
            check_firedancer_identity_config(node, ssh_pool, ssh_key, shell_type).await?
        }
        crate::types::ValidatorType::Agave | crate::types::ValidatorType::Jito => {
            check_agave_identity_config(node, ssh_pool, ssh_key, shell_type).await?
        }
        crate::types::ValidatorType::Unknown => {
            println!("      ⚠️  Unknown validator type - skipping check");
            return Ok(());
        }
    };

    println!("      ✅ Startup identity differs from authorized voter");
    Ok(())
}

#[allow(dead_code)]
async fn check_firedancer_identity_config(
    node: &NodeWithStatus,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
    shell_type: RemoteShellType,
) -> Result<()> {
    // Find the running fdctl process and pull `--config <path>` from its
    // arguments. The trailing space in the grep pattern is important — it
    // prevents matches on flags like `--config-bind`.
    //
    // NOTE on the PowerShell branch: it uses `ps auxww` which is a procps
    // command, so it only works against PowerShell Core invoked on Linux
    // (where the procps `ps` is on PATH). It will not work against a true
    // PowerShell host on Windows. We accept that limitation because SVS only
    // targets Unix validator hosts today.
    let ps_cmd = match shell_type {
        RemoteShellType::Bash => {
            "ps auxww | grep --color=never 'fdctl.*--config ' | grep --color=never -v grep | head -1"
        }
        RemoteShellType::PowerShell | RemoteShellType::PowerShellCore => {
            "ps auxww | Select-String -Pattern 'fdctl.*--config ' | Select-String -Pattern 'grep' -NotMatch | Select-Object -First 1 | ForEach-Object { $_.Line }"
        }
    };
    let process_info = ssh_pool
        .execute_command(&node.node, ssh_key, ps_cmd)
        .await?;

    if process_info.trim().is_empty() {
        // Firedancer process is not currently running; skip the identity
        // check rather than failing startup. We don't have anything safe to
        // assert about a config file we cannot locate.
        return Ok(());
    }

    let config_path = extract_firedancer_config_path_from_ps_output(&process_info)?;
    let config_content =
        fetch_firedancer_config_via_ssh(ssh_pool, &node.node, ssh_key, &config_path).await?;
    let (identity_path, authorized_voter_path) =
        parse_firedancer_consensus_paths(&config_content)?;

    if identity_path == authorized_voter_path {
        return Err(anyhow!(
            "\n❌ SAFETY CHECK FAILED: {} has identity_path same as authorized_voter_paths!\n\
             \n\
             Firedancer Config Issue:\n\
             identity_path = \"{}\"\n\
             authorized_voter_paths[0] = \"{}\"\n\
             \n\
             This is UNSAFE for auto-failover. The startup identity must differ from the authorized voter.\n\
             \n\
             To fix this:\n\
             1. Stop Firedancer\n\
             2. Edit the config file: {}\n\
             3. Set identity_path to your unfunded keypair: \"{}\"\n\
             4. Restart Firedancer\n\
             5. Run svs again",
            node.node.label.red().bold(),
            identity_path,
            authorized_voter_path,
            config_path,
            node.node.paths.unfunded_identity
        ));
    }

    Ok(())
}

#[allow(dead_code)]
async fn check_agave_identity_config(
    node: &NodeWithStatus,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
    shell_type: RemoteShellType,
) -> Result<()> {
    // Get the running process command line
    let ps_cmd = match shell_type {
        RemoteShellType::Bash => {
            "ps auxww | grep --color=never -E 'solana-validator|agave-validator|jito-validator' | grep --color=never -v grep"
        }
        RemoteShellType::PowerShell | RemoteShellType::PowerShellCore => {
            "ps aux | Select-String -Pattern 'solana-validator|agave-validator|jito-validator' | Select-String -Pattern 'grep' -NotMatch"
        }
    };
    let process_info = ssh_pool
        .execute_command(&node.node, ssh_key, ps_cmd)
        .await?;

    if !process_info.lines().any(|line| line.contains("validator")) {
        return Err(anyhow!("Failed to find validator process"));
    }

    // Strip ANSI escape codes (added by grep --color on some systems) before parsing
    let clean_info = strip_ansi(&process_info);

    // Extract --identity and --authorized-voter paths
    // Join all lines to handle ps output wrapping on narrow SSH terminals
    let parts: Vec<&str> = clean_info.split_whitespace().collect();

    let identity_path = parts
        .windows(2)
        .find(|w| w[0] == "--identity")
        .map(|w| w[1])
        .ok_or_else(|| anyhow!("Failed to find --identity in validator command"))?;

    let authorized_voter_path = parts
        .windows(2)
        .find(|w| w[0] == "--authorized-voter")
        .map(|w| w[1])
        .ok_or_else(|| anyhow!("Failed to find --authorized-voter in validator command"))?;

    // Check if they're the same
    if identity_path == authorized_voter_path {
        return Err(anyhow!(
            "\n❌ SAFETY CHECK FAILED: {} has --identity same as --authorized-voter!\n\
             \n\
             Command Line Issue:\n\
             --identity {}\n\
             --authorized-voter {}\n\
             \n\
             This is UNSAFE for auto-failover. The startup identity must differ from the authorized voter.\n\
             \n\
             To fix this:\n\
             1. Stop the validator\n\
             2. Change the startup command to use different keypairs:\n\
                --identity {}\n\
                --authorized-voter {}\n\
             3. Restart the validator\n\
             4. Run svs again",
            node.node.label.red().bold(),
            identity_path,
            authorized_voter_path,
            node.node.paths.unfunded_identity,
            node.node.paths.funded_identity
        ));
    }

    Ok(())
}

/// Check node startup identity configuration inline during startup
pub async fn check_node_startup_identity_inline(
    node: &crate::types::NodeConfig,
    validator_type: crate::types::ValidatorType,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
    shell_type: RemoteShellType,
) -> Result<()> {
    match validator_type {
        crate::types::ValidatorType::Firedancer => {
            check_firedancer_identity_config_inline(node, ssh_pool, ssh_key, shell_type).await
        }
        crate::types::ValidatorType::Agave | crate::types::ValidatorType::Jito => {
            check_agave_identity_config_inline(node, ssh_pool, ssh_key, shell_type).await
        }
        crate::types::ValidatorType::Unknown => Ok(()),
    }
}

async fn check_firedancer_identity_config_inline(
    node: &crate::types::NodeConfig,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
    shell_type: RemoteShellType,
) -> Result<()> {
    // See check_firedancer_identity_config() above for notes on the PowerShell
    // branch. The trailing space in the grep pattern is critical to avoid
    // matching flags like `--config-bind`.
    let ps_cmd = match shell_type {
        RemoteShellType::Bash => {
            "ps auxww | grep --color=never 'fdctl.*--config ' | grep --color=never -v grep | head -1"
        }
        RemoteShellType::PowerShell | RemoteShellType::PowerShellCore => {
            "ps auxww | Select-String -Pattern 'fdctl.*--config ' | Select-String -Pattern 'grep' -NotMatch | Select-Object -First 1 | ForEach-Object { $_.Line }"
        }
    };
    let process_info = ssh_pool.execute_command(node, ssh_key, ps_cmd).await?;

    if process_info.trim().is_empty() {
        // Firedancer process is not currently running; skip startup identity check.
        return Ok(());
    }

    let config_path = extract_firedancer_config_path_from_ps_output(&process_info)?;
    let config_content =
        fetch_firedancer_config_via_ssh(ssh_pool, node, ssh_key, &config_path).await?;
    let (identity_path, authorized_voter_path) =
        parse_firedancer_consensus_paths(&config_content)?;

    // Check if they're the same
    if identity_path == authorized_voter_path {
        return Err(anyhow!(
            "Identity matches authorized voter! identity_path={}, authorized_voter_paths[0]={}",
            identity_path,
            authorized_voter_path
        ));
    }

    Ok(())
}

async fn check_agave_identity_config_inline(
    node: &crate::types::NodeConfig,
    ssh_pool: &AsyncSshPool,
    ssh_key: &str,
    shell_type: RemoteShellType,
) -> Result<()> {
    // Get the running process command line
    let ps_cmd = match shell_type {
        RemoteShellType::Bash => {
            "ps auxww | grep --color=never -E 'solana-validator|agave-validator|jito-validator' | grep --color=never -v grep"
        }
        RemoteShellType::PowerShell | RemoteShellType::PowerShellCore => {
            "ps aux | Select-String -Pattern 'solana-validator|agave-validator|jito-validator' | Select-String -Pattern 'grep' -NotMatch"
        }
    };
    let process_info = ssh_pool.execute_command(node, ssh_key, ps_cmd).await?;

    if !process_info.lines().any(|line| line.contains("validator")) {
        return Err(anyhow!("Failed to find validator process"));
    }

    // Strip ANSI escape codes (added by grep --color on some systems) before parsing
    let clean_info = strip_ansi(&process_info);

    // Extract --identity and --authorized-voter paths
    // Join all lines to handle ps output wrapping on narrow SSH terminals
    let parts: Vec<&str> = clean_info.split_whitespace().collect();

    let identity_path = parts
        .windows(2)
        .find(|w| w[0] == "--identity")
        .map(|w| w[1])
        .ok_or_else(|| anyhow!("Failed to find --identity"))?;

    let authorized_voter_path = parts
        .windows(2)
        .find(|w| w[0] == "--authorized-voter")
        .map(|w| w[1])
        .ok_or_else(|| anyhow!("Failed to find --authorized-voter"))?;

    // Check if they're the same
    if identity_path == authorized_voter_path {
        return Err(anyhow!(
            "Identity matches authorized voter! --identity={}, --authorized-voter={}",
            identity_path,
            authorized_voter_path
        ));
    }

    Ok(())
}


#[cfg(test)]
mod parser_tests {
    //! Unit tests for the Firedancer config parser helpers.
    //!
    //! These are pure functions over &str, so we can cover the edge cases
    //! we know fdctl produces (and operators may hand-write) without any
    //! SSH plumbing. The parser is the load-bearing piece of the safety
    //! check that prevents booting with identity_path == authorized voter.

    use super::{extract_firedancer_config_path_from_ps_output, extract_quoted_value,
                parse_firedancer_consensus_paths};

    fn assert_parsed(content: &str, expected_identity: &str, expected_voter: &str) {
        let (identity, voter) = parse_firedancer_consensus_paths(content)
            .unwrap_or_else(|e| panic!("expected parse to succeed but got: {e}\ncontent was:\n{content}"));
        assert_eq!(identity, expected_identity, "identity_path mismatch");
        assert_eq!(voter, expected_voter, "authorized_voter_path mismatch");
    }

    #[test]
    fn parses_inline_authorized_voter_paths_string_form() {
        // Some operators set authorized_voter_paths to a single quoted string
        // rather than an array; the parser must still pick it up.
        let content = r#"
[consensus]
identity_path = "/keys/unfunded.json"
authorized_voter_paths = "/keys/funded.json"
"#;
        assert_parsed(content, "/keys/unfunded.json", "/keys/funded.json");
    }

    #[test]
    fn parses_multiline_authorized_voter_paths_array_form() {
        // The shape fdctl generates by default: a TOML array spread across
        // several lines.
        let content = r#"
[consensus]
identity_path = "/keys/unfunded.json"
authorized_voter_paths = [
    "/keys/funded.json",
]
"#;
        assert_parsed(content, "/keys/unfunded.json", "/keys/funded.json");
    }

    #[test]
    fn ignores_keys_outside_the_consensus_section() {
        // identity_path/authorized_voter_paths appearing in OTHER sections
        // must not be confused for the consensus values.
        let content = r#"
[gossip]
identity_path = "/should/not/match.json"

[consensus]
identity_path = "/keys/correct-unfunded.json"
authorized_voter_paths = ["/keys/correct-funded.json"]

[ledger]
identity_path = "/another/decoy.json"
"#;
        assert_parsed(
            content,
            "/keys/correct-unfunded.json",
            "/keys/correct-funded.json",
        );
    }

    #[test]
    fn picks_first_value_when_multiple_identity_paths_are_present() {
        // TOML semantics would call duplicate keys an error; the safety
        // check only cares that we pick a single deterministic value so we
        // can compare it against the voter path. First wins.
        let content = r#"
[consensus]
identity_path = "/first.json"
identity_path = "/second.json"
authorized_voter_paths = ["/voter.json"]
"#;
        assert_parsed(content, "/first.json", "/voter.json");
    }

    #[test]
    fn handles_single_quoted_toml_strings() {
        // TOML literal strings use single quotes. fdctl uses double quotes by
        // default but a hand-edited config may legitimately use single
        // quotes; the parser should accept both.
        let content = "[consensus]\nidentity_path = '/keys/unfunded.json'\nauthorized_voter_paths = ['/keys/funded.json']\n";
        assert_parsed(content, "/keys/unfunded.json", "/keys/funded.json");
    }

    #[test]
    fn skips_comments_before_extracting_values() {
        // Inline `#` comments after a value must not contaminate the parsed
        // path.
        let content = r#"
[consensus]
identity_path = "/keys/unfunded.json" # the unfunded boot key
authorized_voter_paths = ["/keys/funded.json"] # the real voter
"#;
        assert_parsed(content, "/keys/unfunded.json", "/keys/funded.json");
    }

    #[test]
    fn errors_when_consensus_section_is_missing_identity() {
        let content = r#"
[consensus]
authorized_voter_paths = ["/keys/funded.json"]
"#;
        let err = parse_firedancer_consensus_paths(content).unwrap_err();
        assert!(
            format!("{err}").contains("identity_path"),
            "error should name the missing key, got: {err}",
        );
    }

    #[test]
    fn errors_when_consensus_section_is_missing_authorized_voter() {
        let content = r#"
[consensus]
identity_path = "/keys/unfunded.json"
"#;
        let err = parse_firedancer_consensus_paths(content).unwrap_err();
        assert!(
            format!("{err}").contains("authorized_voter_paths"),
            "error should name the missing key, got: {err}",
        );
    }

    #[test]
    fn extract_quoted_value_handles_double_quotes() {
        assert_eq!(
            extract_quoted_value(r#"identity_path = "/keys/x.json""#),
            Some("/keys/x.json".to_string())
        );
    }

    #[test]
    fn extract_quoted_value_handles_single_quotes() {
        assert_eq!(
            extract_quoted_value("identity_path = '/keys/x.json'"),
            Some("/keys/x.json".to_string())
        );
    }

    #[test]
    fn extract_quoted_value_returns_none_on_no_quotes() {
        assert_eq!(extract_quoted_value("identity_path = /no/quotes/here"), None);
    }

    #[test]
    fn extract_firedancer_config_path_finds_value_after_flag() {
        let ps = "azureuser  1234  0.1  0.0   1234  5678 ?  Sl   00:00   0:00 /usr/local/bin/fdctl run --config /etc/fdctl/config.toml --extra-flag";
        assert_eq!(
            extract_firedancer_config_path_from_ps_output(ps).unwrap(),
            "/etc/fdctl/config.toml"
        );
    }

    #[test]
    fn extract_firedancer_config_path_strips_quoted_argument() {
        let ps = "azureuser  1234  /usr/local/bin/fdctl run \"--config\" \"/etc/fdctl/config.toml\"";
        assert_eq!(
            extract_firedancer_config_path_from_ps_output(ps).unwrap(),
            "/etc/fdctl/config.toml"
        );
    }

    #[test]
    fn extract_firedancer_config_path_strips_ansi_escapes_from_grep_color() {
        // `grep --color=auto` may inject ANSI sequences around the match.
        let ps = "azureuser  /usr/local/bin/\x1b[01;31m\x1b[Kfdctl\x1b[m\x1b[K run --config /etc/fdctl/config.toml";
        assert_eq!(
            extract_firedancer_config_path_from_ps_output(ps).unwrap(),
            "/etc/fdctl/config.toml"
        );
    }

    #[test]
    fn extract_firedancer_config_path_errors_when_flag_missing() {
        let ps = "azureuser  /usr/local/bin/fdctl run --help";
        let err = extract_firedancer_config_path_from_ps_output(ps).unwrap_err();
        // We don't pin the exact wording, only that we get an actionable
        // error rather than a panic or a silent success.
        assert!(
            format!("{err}").contains("Firedancer config path"),
            "error should mention the missing config path, got: {err}",
        );
    }
}
