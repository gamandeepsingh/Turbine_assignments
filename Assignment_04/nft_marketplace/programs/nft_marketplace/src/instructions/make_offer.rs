use anchor_lang::{prelude::*, system_program};

use crate::{error::MarketplaceError, state::Offer};

pub(crate) fn handler(ctx: Context<MakeOffer>, amount: u64) -> Result<()> {
    require!(amount > 0, MarketplaceError::ZeroAmount);

    let offer = &mut ctx.accounts.offer;
    offer.buyer = ctx.accounts.buyer.key();
    offer.asset = ctx.accounts.asset.key();
    offer.amount = amount;
    offer.bump = ctx.bumps.offer;

    // Escrow the offered SOL into the Offer PDA.
    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.buyer.to_account_info(),
                to: ctx.accounts.offer.to_account_info(),
            },
        ),
        amount,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct MakeOffer<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    /// CHECK: asset public key is just used as a seed; MPL Core validates it elsewhere
    pub asset: UncheckedAccount<'info>,

    #[account(
        init,
        payer = buyer,
        space = 8 + Offer::INIT_SPACE,
        seeds = [b"offer", asset.key().as_ref(), buyer.key().as_ref()],
        bump,
    )]
    pub offer: Account<'info, Offer>,

    pub system_program: Program<'info, System>,
}
