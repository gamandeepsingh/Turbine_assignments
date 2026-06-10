use anchor_lang::prelude::*;

use crate::{error::MarketplaceError, state::Marketplace};

pub(crate) fn handler(ctx: Context<Initialize>, name: String, fee: u16) -> Result<()> {
    require!(fee <= 10_000, MarketplaceError::InvalidFee);
    require!(!name.is_empty() && name.len() <= 32, MarketplaceError::InvalidName);

    let mp = &mut ctx.accounts.marketplace;
    mp.admin = ctx.accounts.admin.key();
    mp.fee = fee;
    mp.bump = ctx.bumps.marketplace;
    mp.treasury_bump = ctx.bumps.treasury;
    mp.name = name;
    Ok(())
}

#[derive(Accounts)]
#[instruction(name: String)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + Marketplace::INIT_SPACE,
        seeds = [b"marketplace", name.as_bytes()],
        bump,
    )]
    pub marketplace: Account<'info, Marketplace>,

    /// SOL treasury for collecting fees.
    #[account(
        seeds = [b"treasury", marketplace.key().as_ref()],
        bump,
    )]
    pub treasury: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}
