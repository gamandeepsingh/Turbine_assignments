use anchor_lang::prelude::*;

/// Global pool state.
/// PDA: [b"pool", token_a_mint, token_b_mint]
///
/// token_a_mint must be lexicographically smaller than token_b_mint so that
/// only one pool can exist per pair (clients must sort mints before calling).
#[account]
#[derive(InitSpace)]
pub struct Pool {
    pub token_a_mint: Pubkey,
    pub token_b_mint: Pubkey,
    /// Mint for LP tokens representing pool shares.
    /// PDA: [b"lp_mint", pool]
    pub lp_mint: Pubkey,
    /// Total outstanding LP tokens (mirrors the mint supply, kept here for
    /// cheap reads without deserialising the Mint account in every instruction).
    pub lp_supply: u64,
    pub bump: u8,
    pub lp_mint_bump: u8,
    pub vault_a_bump: u8,
    pub vault_b_bump: u8,
}
