use anyhow::Result;
use chrono::Local;
use serde_json::json;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::RwLock;
use tokio::time::{interval, Duration};

use crate::types::{AlertConfig, TelegramConfig};
use crate::AppState;

pub struct AlertManager {
    config: AlertConfig,
}

impl AlertManager {
    pub fn new(config: AlertConfig) -> Self {
        Self { config }
    }

    pub async fn send_delinquency_alert(
        &self,
        validator_identity: &str,
        node_label: &str,
        is_active: bool,
        last_vote_slot: u64,
        seconds_since_vote: u64,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        // For now, only send to Telegram if configured
        if let Some(telegram) = &self.config.telegram {
            self.send_telegram_delinquency_alert(
                telegram,
                validator_identity,
                node_label,
                is_active,
                last_vote_slot,
                seconds_since_vote,
            )
            .await?;
        }

        Ok(())
    }

    pub async fn send_test_alert(&self, validators_info: Vec<(&str, &str)>) -> Result<String> {
        if !self.config.enabled {
            return Ok("Alerts are disabled".to_string());
        }

        let mut results = Vec::new();

        // Test Telegram if configured
        if let Some(telegram) = &self.config.telegram {
            match self
                .send_telegram_test_alert(telegram, &validators_info)
                .await
            {
                Ok(msg) => results.push(format!("✅ Telegram: {}", msg)),
                Err(e) => results.push(format!("❌ Telegram: {}", e)),
            }
        } else {
            results.push("⚠️  Telegram: Not configured".to_string());
        }

        if results.is_empty() {
            results.push("No alert services configured".to_string());
        }

        Ok(results.join("\n"))
    }

    async fn send_telegram_delinquency_alert(
        &self,
        telegram: &TelegramConfig,
        validator_identity: &str,
        node_label: &str,
        is_active: bool,
        last_vote_slot: u64,
        seconds_since_vote: u64,
    ) -> Result<()> {
        let status = if is_active { "Active" } else { "Standby" };

        let message = format!(
            "🚨 *VALIDATOR DELINQUENCY ALERT* 🚨\n\n\
            *Validator:* `{}`\n\
            *Node:* {} ({})\n\
            *Last Vote Slot:* {}\n\
            *Time Since Last Vote:* {} seconds\n\
            *Threshold:* {} seconds\n\n\
            ⚠️ *Action Required:* Check validator health",
            validator_identity,
            node_label,
            status,
            last_vote_slot,
            seconds_since_vote,
            self.config.delinquency_threshold_seconds
        );

        self.send_telegram_message(telegram, &message).await
    }

    async fn send_telegram_test_alert(
        &self,
        telegram: &TelegramConfig,
        validators_info: &[(&str, &str)],
    ) -> Result<String> {
        let mut validators_text = String::new();
        for (identity, vote) in validators_info {
            validators_text.push_str(&format!(
                "*Identity:* `{}`\n*Vote:* `{}`\n\n",
                identity, vote
            ));
        }

        let message = format!(
            "✅ *SVS Alert Test* ✅\n\n\
            This is a test message from Solana Validator Switch.\n\
            Your Telegram alerts are configured correctly!\n\n\
            *Monitoring Validators:*\n{}\
            *Delinquency Threshold:* {} seconds\n\n\
            Alerts will be sent when any validator stops voting for more than {} seconds.",
            validators_text,
            self.config.delinquency_threshold_seconds,
            self.config.delinquency_threshold_seconds
        );

        self.send_telegram_message(telegram, &message).await?;
        Ok("Test message sent successfully".to_string())
    }

    async fn send_telegram_message(&self, telegram: &TelegramConfig, message: &str) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            telegram.bot_token
        );

        let payload = json!({
            "chat_id": telegram.chat_id,
            "text": message,
            "parse_mode": "Markdown",
            "disable_web_page_preview": true
        });

        let client = reqwest::Client::new();
        let response = client.post(&url).json(&payload).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Telegram API error: {}", error_text);
        }

        Ok(())
    }
}

// Helper to track alert cooldowns per validator
pub struct AlertTracker {
    last_alert_times: Vec<Option<Instant>>,
    cooldown_seconds: u64,
}

impl AlertTracker {
    pub fn new(validator_count: usize) -> Self {
        Self {
            last_alert_times: vec![None; validator_count],
            cooldown_seconds: 300, // 5 minutes
        }
    }

    pub fn should_send_alert(&mut self, validator_idx: usize) -> bool {
        if validator_idx >= self.last_alert_times.len() {
            return false;
        }

        match self.last_alert_times[validator_idx] {
            Some(last_time) => {
                if last_time.elapsed().as_secs() >= self.cooldown_seconds {
                    self.last_alert_times[validator_idx] = Some(Instant::now());
                    true
                } else {
                    false
                }
            }
            None => {
                self.last_alert_times[validator_idx] = Some(Instant::now());
                true
            }
        }
    }

    pub fn reset(&mut self, validator_idx: usize) {
        if validator_idx < self.last_alert_times.len() {
            self.last_alert_times[validator_idx] = None;
        }
    }
}

// Telegram Bot functionality
#[derive(Debug, Clone)]
pub struct TelegramBot {
    config: TelegramConfig,
    last_update_id: Arc<RwLock<Option<i64>>>,
    log_sender:
        Option<tokio::sync::mpsc::UnboundedSender<crate::commands::status_ui_v2::LogMessage>>,
    view_change_sender:
        Option<tokio::sync::mpsc::UnboundedSender<crate::commands::status_ui_v2::ViewState>>,
}

impl TelegramBot {
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            last_update_id: Arc::new(RwLock::new(None)),
            log_sender: None,
            view_change_sender: None,
        }
    }

    pub fn with_log_sender(
        mut self,
        log_sender: tokio::sync::mpsc::UnboundedSender<crate::commands::status_ui_v2::LogMessage>,
    ) -> Self {
        self.log_sender = Some(log_sender);
        self
    }

    pub fn with_view_change_sender(
        mut self,
        sender: tokio::sync::mpsc::UnboundedSender<crate::commands::status_ui_v2::ViewState>,
    ) -> Self {
        self.view_change_sender = Some(sender);
        self
    }

    pub async fn start_polling(self, app_state: Arc<AppState>) {
        let mut interval = interval(Duration::from_secs(2)); // Poll every 2 seconds

        loop {
            interval.tick().await;

            if let Err(e) = self.poll_updates(&app_state).await {
                eprintln!("Telegram bot polling error: {}", e);
            }
        }
    }

    async fn poll_updates(&self, app_state: &Arc<AppState>) -> Result<()> {
        let mut last_update_id = self.last_update_id.write().await;

        let url = format!(
            "https://api.telegram.org/bot{}/getUpdates",
            self.config.bot_token
        );

        let mut params = json!({
            "timeout": 30,
            "allowed_updates": ["message"]
        });

        if let Some(offset) = *last_update_id {
            params["offset"] = json!(offset + 1);
        }

        let client = reqwest::Client::new();
        let response = client.post(&url).json(&params).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Telegram API error: {}", error_text);
        }

        let updates: serde_json::Value = response.json().await?;

        if let Some(result) = updates["result"].as_array() {
            for update in result {
                if let Some(update_id) = update["update_id"].as_i64() {
                    *last_update_id = Some(update_id);

                    if let Some(message) = update["message"].as_object() {
                        // Check if message is from authorized chat
                        if let Some(chat_id) = message["chat"]["id"].as_i64() {
                            if chat_id.to_string() != self.config.chat_id {
                                continue; // Ignore messages from other chats
                            }
                        }

                        if let Some(text) = message["text"].as_str() {
                            self.handle_command(text, app_state).await?;
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn handle_command(&self, text: &str, app_state: &Arc<AppState>) -> Result<()> {
        let command = text.trim().to_lowercase();

        // Log the received command
        if let Some(log_sender) = &self.log_sender {
            let _ = log_sender.send(crate::commands::status_ui_v2::LogMessage {
                host: "telegram-bot".to_string(),
                message: format!("📱 Received command: {}", command),
                timestamp: Instant::now(),
                level: crate::commands::status_ui_v2::LogLevel::Info,
            });
        }

        match command.as_str() {
            "/status" | "status" => {
                // Change view to Status
                if let Some(view_sender) = &self.view_change_sender {
                    let _ = view_sender.send(crate::commands::status_ui_v2::ViewState::Status);
                }

                if let Some(log_sender) = &self.log_sender {
                    let _ = log_sender.send(crate::commands::status_ui_v2::LogMessage {
                        host: "telegram-bot".to_string(),
                        message: "📋 Generating status snapshot for Telegram...".to_string(),
                        timestamp: Instant::now(),
                        level: crate::commands::status_ui_v2::LogLevel::Info,
                    });
                }

                let status_text = self.format_status_snapshot(app_state).await?;
                self.send_message(&status_text).await?;

                if let Some(log_sender) = &self.log_sender {
                    let _ = log_sender.send(crate::commands::status_ui_v2::LogMessage {
                        host: "telegram-bot".to_string(),
                        message: "✅ Status snapshot sent to Telegram".to_string(),
                        timestamp: Instant::now(),
                        level: crate::commands::status_ui_v2::LogLevel::Info,
                    });
                }
            }
            "/switch" | "switch" | "s" => {
                // Change view to DryRunSwitch
                if let Some(view_sender) = &self.view_change_sender {
                    let _ =
                        view_sender.send(crate::commands::status_ui_v2::ViewState::DryRunSwitch);
                }

                if let Some(log_sender) = &self.log_sender {
                    let _ = log_sender.send(crate::commands::status_ui_v2::LogMessage {
                        host: "telegram-bot".to_string(),
                        message: "🔄 Performing dry-run switch analysis...".to_string(),
                        timestamp: Instant::now(),
                        level: crate::commands::status_ui_v2::LogLevel::Warning,
                    });
                }

                let switch_result = self.perform_dry_run_switch(app_state).await?;
                self.send_message(&switch_result).await?;

                if let Some(log_sender) = &self.log_sender {
                    let _ = log_sender.send(crate::commands::status_ui_v2::LogMessage {
                        host: "telegram-bot".to_string(),
                        message:
                            "✅ Dry-run switch results sent to Telegram (no actual changes made)"
                                .to_string(),
                        timestamp: Instant::now(),
                        level: crate::commands::status_ui_v2::LogLevel::Info,
                    });
                }
            }
            _ => {
                let help_text = "🤖 *Available Commands:*\n\n\
                    `/status` - View current validator status\n\
                    `/switch` or `s` - Perform a dry-run switch\n\n\
                    Just type `status` or `s` for quick access!";
                self.send_message(help_text).await?;

                if let Some(log_sender) = &self.log_sender {
                    let _ = log_sender.send(crate::commands::status_ui_v2::LogMessage {
                        host: "telegram-bot".to_string(),
                        message: format!("ℹ️ Unknown command '{}' - sent help message", command),
                        timestamp: Instant::now(),
                        level: crate::commands::status_ui_v2::LogLevel::Info,
                    });
                }
            }
        }

        Ok(())
    }

    async fn send_message(&self, text: &str) -> Result<()> {
        let url = format!(
            "https://api.telegram.org/bot{}/sendMessage",
            self.config.bot_token
        );

        let payload = json!({
            "chat_id": self.config.chat_id,
            "text": text,
            "parse_mode": "Markdown",
            "disable_web_page_preview": true
        });

        let client = reqwest::Client::new();
        let response = client.post(&url).json(&payload).send().await?;

        if !response.status().is_success() {
            let error_text = response.text().await.unwrap_or_default();
            anyhow::bail!("Telegram API error: {}", error_text);
        }

        Ok(())
    }

    async fn format_status_snapshot(&self, app_state: &Arc<AppState>) -> Result<String> {
        let mut output = String::from("📋 *Validator Status*\n\n");

        for (idx, validator_status) in app_state.validator_statuses.iter().enumerate() {
            let validator_pair = &validator_status.validator_pair;

            // Add validator header
            if let Some(ref metadata) = validator_status.metadata {
                if let Some(ref name) = metadata.name {
                    output.push_str(&format!("*{}*\n", name));
                } else {
                    output.push_str(&format!("*Validator {}*\n", idx + 1));
                }
            } else {
                output.push_str(&format!("*Validator {}*\n", idx + 1));
            }

            output.push_str(&format!("Vote: `{}`\n", validator_pair.vote_pubkey));
            output.push_str(&format!(
                "Identity: `{}`\n\n",
                validator_pair.identity_pubkey
            ));

            // Format nodes in a table-like structure
            if validator_status.nodes_with_status.len() >= 2 {
                let node_0 = &validator_status.nodes_with_status[0];
                let node_1 = &validator_status.nodes_with_status[1];

                let status_0 = match node_0.status {
                    crate::types::NodeStatus::Active => "🟢 ACTIVE",
                    crate::types::NodeStatus::Standby => "🟡 STANDBY",
                    crate::types::NodeStatus::Unknown => "⚫ UNKNOWN",
                };

                let status_1 = match node_1.status {
                    crate::types::NodeStatus::Active => "🟢 ACTIVE",
                    crate::types::NodeStatus::Standby => "🟡 STANDBY",
                    crate::types::NodeStatus::Unknown => "⚫ UNKNOWN",
                };

                output.push_str("```\n");
                output.push_str(&format!(
                    "{:<20} {:<20}\n",
                    format!("{} ({})", node_0.node.label, status_0),
                    format!("{} ({})", node_1.node.label, status_1)
                ));
                output.push_str(&format!("{:<20} {:<20}\n", "─".repeat(19), "─".repeat(19)));
                output.push_str(&format!(
                    "Host: {:<14} Host: {:<14}\n",
                    node_0.node.host, node_1.node.host
                ));

                // Add validator type
                let type_str_0 = match &node_0.validator_type {
                    crate::types::ValidatorType::Agave => "Agave",
                    crate::types::ValidatorType::Jito => "Jito",
                    crate::types::ValidatorType::Firedancer => "Firedancer",
                    crate::types::ValidatorType::Unknown => "Unknown",
                };
                let type_str_1 = match &node_1.validator_type {
                    crate::types::ValidatorType::Agave => "Agave",
                    crate::types::ValidatorType::Jito => "Jito",
                    crate::types::ValidatorType::Firedancer => "Firedancer",
                    crate::types::ValidatorType::Unknown => "Unknown",
                };
                output.push_str(&format!(
                    "Type: {:<14} Type: {:<14}\n",
                    type_str_0, type_str_1
                ));

                output.push_str("```\n\n");
            }
        }

        output.push_str(&format!(
            "_Last updated: {}_",
            Local::now().format("%H:%M:%S")
        ));

        Ok(output)
    }

    async fn perform_dry_run_switch(&self, app_state: &Arc<AppState>) -> Result<String> {
        let mut output = String::from("🔄 *Dry Run Switch*\n\n");

        if app_state.config.validators.is_empty() {
            output.push_str("❌ No validators configured");
            return Ok(output);
        }

        // Use the first validator
        let validator_status = &app_state.validator_statuses[0];

        // Find active and standby nodes
        let active_node = validator_status
            .nodes_with_status
            .iter()
            .find(|n| n.status == crate::types::NodeStatus::Active);
        let standby_node = validator_status
            .nodes_with_status
            .iter()
            .find(|n| n.status == crate::types::NodeStatus::Standby);

        if let (Some(active), Some(standby)) = (active_node, standby_node) {
            output.push_str("*Current State:*\n");
            output.push_str(&format!("• {} → ACTIVE\n", active.node.label));
            output.push_str(&format!("• {} → STANDBY\n\n", standby.node.label));

            output.push_str("*After Switch:*\n");
            output.push_str(&format!(
                "• {} → STANDBY _(was active)_\n",
                active.node.label
            ));
            output.push_str(&format!(
                "• {} → ACTIVE _(was standby)_\n\n",
                standby.node.label
            ));

            output.push_str("*Actions to be performed:*\n");
            output.push_str("1️⃣ Switch active node to unfunded identity\n");
            output.push_str("2️⃣ Transfer tower file to standby node\n");
            output.push_str("3️⃣ Switch standby node to funded identity\n\n");

            output.push_str("✅ *Dry run complete* - No actual changes made\n\n");
            output.push_str("_To perform actual switch, use the CLI command_");
        } else {
            output.push_str("❌ Unable to determine active/standby nodes");
        }

        Ok(output)
    }
}
