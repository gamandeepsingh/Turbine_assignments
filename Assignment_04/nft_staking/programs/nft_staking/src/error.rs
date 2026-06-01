use anchor_lang::prelude::*;

#[error_code]
pub enum StakeError {
    #[msg("NFT is not owned by the signer")]
    NotOwner,
    #[msg("NFT does not belong to the expected collection")]
    InvalidCollection,
    #[msg("Freeze period has not elapsed — too early to unstake")]
    FreezePeriodNotElapsed,
    #[msg("User has reached the maximum staking limit")]
    MaxStakeReached,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("No rewards to claim")]
    NoRewards,
    #[msg("Clock is unavailable")]
    ClockUnavailable,
}
