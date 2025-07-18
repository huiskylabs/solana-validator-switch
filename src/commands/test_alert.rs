use anyhow::Result;
use colored::*;

use crate::alert::AlertManager;
use crate::AppState;

pub async fn test_alert_command(app_state: &AppState) -> Result<()> {
    println!(
        "{}",
        "\n🔔 Testing Alert Configuration...\n".bright_blue().bold()
    );

    // Check if alert config exists
    let alert_config = match &app_state.config.alert_config {
        Some(config) => config,
        None => {
            println!("{}", "❌ No alert configuration found in config file".red());
            println!("\nTo configure alerts, add the following to your config.yaml:");
            println!(
                "{}",
                r#"
alert_config:
  enabled: true
  delinquency_threshold_seconds: 30
  telegram:
    bot_token: "YOUR_BOT_TOKEN"
    chat_id: "YOUR_CHAT_ID"
"#
                .yellow()
            );
            return Ok(());
        }
    };

    if !alert_config.enabled {
        println!("{}", "⚠️  Alerts are disabled in configuration".yellow());
        println!("Set 'enabled: true' in alert_config to enable alerts");
        return Ok(());
    }

    println!("📊 Alert Configuration:");
    println!("  • Enabled: {}", "✓".green());
    println!(
        "  • Delinquency Threshold: {} seconds",
        alert_config.delinquency_threshold_seconds
    );

    // Collect validator information
    let validators_info: Vec<(&str, &str)> = app_state
        .config
        .validators
        .iter()
        .map(|v| (v.identity_pubkey.as_str(), v.vote_pubkey.as_str()))
        .collect();

    // Test alerts
    let alert_manager = AlertManager::new(alert_config.clone());

    match alert_manager.send_test_alert(validators_info).await {
        Ok(result) => {
            println!("\n{}", "📨 Alert Test Results:".bright_blue());
            for line in result.lines() {
                println!("  {}", line);
            }
        }
        Err(e) => {
            println!("\n{}", format!("❌ Alert test failed: {}", e).red());
        }
    }

    println!("\n{}", "✅ Alert test complete!".green().bold());
    Ok(())
}
