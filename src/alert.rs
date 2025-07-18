use anyhow::Result;
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
}

impl TelegramBot {
    pub fn new(config: TelegramConfig) -> Self {
        Self {
            config,
            last_update_id: Arc::new(RwLock::new(None)),
            log_sender: None,
        }
    }

    pub fn with_log_sender(
        mut self,
        log_sender: tokio::sync::mpsc::UnboundedSender<crate::commands::status_ui_v2::LogMessage>,
    ) -> Self {
        self.log_sender = Some(log_sender);
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
            "/switch" | "switch" | "/s" | "s" => {
                // Execute the switch directly
                if let Some(log_sender) = &self.log_sender {
                    let _ = log_sender.send(crate::commands::status_ui_v2::LogMessage {
                        host: "telegram-bot".to_string(),
                        message: "⚠️ Executing validator switch...".to_string(),
                        timestamp: Instant::now(),
                        level: crate::commands::status_ui_v2::LogLevel::Warning,
                    });
                }

                let switch_result = self.perform_real_switch(app_state).await?;
                self.send_message(&switch_result).await?;
            }
            _ => {
                let help_text = "Use `/s` or `s` to switch validators";
                self.send_message(help_text).await?;
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

    async fn perform_real_switch(&self, app_state: &Arc<AppState>) -> Result<String> {
        if app_state.config.validators.is_empty() {
            return Ok("❌ No validators configured".to_string());
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

        if let (Some(_active), Some(standby)) = (active_node, standby_node) {
            let new_active = standby.node.label.clone();
            let new_active_host = standby.node.host.clone();

            // Perform the actual switch using the switch command (skip confirmation for Telegram)
            // Set environment variable to suppress output
            std::env::set_var("SVS_SILENT_MODE", "1");
            let result =
                crate::commands::switch::switch_command_with_confirmation(false, app_state, false)
                    .await;
            std::env::remove_var("SVS_SILENT_MODE");

            match result {
                Ok(_) => Ok(format!(
                    "✅ Switch successful\nNew active: {} ({})",
                    new_active, new_active_host
                )),
                Err(e) => Ok(format!("❌ Switch failed: {}", e)),
            }
        } else {
            Ok("❌ Unable to determine active/standby nodes".to_string())
        }
    }
}
