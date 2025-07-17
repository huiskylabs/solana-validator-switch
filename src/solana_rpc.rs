use anyhow::{anyhow, Result};
use solana_client::rpc_client::RpcClient;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;
use serde::{Deserialize, Serialize};

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
pub struct ValidatorVoteData {
    pub vote_account_info: VoteAccountInfo,
    pub recent_votes: Vec<RecentVote>,
    pub is_voting: bool,
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
    let vote_pubkey = Pubkey::from_str(vote_pubkey_str)
        .map_err(|e| anyhow!("Invalid vote pubkey: {}", e))?;

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

    // Get detailed vote account data
    let account_data = rpc_client
        .get_account(&vote_pubkey)
        .map_err(|e| anyhow!("Failed to get vote account data: {}", e))?;

    // Parse vote state from account data
    let vote_state = solana_sdk::vote::state::VoteState::deserialize(&account_data.data)
        .map_err(|e| anyhow!("Failed to deserialize vote state: {}", e))?;

    // Get recent votes with latency
    let mut recent_votes = Vec::new();
    let current_slot = rpc_client
        .get_slot()
        .map_err(|e| anyhow!("Failed to get current slot: {}", e))?;

    // Get the most recent votes (up to 31 as shown in the example)
    // The votes are stored in order, with most recent at the end
    let vote_count = vote_state.votes.len();
    for (i, lockout) in vote_state.votes.iter().rev().take(31).enumerate() {
        // Calculate latency as difference between consecutive votes
        // For the most recent vote, use current slot
        let latency = if i == 0 {
            // Most recent vote - latency from current slot
            current_slot.saturating_sub(lockout.slot())
        } else if i < vote_count - 1 {
            // Get the next more recent vote (previous in reversed iteration)
            if let Some(next_vote) = vote_state.votes.get(vote_count - i) {
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

    // Determine if validator is voting (has voted recently)
    let is_voting = if let Some(last_vote) = recent_votes.first() {
        last_vote.latency < 150 // Consider voting if voted within last 150 slots (~1 minute)
    } else {
        false
    };

    // Get recent timestamp if available
    let recent_timestamp = Some(format!("{}", 
        chrono::DateTime::<chrono::Utc>::from_timestamp(vote_state.last_timestamp.timestamp, 0)
            .unwrap_or_default()
            .format("%Y-%m-%dT%H:%M:%SZ")));

    Ok(ValidatorVoteData {
        vote_account_info: VoteAccountInfo {
            vote_pubkey: vote_pubkey_str.to_string(),
            validator_identity: vote_info.node_pubkey.clone(),
            activated_stake: vote_info.activated_stake,
            commission: vote_info.commission,
            root_slot: vote_info.root_slot,
            last_vote: vote_info.last_vote,
            credits: vote_state.credits(),
            recent_timestamp,
            current_slot: Some(current_slot),
        },
        recent_votes,
        is_voting,
    })
}