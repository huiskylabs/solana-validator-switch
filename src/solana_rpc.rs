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

pub async fn fetch_vote_account_data(
    rpc_url: &str,
    vote_pubkey_str: &str,
) -> Result<ValidatorVoteData> {
    use std::time::Duration;

    // Validate RPC URL
    if rpc_url.is_empty() {
        return Err(anyhow!("RPC URL is empty"));
    }

    // Log the RPC URL being used (for debugging)
    // eprintln!("Using RPC URL: {}", rpc_url);
    // eprintln!("Looking for vote account: {}", vote_pubkey_str);

    let rpc_client = RpcClient::new_with_timeout(rpc_url.to_string(), Duration::from_secs(3));
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

    // Get detailed vote account data. We still ask for this because the
    // deserialized VoteState gives us a richer view (recent votes list with
    // per-vote latency, credits, last_timestamp) — but newer on-chain vote
    // state formats (e.g. VoteStateV4 introduced with Agave 2.x / Firedancer
    // 0.5+) are not understood by the deserializer in solana-sdk 1.18 and
    // produce a "invalid account data for instruction" error. When that
    // happens we fall back to the lighter view derivable from `vote_info`,
    // which is enough to keep delinquency detection working.
    let account_data = rpc_client
        .get_account(&vote_pubkey)
        .map_err(|e| anyhow!("Failed to get vote account data: {}", e))?;

    let vote_state = solana_sdk::vote::state::VoteState::deserialize(&account_data.data).ok();

    let current_slot = rpc_client
        .get_slot()
        .map_err(|e| anyhow!("Failed to get current slot: {}", e))?;

    // Build the recent_votes list. Prefer the rich VoteState path; fall back
    // to a single synthesized entry from vote_info.last_vote when the on-chain
    // format is newer than what the SDK can decode.
    let mut recent_votes = Vec::new();
    if let Some(ref vs) = vote_state {
        // Get the most recent votes (up to 31 as shown in the example).
        // The votes are stored in order, with most recent at the end.
        let vote_count = vs.votes.len();
        for (i, lockout) in vs.votes.iter().rev().take(31).enumerate() {
            // Calculate latency as difference between consecutive votes.
            // For the most recent vote, use current slot.
            let latency = if i == 0 {
                // Most recent vote - latency from current slot
                current_slot.saturating_sub(lockout.slot())
            } else if i < vote_count - 1 {
                // Get the next more recent vote (previous in reversed iteration)
                if let Some(next_vote) = vs.votes.get(vote_count - i) {
                    next_vote.slot().saturating_sub(lockout.slot())
                } else {
                    1 // Default latency
                }
            } else {
                1 // Default latency for oldest vote
            };

            recent_votes.push(RecentVote {
                slot: lockout.slot(),
                confirmation_count: (i + 1) as u32,
                latency,
            });
        }
    } else {
        // Fallback path: we couldn't decode the on-chain VoteState (likely a
        // VoteStateV4 / newer format). vote_info.last_vote is still trustworthy
        // because it comes from get_vote_accounts() and doesn't require account
        // data decoding on our side. One entry is enough to drive delinquency
        // detection in status_ui_v2; the richer UI columns (latency over the
        // last 31 votes, missed-vote window, etc.) simply degrade.
        recent_votes.push(RecentVote {
            slot: vote_info.last_vote,
            confirmation_count: 1,
            latency: current_slot.saturating_sub(vote_info.last_vote),
        });
    }

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
