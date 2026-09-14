use anyhow::{anyhow, Result};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::time::timeout;

use crate::alert::AlertManager;
use crate::commands::switch::{FailoverMode, SwitchManager};
use crate::ssh::AsyncSshPool;
use crate::types::{NodeWithStatus, ValidatorPair};

/// Result of checking whether the standby actually took the funded identity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum PromotionOutcome {
    /// The target reports the funded identity. The takeover really happened.
    Confirmed,
    /// The target answered but is running some other identity.
    WrongIdentity { observed: String },
    /// The target could not be queried, so the takeover is unproven.
    Unverifiable { reason: String },
}

/// Decide whether a promotion actually landed.
///
/// `set-identity` returning success only means the command was accepted over
/// SSH. A validator whose RPC is dead can accept it and still not vote, which
/// is how an emergency takeover previously reported "completed" twice against a
/// node that never came back while the vote account sat frozen.
pub(crate) fn classify_promotion(
    observed_identity: Result<String, String>,
    funded_identity: &str,
) -> PromotionOutcome {
    match observed_identity {
        Ok(identity) if identity == funded_identity => PromotionOutcome::Confirmed,
        Ok(identity) => PromotionOutcome::WrongIdentity { observed: identity },
        Err(reason) => PromotionOutcome::Unverifiable { reason },
    }
}

pub struct EmergencyFailover {
    active_node: NodeWithStatus,
    standby_node: NodeWithStatus,
    validator_pair: ValidatorPair,
    ssh_pool: Arc<AsyncSshPool>,
    detected_ssh_keys: std::collections::HashMap<String, String>,
    alert_manager: AlertManager,
    // Track results
    primary_switch_success: bool,
    tower_copy_success: bool,
    standby_switch_success: bool,
    total_time: Option<Duration>,
    mode: FailoverMode,
}

impl EmergencyFailover {
    pub fn new(
        active_node: NodeWithStatus,
        standby_node: NodeWithStatus,
        validator_pair: ValidatorPair,
        ssh_pool: Arc<AsyncSshPool>,
        detected_ssh_keys: std::collections::HashMap<String, String>,
        alert_manager: AlertManager,
        mode: FailoverMode,
    ) -> Self {
        Self {
            active_node,
            standby_node,
            validator_pair,
            ssh_pool,
            detected_ssh_keys,
            alert_manager,
            primary_switch_success: false,
            tower_copy_success: false,
            standby_switch_success: false,
            total_time: None,
            mode,
        }
    }

    pub async fn execute_emergency_takeover(&mut self) -> Result<()> {
        let start_time = Instant::now();

        // Log the emergency takeover
        eprintln!("🚨 EMERGENCY TAKEOVER INITIATED");
        eprintln!(
            "   Active node ({}) not voting, attempting failover to standby ({})",
            self.active_node.node.label, self.standby_node.node.label
        );

        // Create switch manager for the operations
        let mut switch_manager = SwitchManager::new(
            self.active_node.clone(),
            self.standby_node.clone(),
            self.validator_pair.clone(),
            self.ssh_pool.clone(),
            self.detected_ssh_keys.clone(),
        );

        std::env::set_var("SVS_SILENT_MODE", "1");
        let plan = self.mode.step_plan();

        if plan.demote_source {
            eprintln!("📤 Switching primary to unfunded...");
        let primary_result = match timeout(
                Duration::from_secs(10),
            switch_manager.switch_primary_to_unfunded(false),
        )
        .await
        {
            Ok(Ok(_)) => {
                eprintln!("   ✅ Primary switched to unfunded successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                eprintln!("   ⚠️  Failed to switch primary: {}", e);
                Err(e)
            }
            Err(_) => {
                eprintln!("   ⚠️  Switch primary timed out");
                Err(anyhow!("Operation timed out"))
            }
        };
        self.primary_switch_success = primary_result.is_ok();
        } else {
            eprintln!(
                "   ⚠️  Primary is unreachable and confirmed delinquent; skipping primary demotion"
            );
        }

        if plan.transfer_tower {
        eprintln!("📤 Copying tower file...");
        let tower_result = match timeout(
                Duration::from_secs(10),
            switch_manager.transfer_tower_file(false),
        )
        .await
        {
            Ok(Ok(_)) => {
                eprintln!("   ✅ Tower file copied successfully");
                Ok(())
            }
            Ok(Err(e)) => {
                eprintln!("   ⚠️  Failed to copy tower: {}", e);
                Err(e)
            }
            Err(_) => {
                eprintln!("   ⚠️  Tower copy timed out");
                Err(anyhow!("Operation timed out"))
            }
        };
        self.tower_copy_success = tower_result.is_ok();
        } else {
            eprintln!("   ⚠️  Tower copy skipped because the primary is unreachable");
        }

        // Standby promotion is required in every mode.
        debug_assert!(plan.promote_standby);
        eprintln!("🚀 Switching standby to funded identity...");
        match switch_manager.switch_backup_to_funded(false).await {
            Ok(_) => {
                self.standby_switch_success = true;
                eprintln!("   ✅ Standby switched to funded identity successfully");
            }
            Err(e) => {
                eprintln!("   ❌ CRITICAL: Failed to switch standby to funded: {}", e);
                self.total_time = Some(start_time.elapsed());

                // Send failure notification
                let _ = self
                    .alert_manager
                    .send_emergency_takeover_alert(
                        &self.validator_pair.identity_pubkey,
                        &self.active_node.node.label,
                        &self.standby_node.node.label,
                        self.primary_switch_success,
                        self.tower_copy_success,
                        false, // standby switch failed
                        self.total_time.unwrap(),
                        Some(&format!("Failed to activate standby: {}", e)),
                    )
                    .await;

                return Err(anyhow!(
                    "Emergency takeover failed: could not activate standby node"
                ));
            }
        }

        self.total_time = Some(start_time.elapsed());

        // Confirm the promotion actually landed. `set-identity` succeeding only
        // proves the command was accepted; the target must also report the
        // funded identity, or the takeover has not really happened.
        let promotion = self.verify_promotion().await;
        let verification_note = match &promotion {
            PromotionOutcome::Confirmed => {
                eprintln!(
                    "   ✅ Verified: {} now reports the funded identity",
                    self.standby_node.node.label
                );
                None
            }
            PromotionOutcome::WrongIdentity { observed } => {
                eprintln!(
                    "   ❌ CRITICAL: {} reports identity {}, not the funded identity {}",
                    self.standby_node.node.label, observed, self.validator_pair.identity_pubkey
                );
                Some(format!(
                    "Promotion NOT confirmed: {} reports {} instead of the funded identity",
                    self.standby_node.node.label, observed
                ))
            }
            PromotionOutcome::Unverifiable { reason } => {
                eprintln!(
                    "   ⚠️  Could not confirm promotion on {}: {}",
                    self.standby_node.node.label, reason
                );
                Some(format!(
                    "Promotion UNCONFIRMED on {}: {}",
                    self.standby_node.node.label, reason
                ))
            }
        };

        // Send notification. A failover we could not verify must not be
        // reported as a clean success.
        let _ = self
            .alert_manager
            .send_emergency_takeover_alert(
                &self.validator_pair.identity_pubkey,
                &self.active_node.node.label,
                &self.standby_node.node.label,
                self.primary_switch_success,
                self.tower_copy_success,
                self.standby_switch_success,
                self.total_time.unwrap(),
                verification_note.as_deref(),
            )
            .await;

        if let Some(note) = verification_note {
            return Err(anyhow!("{}", note));
        }

        eprintln!(
            "\n✅ Emergency takeover completed in {:?}",
            self.total_time.unwrap()
        );
        eprintln!(
            "   Primary → Unfunded: {}",
            if self.primary_switch_success {
                "✅"
            } else {
                "❌"
            }
        );
        eprintln!(
            "   Tower Copy: {}",
            if self.tower_copy_success {
                "✅"
            } else {
                "❌"
            }
        );
        eprintln!("   Standby → Funded: ✅");

        Ok(())
    }

    /// Ask the promoted node which identity it is actually running.
    async fn verify_promotion(&self) -> PromotionOutcome {
        let Some(ssh_key) = self.detected_ssh_keys.get(&self.standby_node.node.host) else {
            return PromotionOutcome::Unverifiable {
                reason: format!("no SSH key for {}", self.standby_node.node.host),
            };
        };

        let rpc_port =
            crate::validator_rpc::get_rpc_port(self.standby_node.validator_type.clone(), None);

        let observed = match timeout(
            Duration::from_secs(15),
            crate::validator_rpc::get_identity(
                &self.ssh_pool,
                &self.standby_node.node,
                ssh_key,
                rpc_port,
            ),
        )
        .await
        {
            Ok(Ok(identity)) => Ok(identity),
            Ok(Err(error)) => Err(error.to_string()),
            Err(_) => Err("identity check timed out".to_string()),
        };

        classify_promotion(observed, &self.validator_pair.identity_pubkey)
    }
}

#[cfg(test)]
mod promotion_verification_tests {
    //! An emergency takeover must not report success it cannot prove.
    //!
    //! `set-identity` returning Ok only means SSH accepted the command. A node
    //! with dead RPC accepts it and never votes — observed twice in production,
    //! both logged as "AUTO-FAILOVER completed" while the vote account stayed
    //! frozen at the same slot for 20 minutes.

    use super::{classify_promotion, PromotionOutcome};

    const FUNDED: &str = "YiUD1FC24FPTgTbtMnqFPDME6cqSQoYFhBxFiu1AiPN";

    #[test]
    fn target_reporting_the_funded_identity_is_confirmed() {
        assert_eq!(
            classify_promotion(Ok(FUNDED.to_string()), FUNDED),
            PromotionOutcome::Confirmed
        );
    }

    #[test]
    fn target_still_on_the_unfunded_identity_is_not_a_success() {
        let unfunded = "CzakHEC9yh1gPrbStoPEG6Mpi3JJp2PGr66hkzVaiLaa";

        assert_eq!(
            classify_promotion(Ok(unfunded.to_string()), FUNDED),
            PromotionOutcome::WrongIdentity {
                observed: unfunded.to_string()
            }
        );
    }

    #[test]
    fn unreachable_target_is_unverifiable_not_confirmed() {
        // The production failure: RPC dead, set-identity "succeeded".
        let outcome = classify_promotion(
            Err("Failed to parse RPC response: EOF while parsing a value".to_string()),
            FUNDED,
        );

        assert!(
            matches!(outcome, PromotionOutcome::Unverifiable { .. }),
            "an unreachable target must never be reported as confirmed"
        );
        assert_ne!(outcome, PromotionOutcome::Confirmed);
    }
}
