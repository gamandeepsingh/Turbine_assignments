use anchor_lang::prelude::*;

#[error_code]
pub enum MarketplaceError {
    #[msg("Fee must be 0–10 000 bps")]
    InvalidFee,
    #[msg("Marketplace name must be 1–32 characters")]
    InvalidName,
    #[msg("Caller is not the asset seller")]
    NotSeller,
    #[msg("Caller is not the offer maker")]
    NotBuyer,
    #[msg("Wrong payment mint for this listing")]
    WrongPaymentMint,
    #[msg("Listing is not a SOL listing")]
    NotSolListing,
    #[msg("Offer amount must be greater than zero")]
    ZeroAmount,
    #[msg("Arithmetic overflow")]
    Overflow,
}
