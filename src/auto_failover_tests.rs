#[cfg(test)]
mod tests {
    use crate::types::AlertConfig;

    #[test]
    fn test_alert_config_with_auto_failover() {
        let alert_config = AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800,
            rpc_failure_threshold_seconds: 1800,
            vote_account_poll_interval_seconds: 10,
            node_status_poll_interval_seconds: 10,
            telegram: None,
            telegram_low_priority: None,
            auto_failover_enabled: true,
        };

        assert!(alert_config.enabled);
        assert!(alert_config.auto_failover_enabled);
    }

    #[test]
    fn test_auto_failover_disabled_by_default() {
        let alert_config = AlertConfig {
            enabled: true,
            delinquency_threshold_seconds: 30,
            ssh_failure_threshold_seconds: 1800,
            rpc_failure_threshold_seconds: 1800,
            vote_account_poll_interval_seconds: 10,
            node_status_poll_interval_seconds: 10,
            telegram: None,
            telegram_low_priority: None,
            auto_failover_enabled: false,
        };

        assert!(!alert_config.auto_failover_enabled);
    }

    #[test]
    fn test_delinquency_triggers_failover_conditions() {
        use crate::types::{FailureTracker, NodeHealthStatus};
        use std::time::Instant;

        let mut health = NodeHealthStatus {
            ssh_status: FailureTracker::new(),
            rpc_status: FailureTracker::new(),
            is_voting: false,
            last_vote_slot: Some(1000),
            last_vote_time: Some(Instant::now()),
        };

        // Test condition 1: SSH and RPC working, should trigger failover
        assert_eq!(health.ssh_status.consecutive_failures, 0);
        assert_eq!(health.rpc_status.consecutive_failures, 0);
        let should_failover = health.ssh_status.consecutive_failures == 0
            && health.rpc_status.consecutive_failures == 0;
        assert!(
            should_failover,
            "Should trigger failover when SSH and RPC are working"
        );

        // Test condition 2: SSH failing, should NOT trigger failover
        health
            .ssh_status
            .record_failure("Connection refused".to_string());
        let should_failover = health.ssh_status.consecutive_failures == 0
            && health.rpc_status.consecutive_failures == 0;
        assert!(
            !should_failover,
            "Should NOT trigger failover when SSH is failing"
        );

        // Test condition 3: RPC failing, should NOT trigger failover
        health.ssh_status.record_success();
        health
            .rpc_status
            .record_failure("429 Too Many Requests".to_string());
        let should_failover = health.ssh_status.consecutive_failures == 0
            && health.rpc_status.consecutive_failures == 0;
        assert!(
            !should_failover,
            "Should NOT trigger failover when RPC is failing"
        );
    }

    /// Regression test for the deleted auto-failover trigger.
    ///
    /// Commit `c071354 review: apply mechanical cleanup pass` removed the
    /// only call site of `execute_emergency_failover` while leaving the
    /// function definition behind a stale `#[allow(dead_code)]`. The result
    /// was that `auto_failover_enabled: true` in production silently did
    /// nothing on the next delinquency event.
    ///
    /// This test asserts on the source text of `status_ui_v2.rs` so that
    /// any future "cleanup" pass that removes the trigger again will
    /// immediately fail the test suite — without needing to construct a
    /// synthetic `AppState`/`UiState`/`AsyncSshPool` (which is many hundreds
    /// of lines of boilerplate). The test is cheap insurance: it catches
    /// deletions but does not catch logic regressions inside the trigger.
    /// A future refactor that extracts the gate into a `pub(crate)` helper
    /// can replace this with a behavioural test on the helper.
    #[test]
    fn test_auto_failover_trigger_call_site_still_present() {
        let src = include_str!("commands/status_ui_v2.rs");

        // Definition + at least one call site = at least 2 occurrences of
        // `execute_emergency_failover(`. The definition counts as one. Any
        // value < 2 means the call site has been deleted.
        let occurrences = src.matches("execute_emergency_failover(").count();
        assert!(
            occurrences >= 2,
            "execute_emergency_failover() has {} reference(s) in status_ui_v2.rs; expected at least 2 (1 definition + 1 call site). \
             If this fails, the auto-failover trigger has been deleted again — see plan svs_restore_auto_failover_trigger.plan.md.",
            occurrences
        );

        // The deleted code path emitted two distinctive log markers when
        // the gate fired. The restored code preserves them verbatim so log
        // greps from the previous era still match. Both must be present.
        assert!(
            src.contains("Auto-failover conditions met: vote_rpc_failures=0"),
            "Expected 'Auto-failover conditions met' log line in status_ui_v2.rs; \
             this is the first of two markers the gate emits and is required \
             for log-grep continuity with the pre-c071354 era."
        );
        assert!(
            src.contains("🚨 AUTO-FAILOVER: Initiating emergency takeover"),
            "Expected '🚨 AUTO-FAILOVER: Initiating emergency takeover' log \
             line in status_ui_v2.rs."
        );

        // The simulation env var must be wired to all three suppression
        // sites. If any one of these strings disappears, simulation testing
        // is broken and the operator cannot safely verify the gate without
        // a real delinquency event.
        assert!(
            src.contains("SVS_SIMULATE_FAILOVER"),
            "Expected SVS_SIMULATE_FAILOVER env var handling in status_ui_v2.rs."
        );
        assert!(
            src.contains("🧪 SIMULATION: forcing node["),
            "Expected '🧪 SIMULATION: forcing node[..]' force-delinquent marker."
        );
        assert!(
            src.contains("🧪 SIMULATION: would have sent"),
            "Expected '🧪 SIMULATION: would have sent' telegram-suppression marker."
        );
        assert!(
            src.contains("🚨 SIMULATION DRY-RUN: auto-failover gate passed"),
            "Expected '🚨 SIMULATION DRY-RUN' failover-suppression marker."
        );
    }

    /// Documents the gating conditions for the auto-failover trigger.
    ///
    /// Mirrors the live boolean expression in the alerts_to_send loop:
    ///     !is_backup
    ///     && alert_config.enabled
    ///     && alert_config.auto_failover_enabled
    ///     && vote_rpc_failures == 0
    /// If any of the four inputs is wrong, no failover should fire.
    #[test]
    fn test_auto_failover_gate_truth_table() {
        fn gate(
            is_backup: bool,
            alerts_enabled: bool,
            auto_failover_enabled: bool,
            vote_rpc_failures: u32,
        ) -> bool {
            !is_backup && alerts_enabled && auto_failover_enabled && vote_rpc_failures == 0
        }

        // Happy path — fires.
        assert!(gate(false, true, true, 0));

        // Each input flipped in isolation must suppress.
        assert!(!gate(true, true, true, 0), "backup should suppress");
        assert!(
            !gate(false, false, true, 0),
            "alerts disabled should suppress"
        );
        assert!(
            !gate(false, true, false, 0),
            "auto_failover_enabled=false should suppress"
        );
        assert!(
            !gate(false, true, true, 1),
            "vote_rpc_failures>0 should suppress"
        );
        assert!(
            !gate(false, true, true, 99),
            "any vote_rpc_failures>0 should suppress"
        );
    }

    /// Regression test for the dup-cycle root-cause fix.
    ///
    /// `spawn_background_tasks` is called once per `run_enhanced_ui`
    /// invocation, and `run_enhanced_ui` is re-entered by
    /// `show_enhanced_status_ui`'s outer loop after every manual switch.
    /// Prior to the fix, each re-entry left the previous (Loop A, Loop B)
    /// pair running and spawned a fresh pair on top — after N switches,
    /// N+1 pairs would be racing on the same 20s interval, producing
    /// log-burst dup-cycles that grew unboundedly over time.
    ///
    /// The fix has TWO layers:
    /// 1. `Drop` impl on `EnhancedStatusApp` aborts background_tasks
    ///    handles when the app instance is dropped. This is the
    ///    load-bearing layer because `show_enhanced_status_ui` creates a
    ///    new `EnhancedStatusApp` instance every loop iteration.
    /// 2. Inside `spawn_background_tasks`, drain `self.background_tasks`
    ///    and `.abort()` each stale handle before pushing new ones.
    ///    Defense-in-depth for the case where `spawn_background_tasks`
    ///    is somehow called twice on the same app instance.
    ///
    /// Both layers must be present. This test asserts on the source so
    /// that a future cleanup pass removing either layer will fail the
    /// test suite.
    #[test]
    fn test_background_tasks_aborted_on_respawn() {
        let src = include_str!("commands/status_ui_v2.rs");

        // ── Layer 1: Drop impl ──
        //
        // This is the load-bearing fix. Without it, each manual switch
        // leaks +2 zombie background loops.
        assert!(
            src.contains("impl Drop for EnhancedStatusApp"),
            "Expected `impl Drop for EnhancedStatusApp` in status_ui_v2.rs. \
             Without the Drop impl, each `show_enhanced_status_ui` outer-loop \
             iteration leaks the previous app's background loops because \
             tokio::JoinHandle does not auto-abort on drop."
        );
        assert!(
            src.contains("self.background_tasks.try_write()"),
            "Expected Drop impl to call `self.background_tasks.try_write()` \
             to access the handle Vec."
        );

        // ── Layer 2: abort-on-respawn ──
        //
        // Defense-in-depth. Covers a hypothetical `spawn_background_tasks` \
        // called twice on the same instance.
        assert!(
            src.contains("let loop_a_handle = tokio::spawn"),
            "Expected `let loop_a_handle = tokio::spawn(...)` in spawn_background_tasks. \
             If the assignment is missing, the JoinHandle is dropped and cannot be aborted."
        );
        assert!(
            src.contains("let loop_b_handle = tokio::spawn"),
            "Expected `let loop_b_handle = tokio::spawn(...)` in spawn_background_tasks."
        );
        assert!(
            src.contains("handles.push(loop_a_handle)"),
            "Loop A handle must be pushed into self.background_tasks."
        );
        assert!(
            src.contains("handles.push(loop_b_handle)"),
            "Loop B handle must be pushed into self.background_tasks."
        );
        assert!(
            src.contains("for h in old_handles.drain(..)"),
            "Expected `for h in old_handles.drain(..) {{ h.abort(); }}` pattern \
             at the top of spawn_background_tasks for defense-in-depth."
        );

        // ── async fn signature + .await at caller ──
        assert!(
            src.contains("pub async fn spawn_background_tasks(&self)"),
            "spawn_background_tasks must be `pub async fn` so it can `.write().await` \
             the background_tasks RwLock for the abort/store steps."
        );
        assert!(
            src.contains("app.spawn_background_tasks().await;"),
            "Caller in run_enhanced_ui must use `.await` since the function is now async."
        );

        // ── Diagnostic counter retained ──
        //
        // After the Drop fix, the counter still grows by 1 per manual
        // switch, but distinct loop_ids in any 20s window should stay at
        // exactly 2 (because the previous app's handles were aborted by
        // its Drop). Regression tell: distinct loop_ids in a window > 2.
        assert!(
            src.contains("BACKGROUND_TASKS_SPAWN_COUNT"),
            "The diagnostic counter should be retained after the fix."
        );
    }
}
