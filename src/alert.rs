use anyhow::Result;
use serde_json::json;
use std::time::Instant;

use crate::types::{AlertConfig, TelegramConfig};

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
