use anyhow::Result;
use serde_json::json;
use std::time::{Duration, Instant};

use crate::types::{AlertConfig, TelegramConfig, NodeHealthStatus};

#[derive(Clone)]
pub struct AlertManager {
    config: AlertConfig,
}

impl AlertManager {
    pub fn new(config: AlertConfig) -> Self {
        Self { config }
    }

    #[allow(dead_code)]
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

    #[allow(dead_code)]
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

        // Send main test message
        let message = format!(
            "✅ *SVS Alert Test* ✅\n\n\
            This is a test message from Solana Validator Switch.\n\
            Your Telegram alerts are configured correctly!\n\n\
            *Monitoring Validators:*\n{}\
            *Delinquency Threshold:* {} seconds\n\n\
            The following alert types are configured:\n\
            • Validator Delinquency Alerts\n\
            • Catchup Failure Alerts\n\
            • Switch Result Alerts",
            validators_text,
            self.config.delinquency_threshold_seconds
        );

        self.send_telegram_message(telegram, &message).await?;

        // Send example delinquency alert
        let delinquency_example = format!(
            "🚨 *EXAMPLE: VALIDATOR DELINQUENCY ALERT* 🚨\n\n\
            *Validator:* `{}`\n\
            *Node:* Example Node (Active)\n\
            *Last Vote Slot:* 123456789\n\
            *Time Since Last Vote:* {} seconds\n\
            *Threshold:* {} seconds\n\n\
            ⚠️ *This is just an example alert*",
            validators_info.first().map(|(id, _)| *id).unwrap_or("ExampleValidator"),
            self.config.delinquency_threshold_seconds,
            self.config.delinquency_threshold_seconds
        );

        self.send_telegram_message(telegram, &delinquency_example).await?;

        // Send example catchup failure alert
        let catchup_example = format!(
            "⚠️ *EXAMPLE: STANDBY NODE CATCHUP FAILURE* ⚠️\n\n\
            *Validator:* `{}`\n\
            *Standby Node:* Example Standby Node\n\
            *Consecutive Failures:* 3\n\n\
            The standby node has failed catchup check 3 times in a row.\n\
            This may indicate issues with the standby node's sync status.\n\n\
            ⚠️ *This is just an example alert*",
            validators_info.first().map(|(id, _)| *id).unwrap_or("ExampleValidator")
        );

        self.send_telegram_message(telegram, &catchup_example).await?;

        // Send example switch success alert
        let switch_success_example = 
            "✅ *EXAMPLE: VALIDATOR SWITCH SUCCESSFUL* in 850ms\n\n\
            *Previous Active:* Node A\n\
            *New Active:* Node B\n\n\
            Switch completed successfully!\n\n\
            ⚠️ *This is just an example alert*";

        self.send_telegram_message(telegram, &switch_success_example).await?;

        // Send example switch failure alert
        let switch_failure_example = 
            "❌ *EXAMPLE: VALIDATOR SWITCH FAILED*\n\n\
            *Active Node:* Node A\n\
            *Standby Node:* Node B\n\
            *Error:* Example error message\n\n\
            ⚠️ *Manual intervention may be required*\n\n\
            ⚠️ *This is just an example alert*";

        self.send_telegram_message(telegram, &switch_failure_example).await?;

        Ok("Test messages sent successfully (including examples of all alert types)".to_string())
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

    pub async fn send_switch_result(
        &self,
        success: bool,
        active_node: &str,
        standby_node: &str,
        total_time: Option<std::time::Duration>,
        error: Option<&str>,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        if let Some(telegram) = &self.config.telegram {
            let message = if success {
                let time_str = if let Some(time) = total_time {
                    format!(" in {}ms", time.as_millis())
                } else {
                    String::new()
                };

                format!(
                    "✅ *VALIDATOR SWITCH SUCCESSFUL*{}\n\n\
                    *Previous Active:* {}\n\
                    *New Active:* {}\n\n\
                    Switch completed successfully!",
                    time_str, active_node, standby_node
                )
            } else {
                let error_msg = error.unwrap_or("Unknown error");
                format!(
                    "❌ *VALIDATOR SWITCH FAILED*\n\n\
                    *Active Node:* {}\n\
                    *Standby Node:* {}\n\
                    *Error:* {}\n\n\
                    ⚠️ *Manual intervention may be required*",
                    active_node, standby_node, error_msg
                )
            };

            self.send_telegram_message(telegram, &message).await?;
        }

        Ok(())
    }

    pub async fn send_ssh_failure_alert(
        &self,
        validator_identity: &str,
        node_label: &str,
        consecutive_failures: u32,
        seconds_since_first_failure: u64,
        last_error: &str,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        if let Some(telegram) = &self.config.telegram {
            let message = format!(
                "🔌 *SSH CONNECTION FAILURE* 🔌\n\n\
                *Validator:* `{}`\n\
                *Node:* {}\n\
                *Consecutive Failures:* {}\n\
                *Time Since First Failure:* {} seconds\n\
                *Last Error:* {}\n\n\
                ⚠️ *Action Required:* Check server connectivity and SSH access",
                validator_identity,
                node_label,
                consecutive_failures,
                seconds_since_first_failure,
                last_error
            );

            self.send_telegram_message(telegram, &message).await?;
        }

        Ok(())
    }

    pub async fn send_rpc_failure_alert(
        &self,
        validator_identity: &str,
        vote_pubkey: &str,
        consecutive_failures: u32,
        seconds_since_first_failure: u64,
        last_error: &str,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        if let Some(telegram) = &self.config.telegram {
            let message = format!(
                "🌐 *RPC CONNECTION FAILURE* 🌐\n\n\
                *Validator:* `{}`\n\
                *Vote Account:* `{}`\n\
                *Consecutive Failures:* {}\n\
                *Time Since First Failure:* {} seconds\n\
                *Last Error:* {}\n\n\
                ⚠️ *Action Required:* Check RPC endpoint status and rate limits",
                validator_identity,
                vote_pubkey,
                consecutive_failures,
                seconds_since_first_failure,
                last_error
            );

            self.send_telegram_message(telegram, &message).await?;
        }

        Ok(())
    }

    pub async fn send_delinquency_alert_with_health(
        &self,
        validator_identity: &str,
        node_label: &str,
        is_active: bool,
        last_vote_slot: u64,
        seconds_since_vote: u64,
        node_health: &NodeHealthStatus,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        if let Some(telegram) = &self.config.telegram {
            let status = if is_active { "Active" } else { "Standby" };
            
            // Build SSH status string
            let ssh_status = if node_health.ssh_status.consecutive_failures > 0 {
                format!(
                    "❌ Failed ({} failures, {} seconds ago)",
                    node_health.ssh_status.consecutive_failures,
                    node_health.ssh_status.seconds_since_first_failure().unwrap_or(0)
                )
            } else {
                "✅ Connected".to_string()
            };

            // Build RPC status string
            let rpc_status = if node_health.rpc_status.consecutive_failures > 0 {
                format!(
                    "❌ Failed ({} failures, {} seconds ago)",
                    node_health.rpc_status.consecutive_failures,
                    node_health.rpc_status.seconds_since_first_failure().unwrap_or(0)
                )
            } else {
                "✅ Working".to_string()
            };

            let message = format!(
                "🚨 *VALIDATOR DELINQUENCY ALERT* 🚨\n\n\
                *Validator:* `{}`\n\
                *Node:* {} ({})\n\
                *Last Vote Slot:* {}\n\
                *Time Since Last Vote:* {} seconds\n\
                *Threshold:* {} seconds\n\n\
                *Health Status:*\n\
                • SSH: {}\n\
                • RPC: {}\n\n\
                ⚠️ *Action Required:* Check validator health",
                validator_identity,
                node_label,
                status,
                last_vote_slot,
                seconds_since_vote,
                self.config.delinquency_threshold_seconds,
                ssh_status,
                rpc_status
            );

            self.send_telegram_message(telegram, &message).await?;
        }

        Ok(())
    }

    pub async fn send_emergency_takeover_alert(
        &self,
        validator_identity: &str,
        active_node: &str,
        standby_node: &str,
        primary_switch_success: bool,
        tower_copy_success: bool,
        standby_switch_success: bool,
        total_time: Duration,
        error: Option<&str>,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        if let Some(telegram) = &self.config.telegram {
            let primary_status = if primary_switch_success { "✅" } else { "❌" };
            let tower_status = if tower_copy_success { "✅" } else { "❌" };
            
            let message = if let Some(error_msg) = error {
                format!(
                    "❌ *EMERGENCY TAKEOVER FAILED*\n\n\
                    *Validator:* `{}`\n\
                    *Reason:* Not voting with confirmed connectivity\n\n\
                    *Previous Active:* {} ❌\n\
                    *Attempted New Active:* {} ❌\n\n\
                    *Optional Steps:*\n\
                    • Primary → Unfunded: {}\n\
                    • Tower Copy: {}\n\
                    • Standby → Funded: ❌\n\n\
                    *Error:* {}\n\
                    *Duration:* {}ms\n\n\
                    ⚠️ *MANUAL INTERVENTION REQUIRED*",
                    validator_identity,
                    active_node,
                    standby_node,
                    primary_status,
                    tower_status,
                    error_msg,
                    total_time.as_millis()
                )
            } else {
                format!(
                    "{} *EMERGENCY TAKEOVER {}*\n\n\
                    *Validator:* `{}`\n\
                    *Reason:* Not voting for 30+ seconds with confirmed connectivity\n\n\
                    *Previous Active:* {} ❌\n\
                    *New Active:* {} ✅\n\n\
                    *Optional Steps:*\n\
                    • Primary → Unfunded: {} {}\n\
                    • Tower Copy: {} {}\n\n\
                    *Required Step:*\n\
                    • Standby → Funded: ✅ Success\n\n\
                    *Takeover completed in:* {}ms\n\n\
                    ⚠️ *VERIFY VALIDATOR STATUS IMMEDIATELY*",
                    if standby_switch_success { "🚨" } else { "❌" },
                    if standby_switch_success { "INITIATED" } else { "FAILED" },
                    validator_identity,
                    active_node,
                    standby_node,
                    primary_status,
                    if primary_switch_success { "Success" } else { "Failed (continued)" },
                    tower_status,
                    if tower_copy_success { "Success" } else { "Failed (continued)" },
                    total_time.as_millis()
                )
            };

            self.send_telegram_message(telegram, &message).await?;
        }

        Ok(())
    }

    pub async fn send_catchup_failure_alert(
        &self,
        validator_identity: &str,
        node_label: &str,
        consecutive_failures: u32,
    ) -> Result<()> {
        if !self.config.enabled {
            return Ok(());
        }

        if let Some(telegram) = &self.config.telegram {
            let message = format!(
                "⚠️ *STANDBY NODE CATCHUP FAILURE* ⚠️\n\n\
                *Validator:* `{}`\n\
                *Standby Node:* {}\n\
                *Consecutive Failures:* {}\n\n\
                The standby node has failed catchup check {} times in a row.\n\
                This may indicate issues with the standby node's sync status.",
                validator_identity,
                node_label,
                consecutive_failures,
                consecutive_failures
            );

            self.send_telegram_message(telegram, &message).await?;
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
        Self::with_cooldown(validator_count, 1800) // Default 30 minutes
    }
    
    pub fn with_cooldown(validator_count: usize, cooldown_seconds: u64) -> Self {
        Self {
            last_alert_times: vec![None; validator_count],
            cooldown_seconds,
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

// Comprehensive alert tracker for different alert types
pub struct ComprehensiveAlertTracker {
    pub delinquency_tracker: AlertTracker,
    pub ssh_failure_tracker: Vec<AlertTracker>, // Per node tracker
    pub rpc_failure_tracker: AlertTracker,
}

impl ComprehensiveAlertTracker {
    pub fn new(validator_count: usize, nodes_per_validator: usize) -> Self {
        let mut ssh_trackers = Vec::new();
        for _ in 0..nodes_per_validator {
            // Low severity: 30-minute cooldown for SSH failures
            ssh_trackers.push(AlertTracker::with_cooldown(validator_count, 1800));
        }
        
        Self {
            // High severity: 15-minute cooldown for delinquency
            delinquency_tracker: AlertTracker::with_cooldown(validator_count, 900),
            ssh_failure_tracker: ssh_trackers,
            // Low severity: 30-minute cooldown for RPC failures
            rpc_failure_tracker: AlertTracker::with_cooldown(validator_count, 1800),
        }
    }
}
