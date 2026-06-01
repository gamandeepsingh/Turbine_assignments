use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod state;

pub use error::*;
pub use instructions::*;
pub use state::*;

declare_id!("6xBXPR7MpTSa8x3gaTUXQ1WfY7Q3prjTg9HKBiWFJDAh");

#[program]
pub mod nft_staking {
    use super::*;

    /// Create the global staking config and the reward token mint.
    pub fn initialize_config(
        ctx: Context<InitializeConfig>,
        points_per_stake: u8,
        max_stake: u8,
        freeze_period: u32,
    ) -> Result<()> {
        instructions::initialize_config::handler(ctx, points_per_stake, max_stake, freeze_period)
    }

    /// Create a per-user account that tracks points and staked count.
    pub fn initialize_user(ctx: Context<InitializeUser>) -> Result<()> {
        instructions::initialize_user::handler(ctx)
    }

    /// Stake an MPL Core NFT: adds a FreezeDelegate plugin (authority = config
    /// PDA), freezes the asset, and increments the collection's staked_count
    /// Attributes plugin.
    pub fn stake(ctx: Context<Stake>) -> Result<()> {
        instructions::stake::handler(ctx)
    }

    /// Claim accrued rewards without unstaking.  Rewards accumulate at
    /// `points_per_stake` tokens per second.  `last_update` on the stake
    /// account is advanced to `now` so the same seconds cannot be claimed twice.
    /// Calling unstake immediately after this is safe — 0 pending rewards is
    /// valid and the freeze period check is the only gate.
    pub fn claim_rewards(ctx: Context<ClaimRewards>) -> Result<()> {
        instructions::claim_rewards::handler(ctx)
    }

    /// Unstake the NFT (unfreeze + remove FreezeDelegate plugin, close stake
    /// account, decrement collection staked_count).  Does NOT auto-claim rewards;
    /// call `claim_rewards` first if you want the accrued tokens.
    pub fn unstake(ctx: Context<Unstake>) -> Result<()> {
        instructions::unstake::handler(ctx)
    }
}
