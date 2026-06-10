use anchor_lang::prelude::*;

#[error_code]
pub enum AmmError {
    #[msg("Slippage tolerance exceeded")]
    SlippageExceeded,
    #[msg("Insufficient liquidity in pool")]
    InsufficientLiquidity,
    #[msg("Deposit amounts must both be non-zero")]
    InvalidAmounts,
    #[msg("The instruction immediately before withdraw must be burn_lp for this pool")]
    MissingBurnInstruction,
    #[msg("The burn_lp instruction referenced a different pool")]
    WrongPool,
    #[msg("The burn_lp instruction was signed by a different user")]
    WrongUser,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("LP amount resolves to zero")]
    ZeroLp,
}
