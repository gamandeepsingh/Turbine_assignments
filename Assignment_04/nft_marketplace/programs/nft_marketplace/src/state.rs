use anchor_lang::prelude::*;

/// Global marketplace config.
/// PDA: [b"marketplace", name.as_bytes()]
#[account]
#[derive(InitSpace)]
pub struct Marketplace {
    pub admin: Pubkey,
    /// Fee in basis points (e.g. 200 = 2%).
    pub fee: u16,
    pub bump: u8,
    pub treasury_bump: u8,
    #[max_len(32)]
    pub name: String,
}

/// Active NFT listing.
/// PDA: [b"listing", marketplace, asset]
#[account]
#[derive(InitSpace)]
pub struct Listing {
    pub seller: Pubkey,
    pub asset: Pubkey,
    /// Price in lamports (SOL) or token base units.
    pub price: u64,
    /// system_program::ID → pay in SOL; otherwise the SPL-token mint.
    pub payment_mint: Pubkey,
    pub bump: u8,
}

/// Open buyer offer (SOL-denominated, regardless of listing currency).
/// PDA: [b"offer", asset, buyer]
/// Holds `amount` lamports of escrowed SOL in addition to rent.
#[account]
#[derive(InitSpace)]
pub struct Offer {
    pub buyer: Pubkey,
    pub asset: Pubkey,
    pub amount: u64,
    pub bump: u8,
}
