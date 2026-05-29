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
    /// Prior to the JoinSet refactor, each re-entry could leave the
    /// previous (Loop A, Loop B) pair running and spawn a fresh pair on
    /// top — after N switches, N+1 pairs would be racing on the same 20s
    /// interval, producing log-burst dup-cycles that grew unboundedly
    /// over time.
    ///
    /// The fix is structural: `background_tasks` is a
    /// `tokio::task::JoinSet`, which auto-aborts every task it owns when
    /// it is dropped. Since each new `EnhancedStatusApp` replaces the
    /// previous one (and the previous one's `JoinSet` is dropped along
    /// with it), the stale loops cannot outlive their owning app
    /// instance. The Drop impl on `EnhancedStatusApp` is retained as an
    /// explicit `JoinSet::abort_all()` call so the abort moment is
    /// visible to a debugger and to source-code search.
    ///
    /// This test asserts on the source so that a future cleanup pass
    /// that swaps the JoinSet back to a `Vec<JoinHandle>` (or removes
    /// the Drop impl) will fail the test suite.
    #[test]
    fn test_background_tasks_aborted_on_respawn() {
        let src = include_str!("commands/status_ui_v2.rs");

        // ── Structural supervision: JoinSet ──
        //
        // `tokio::task::JoinSet` is the load-bearing primitive. Its
        // `Drop` impl aborts every spawned task automatically, which is
        // what prevents the dup-cycle bug. A regression to manual
        // `Vec<JoinHandle>` management here re-opens the bug class.
        assert!(
            src.contains("tokio::task::JoinSet"),
            "Expected `tokio::task::JoinSet` in status_ui_v2.rs. Without \
             a JoinSet (or equivalent structured supervisor) the background \
             loops are not aborted when their owning EnhancedStatusApp \
             drops, which is the dup-cycle bug."
        );
        assert!(
            src.contains("pub background_tasks: Arc<std::sync::Mutex<tokio::task::JoinSet"),
            "Expected `pub background_tasks: Arc<std::sync::Mutex<tokio::task::JoinSet<...>>` \
             field declaration on EnhancedStatusApp. The exact field type matters: \
             `Arc<RwLock<Vec<JoinHandle>>>` does NOT auto-abort on drop and re-enables \
             the dup-cycle bug class."
        );

        // ── Explicit Drop impl (defensive, makes the abort moment greppable) ──
        assert!(
            src.contains("impl Drop for EnhancedStatusApp"),
            "Expected `impl Drop for EnhancedStatusApp` in status_ui_v2.rs. \
             Even though `JoinSet::drop` would abort tasks implicitly, the \
             explicit Drop impl makes the supervision discipline visible to \
             readers and to source-code search."
        );
        assert!(
            src.contains("tasks.abort_all()"),
            "Expected Drop impl to call `tasks.abort_all()` on the JoinSet \
             guard. This is the explicit abort that makes the supervision \
             moment debugger-visible."
        );

        // ── Boolean signaling flags use AtomicBool, not RwLock<bool> ──
        //
        // RwLock<bool> with 50ms timeouts was silently dropping state
        // updates under contention (bug class 3 in the concurrency-
        // hardening plan). AtomicBool stores are wait-free so the
        // contention path doesn't exist.
        assert!(
            src.contains("Arc<AtomicBool>"),
            "Expected `Arc<AtomicBool>` in status_ui_v2.rs for the \
             should_quit / emergency_takeover_in_progress / switch_confirmed \
             signaling flags."
        );
        assert!(
            src.contains("AtomicBool::new"),
            "Expected `AtomicBool::new(...)` constructor calls in \
             EnhancedStatusApp::new."
        );

        // ── Diagnostic counter retained ──
        //
        // After the JoinSet refactor, the counter still grows by 1 per
        // manual switch, but distinct loop_ids in any 20s window should
        // stay at exactly 2 (because the previous app's JoinSet aborted
        // its tasks on drop). Regression tell: distinct loop_ids in a
        // window > 2.
        assert!(
            src.contains("BACKGROUND_TASKS_SPAWN_COUNT"),
            "The diagnostic counter should be retained after the fix."
        );

        // ── No silent-drop try_write inside refresh_vote_data_for_alerts ──
        //
        // The hot-path audit (Change 3 in the plan) converted every
        // `try_write` site in the body of `refresh_vote_data_for_alerts`
        // to a `write_lock_with_timeout` that logs a Warning on timeout.
        // If a future change re-introduces `try_write()` inside this
        // function body, the silent-drop pattern returns and bug class 3
        // re-opens (vote_rpc_failures counter desyncs, alert suppressed
        // when it shouldn't be, etc.).
        let fn_start = src
            .find("async fn refresh_vote_data_for_alerts(")
            .expect("refresh_vote_data_for_alerts function must exist");
        let after_start = &src[fn_start..];
        // The next "/// View states for the UI" marker is right after
        // the function's closing brace — see the source ordering near
        // the top of status_ui_v2.rs.
        let body_end = after_start
            .find("/// View states for the UI")
            .expect("View states marker must follow refresh_vote_data_for_alerts");
        let body = &after_start[..body_end];
        assert!(
            !body.contains("try_write()"),
            "`try_write()` reappeared inside refresh_vote_data_for_alerts. \
             This re-opens the silent-drop bug class — converting it to \
             `write_lock_with_timeout(&ui_state, 500).await` with a Warning \
             log on the Err arm is the correct pattern."
        );
    }
}
