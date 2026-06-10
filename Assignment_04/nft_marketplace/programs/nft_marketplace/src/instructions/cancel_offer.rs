use anchor_lang::prelude::*;

use crate::{error::MarketplaceError, state::Offer};

pub(crate) fn handler(ctx: Context<CancelOffer>) -> Result<()> {
    // The `close = buyer` constraint on `offer` drains all lamports back to
    // the buyer (including the escrowed amount + rent). Nothing else to do.
    let _ = &ctx.accounts;
    Ok(())
}

#[derive(Accounts)]
pub struct CancelOffer<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    /// CHECK: asset public key is just used as a seed
    pub asset: UncheckedAccount<'info>,

    #[account(
        mut,
        close = buyer,
        seeds = [b"offer", asset.key().as_ref(), buyer.key().as_ref()],
        bump = offer.bump,
        constraint = offer.buyer == buyer.key() @ MarketplaceError::NotBuyer,
    )]
    pub offer: Account<'info, Offer>,

    pub system_program: Program<'info, System>,
}
