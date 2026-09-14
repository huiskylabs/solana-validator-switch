use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VoteAccountInfo {
    pub vote_pubkey: String,
    pub validator_identity: String,
    pub activated_stake: u64,
    pub commission: u8,
    pub root_slot: u64,
    pub last_vote: u64,
    pub credits: u64,
    pub recent_timestamp: Option<String>,
    pub current_slot: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentVote {
    pub slot: u64,
    pub confirmation_count: u32,
    pub latency: u64,
}

#[derive(Debug, Clone)]
pub struct TvcPerformanceMetrics {
    pub tvc_rank: u32,
    pub total_validators: u32,
    pub avg_vote_latency: f64,
    pub missed_votes: u64,
    pub missed_votes_window: u64,
}

#[derive(Debug, Clone)]
pub struct ValidatorVoteData {
    #[allow(dead_code)]
    pub vote_account_info: VoteAccountInfo,
    pub recent_votes: Vec<RecentVote>,
    pub is_voting: bool,
    pub tvc_metrics: Option<TvcPerformanceMetrics>,
}

fn compute_tvc_rank(
    vote_account: &solana_client::rpc_response::RpcVoteAccountStatus,
    vote_pubkey_str: &str,
) -> Option<(u32, u32)> {
    let mut epoch_credits: Vec<(String, u64)> = vote_account
        .current
        .iter()
        .chain(vote_account.delinquent.iter())
        .filter_map(|acct| {
            acct.epoch_credits.last().map(|&(_, credits, prev)| {
                (acct.vote_pubkey.clone(), credits.saturating_sub(prev))
            })
        })
        .collect();

    epoch_credits.sort_by(|a, b| b.1.cmp(&a.1));
    let total = epoch_credits.len() as u32;
    let rank = epoch_credits
        .iter()
        .position(|(pk, _)| pk == vote_pubkey_str)
        .map(|pos| (pos as u32) + 1)?;
    Some((rank, total))
}

fn compute_avg_vote_latency(recent_votes: &[RecentVote]) -> Option<f64> {
    if recent_votes.len() <= 1 {
        return None;
    }
    // Exclude the last element (oldest vote, which defaults to 1)
    let votes_to_avg = &recent_votes[..recent_votes.len() - 1];
    if votes_to_avg.is_empty() {
        return None;
    }
    let sum: u64 = votes_to_avg.iter().map(|v| v.latency).sum();
    Some(sum as f64 / votes_to_avg.len() as f64)
}

fn compute_missed_votes(
    votes: &std::collections::VecDeque<solana_sdk::vote::state::LandedVote>,
    current_slot: u64,
    max_window: u64,
) -> (u64, u64) {
    if votes.is_empty() {
        return (0, 0);
    }
    let voted_slots: std::collections::HashSet<u64> =
        votes.iter().map(|l| l.lockout.slot()).collect();
    let oldest_slot = votes.front().map(|l| l.lockout.slot()).unwrap_or(current_slot);
    let raw_window = current_slot.saturating_sub(oldest_slot) + 1;
    let effective_window = raw_window.min(max_window);
    let window_start = current_slot.saturating_sub(effective_window - 1);
    let voted_in_window = voted_slots
        .iter()
        .filter(|&&s| s >= window_start && s <= current_slot)
        .count() as u64;
    let missed = effective_window.saturating_sub(voted_in_window);
    (missed, effective_window)
}

/// Timeout for the cluster RPC calls vote monitoring depends on.
///
/// The original 3s was below this endpoint's tail latency. Two calls at this
/// bound still fit inside the 60s vote poll interval.
const RPC_CALL_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(10);

/// Timeout for the optional vote-account enrichment call.
///
/// `api.testnet.solana.com` answers `getAccountInfo` in ~0.2s most of the time
/// but intermittently stalls for 5-10s. Enrichment is decoration, so it gets a
/// tight budget: waiting on it would delay the delinquency check behind it by
/// longer than the 30s delinquency threshold is trying to detect.
const ENRICHMENT_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(2);

/// Whether an enrichment state change is worth a log line.
///
/// Enrichment runs every poll. Logging each failure produced one warning per
/// minute forever; logging only transitions reports the same information once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EnrichmentTransition {
    /// Enrichment just started failing.
    Degraded,
    /// Enrichment just started working again.
    Recovered,
    /// No change since the last poll.
    Unchanged,
}

/// Compare the current enrichment outcome against the previous one.
pub(crate) fn enrichment_transition(
    previously_ok: Option<bool>,
    currently_ok: bool,
) -> EnrichmentTransition {
    match (previously_ok, currently_ok) {
        (Some(true), false) | (None, false) => EnrichmentTransition::Degraded,
        (Some(false), true) => EnrichmentTransition::Recovered,
        _ => EnrichmentTransition::Unchanged,
    }
}

/// Last enrichment outcome per vote account, so only transitions are logged.
fn enrichment_state() -> &'static std::sync::Mutex<std::collections::HashMap<String, bool>> {
    static STATE: std::sync::OnceLock<
        std::sync::Mutex<std::collections::HashMap<String, bool>>,
    > = std::sync::OnceLock::new();
    STATE.get_or_init(|| std::sync::Mutex::new(std::collections::HashMap::new()))
}

/// Record the outcome and report whether it changed.
fn record_enrichment_outcome(vote_pubkey: &str, ok: bool) -> EnrichmentTransition {
    let Ok(mut state) = enrichment_state().lock() else {
        return EnrichmentTransition::Unchanged;
    };
    let transition = enrichment_transition(state.get(vote_pubkey).copied(), ok);
    state.insert(vote_pubkey.to_string(), ok);
    transition
}

/// Why a cluster RPC call failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RpcFailureKind {
    /// The request did not complete in time. Transient; retry on the next poll.
    Timeout,
    /// Anything else — a genuine protocol or data error.
    Other,
}

/// Classify an RPC error string.
///
/// `solana-client` renders a timed-out `get_account` as
/// `AccountNotFound: pubkey=...: error sending request ...: operation timed out`,
/// which reads like the account is missing when it is really a timeout. Callers
/// use this to describe the failure accurately.
pub(crate) fn classify_rpc_failure(error: &str) -> RpcFailureKind {
    let lowered = error.to_ascii_lowercase();
    if lowered.contains("timed out") || lowered.contains("timeout") {
        RpcFailureKind::Timeout
    } else {
        RpcFailureKind::Other
    }
}

/// Build the recent-vote list, preferring decoded `VoteState` and falling back
/// to the single `last_vote` reported by `get_vote_accounts`.
///
/// The fallback must keep working: it is the only thing standing between an
/// undecodable (or unavailable) vote account and total loss of vote monitoring.
fn build_recent_votes(
    vote_state: Option<&solana_sdk::vote::state::VoteState>,
    last_vote: u64,
    current_slot: u64,
) -> Vec<RecentVote> {
    let mut recent_votes = Vec::new();

    if let Some(vs) = vote_state {
        // Votes are stored oldest-first, so iterate in reverse for most-recent.
        let vote_count = vs.votes.len();
        for (i, lockout) in vs.votes.iter().rev().take(31).enumerate() {
            let latency = if i == 0 {
                current_slot.saturating_sub(lockout.slot())
            } else if i < vote_count - 1 {
                if let Some(next_vote) = vs.votes.get(vote_count - i) {
                    next_vote.slot().saturating_sub(lockout.slot())
                } else {
                    1
                }
            } else {
                1
            };

            recent_votes.push(RecentVote {
                slot: lockout.slot(),
                confirmation_count: (i + 1) as u32,
                latency,
            });
        }
    } else {
        // Fallback path: VoteState was undecodable (e.g. VoteStateV4, newer than
        // solana-sdk 1.18 understands) or the enrichment call did not complete.
        // vote_info.last_vote comes from get_vote_accounts() and needs no
        // account-data decoding, so it stays trustworthy. One entry is enough to
        // drive delinquency detection; the richer UI columns simply degrade.
        //
        // Do not write to stderr here: this runs while the TUI is active and
        // direct stderr writes corrupt the display.
        recent_votes.push(RecentVote {
            slot: last_vote,
            confirmation_count: 1,
            latency: current_slot.saturating_sub(last_vote),
        });
    }

    recent_votes
}

pub async fn fetch_vote_account_data(
    rpc_url: &str,
    vote_pubkey_str: &str,
) -> Result<ValidatorVoteData> {
    // Validate RPC URL
    if rpc_url.is_empty() {
        return Err(anyhow!("RPC URL is empty"));
    }

    // `RpcClient` here is the blocking client, so it must not run on a Tokio
    // worker thread. Driving it inline starved the monitoring loops after a
    // switch spawned a second set of them, and every call then hit the timeout
    // until the process was restarted.
    let rpc_url = rpc_url.to_string();
    let vote_pubkey_str = vote_pubkey_str.to_string();
    tokio::task::spawn_blocking(move || fetch_vote_account_data_blocking(&rpc_url, &vote_pubkey_str))
        .await
        .map_err(|e| anyhow!("Vote data task failed to run: {}", e))?
}

fn fetch_vote_account_data_blocking(
    rpc_url: &str,
    vote_pubkey_str: &str,
) -> Result<ValidatorVoteData> {
    let rpc_client = RpcClient::new_with_timeout(rpc_url.to_string(), RPC_CALL_TIMEOUT);
    let vote_pubkey =
        Pubkey::from_str(vote_pubkey_str).map_err(|e| anyhow!("Invalid vote pubkey: {}", e))?;

    // Get vote account info
    let vote_account = rpc_client
        .get_vote_accounts()
        .map_err(|e| anyhow!("Failed to get vote accounts: {}", e))?;

    // Find our specific vote account in current or delinquent
    let vote_info = vote_account
        .current
        .iter()
        .chain(vote_account.delinquent.iter())
        .find(|account| account.vote_pubkey == vote_pubkey_str)
        .ok_or_else(|| {
            let total_accounts = vote_account.current.len() + vote_account.delinquent.len();
            anyhow!("Vote account {} not found among {} vote accounts. Make sure the RPC endpoint matches the network (mainnet/testnet/devnet) where this vote account exists.", vote_pubkey_str, total_accounts)
        })?;

    // Detailed account data is enrichment only: it yields per-vote latency,
    // credits and last_timestamp. Everything delinquency detection needs is
    // already in `vote_info`. A failure here must therefore degrade rather than
    // abort — returning Err would mark the whole poll a vote-RPC failure, and
    // auto-failover is gated on `vote_rpc_failures == 0`, so a slow optional
    // call would silently disarm failover.
    //
    // It gets its own short-timeout client so a stalled optional call cannot
    // hold up the delinquency check that follows it.
    let enrichment_client =
        RpcClient::new_with_timeout(rpc_url.to_string(), ENRICHMENT_TIMEOUT);
    let vote_state = match enrichment_client.get_account(&vote_pubkey) {
        Ok(account_data) => {
            if record_enrichment_outcome(vote_pubkey_str, true) == EnrichmentTransition::Recovered {
                crate::startup_logger::append_runtime_log(
                    "INFO",
                    vote_pubkey_str,
                    "Vote account enrichment recovered; full vote metrics available again.",
                );
            }
            solana_sdk::vote::state::VoteState::deserialize(&account_data.data).ok()
        }
        Err(error) => {
            if record_enrichment_outcome(vote_pubkey_str, false) == EnrichmentTransition::Degraded {
                let detail = error.to_string();
                let reason = match classify_rpc_failure(&detail) {
                    RpcFailureKind::Timeout => "request timed out",
                    RpcFailureKind::Other => "request failed",
                };
                // File-only: this runs while the TUI owns the terminal, so
                // stderr writes would corrupt the display.
                crate::startup_logger::append_runtime_log(
                    "WARNING",
                    vote_pubkey_str,
                    &format!(
                        "Vote account enrichment unavailable ({reason}); continuing with vote \
                         data from getVoteAccounts. Vote latency and missed-vote metrics will \
                         show as unavailable until it recovers. Detail: {detail}"
                    ),
                );
            }
            None
        }
    };

    let current_slot = rpc_client
        .get_slot()
        .map_err(|e| anyhow!("Failed to get current slot: {}", e))?;

    let recent_votes = build_recent_votes(vote_state.as_ref(), vote_info.last_vote, current_slot);

    // Compute TVC performance metrics from already-fetched data
    let tvc_metrics = {
        let rank_data = compute_tvc_rank(&vote_account, vote_pubkey_str);
        let avg_latency = compute_avg_vote_latency(&recent_votes);
        // Missed-vote counting needs the full lockout history; we only have
        // that on the rich path. On the fallback path we report
        // (missed=0, window=0) which the UI can interpret as "no data".
        let (missed, window) = if let Some(ref vs) = vote_state {
            compute_missed_votes(&vs.votes, current_slot, 500)
        } else {
            (0, 0)
        };

        match (rank_data, avg_latency) {
            (Some((rank, total)), Some(latency)) => Some(TvcPerformanceMetrics {
                tvc_rank: rank,
                total_validators: total,
                avg_vote_latency: latency,
                missed_votes: missed,
                missed_votes_window: window,
            }),
            _ => None,
        }
    };

    // Determine if validator is voting (has voted recently)
    let is_voting = if let Some(last_vote) = recent_votes.first() {
        last_vote.latency < 150 // Consider voting if voted within last 150 slots (~1 minute)
    } else {
        false
    };

    // Pull credits and timestamp from the rich path when we have it, otherwise
    // fall back: epoch_credits is part of vote_info and gives the cumulative
    // credit count without needing to decode the account data ourselves.
    let credits = if let Some(ref vs) = vote_state {
        vs.credits()
    } else {
        vote_info
            .epoch_credits
            .last()
            .map(|(_, credits, _)| *credits)
            .unwrap_or(0)
    };

    let recent_timestamp = vote_state.as_ref().map(|vs| {
        chrono::DateTime::<chrono::Utc>::from_timestamp(vs.last_timestamp.timestamp, 0)
            .unwrap_or_default()
            .format("%Y-%m-%dT%H:%M:%SZ")
            .to_string()
    });

    Ok(ValidatorVoteData {
        vote_account_info: VoteAccountInfo {
            vote_pubkey: vote_pubkey_str.to_string(),
            validator_identity: vote_info.node_pubkey.clone(),
            activated_stake: vote_info.activated_stake,
            commission: vote_info.commission,
            root_slot: vote_info.root_slot,
            last_vote: vote_info.last_vote,
            credits,
            recent_timestamp,
            current_slot: Some(current_slot),
        },
        recent_votes,
        is_voting,
        tvc_metrics,
    })
}

#[cfg(test)]
mod vote_data_degradation_tests {
    //! Vote monitoring must survive the loss of optional enrichment.
    //!
    //! `get_account` only adds per-vote latency, credits and last_timestamp.
    //! When it was fatal, a single slow call marked the whole poll a vote-RPC
    //! failure — and because auto-failover requires `vote_rpc_failures == 0`,
    //! that silently disarmed failover until the process was restarted.

    use super::{
        build_recent_votes, classify_rpc_failure, enrichment_transition, EnrichmentTransition,
        RpcFailureKind,
    };

    #[test]
    fn repeated_enrichment_failures_are_logged_once_not_every_poll() {
        // Enrichment runs every 60s. Logging each failure produced one warning
        // per minute indefinitely; only the transitions carry information.
        assert_eq!(
            enrichment_transition(None, false),
            EnrichmentTransition::Degraded,
            "first failure should be reported"
        );
        assert_eq!(
            enrichment_transition(Some(false), false),
            EnrichmentTransition::Unchanged,
            "still-failing must stay silent"
        );
        assert_eq!(
            enrichment_transition(Some(false), true),
            EnrichmentTransition::Recovered,
            "recovery should be reported"
        );
        assert_eq!(
            enrichment_transition(Some(true), true),
            EnrichmentTransition::Unchanged,
            "steady-state success must stay silent"
        );
        assert_eq!(
            enrichment_transition(Some(true), false),
            EnrichmentTransition::Degraded,
            "regression after success should be reported"
        );
    }

    #[test]
    fn first_successful_poll_is_silent() {
        // Normal startup must not announce that nothing is wrong.
        assert_eq!(
            enrichment_transition(None, true),
            EnrichmentTransition::Unchanged
        );
    }

    #[test]
    fn enrichment_loss_still_yields_a_vote_from_get_vote_accounts() {
        let votes = build_recent_votes(None, 435_692_331, 435_692_379);

        assert_eq!(votes.len(), 1, "delinquency detection needs at least one vote");
        assert_eq!(votes[0].slot, 435_692_331);
        assert_eq!(votes[0].confirmation_count, 1);
        assert_eq!(votes[0].latency, 48);
    }

    #[test]
    fn fallback_latency_saturates_when_last_vote_is_ahead_of_current_slot() {
        // Cluster slot can lag the node's reported last_vote across RPC reads.
        let votes = build_recent_votes(None, 500, 400);

        assert_eq!(votes.len(), 1);
        assert_eq!(votes[0].latency, 0, "must not underflow");
    }

    #[test]
    fn timed_out_get_account_is_not_reported_as_a_missing_account() {
        // Verbatim from the observed failure; solana-client renders a timeout
        // behind an AccountNotFound prefix.
        let observed = "AccountNotFound: pubkey=5DM6MhByWpupUehpJsoPHtt1MguGMgzBk32shquyLbLs: \
                        error sending request for url (https://api.testnet.solana.com/): \
                        operation timed out";

        assert_eq!(classify_rpc_failure(observed), RpcFailureKind::Timeout);
    }

    #[test]
    fn genuine_errors_are_not_classified_as_timeouts() {
        for error in [
            "invalid account data for instruction",
            "AccountNotFound: pubkey=abc",
            "RPC response error -32005: Node is behind by 398 slots",
        ] {
            assert_eq!(
                classify_rpc_failure(error),
                RpcFailureKind::Other,
                "misclassified: {error}"
            );
        }
    }
}