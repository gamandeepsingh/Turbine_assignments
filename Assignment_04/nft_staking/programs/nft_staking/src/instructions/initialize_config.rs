use anchor_lang::prelude::*;
use anchor_spl::token::{Mint, Token};

use crate::state::StakeConfig;

pub(crate) fn handler(
    ctx: Context<InitializeConfig>,
    points_per_stake: u8,
    max_stake: u8,
    freeze_period: u32,
) -> Result<()> {
    let cfg = &mut ctx.accounts.config;
    cfg.points_per_stake = points_per_stake;
    cfg.max_stake = max_stake;
    cfg.freeze_period = freeze_period;
    cfg.bump = ctx.bumps.config;
    cfg.reward_bump = ctx.bumps.reward_mint;
    Ok(())
}

#[derive(Accounts)]
pub struct InitializeConfig<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    #[account(
        init,
        payer = admin,
        space = 8 + StakeConfig::INIT_SPACE,
        seeds = [b"config"],
        bump,
    )]
    pub config: Account<'info, StakeConfig>,

    /// SPL Token mint for distributing staking rewards
    #[account(
        init,
        payer = admin,
        mint::decimals = 6,
        mint::authority = config,
        seeds = [b"rewards", config.key().as_ref()],
        bump,
    )]
    pub reward_mint: Account<'info, Mint>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}
