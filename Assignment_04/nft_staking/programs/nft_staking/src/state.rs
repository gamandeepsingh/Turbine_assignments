use anchor_lang::prelude::*;

#[account]
#[derive(InitSpace)]
pub struct StakeConfig {
    /// Reward tokens minted per second per staked NFT
    pub points_per_stake: u8,
    /// Maximum NFTs a single user can stake at once
    pub max_stake: u8,
    /// Minimum lock period in seconds before unstake is allowed
    pub freeze_period: u32,
    pub bump: u8,
    pub reward_bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct UserAccount {
    /// Accumulated unclaimed reward token amount (in lamports of reward mint)
    pub points: u64,
    /// Number of NFTs currently staked by this user
    pub amount_staked: u8,
    pub bump: u8,
}

#[account]
#[derive(InitSpace)]
pub struct StakeAccount {
    pub owner: Pubkey,
    /// MPL Core asset address of the staked NFT
    pub asset: Pubkey,
    /// Timestamp of stake or last reward claim — used to compute pending rewards
    pub last_update: i64,
    pub bump: u8,
}
