use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{mint_to, Mint, MintTo, Token, TokenAccount},
};

use crate::{error::StakeError, state::{StakeAccount, StakeConfig, UserAccount}};

pub(crate) fn handler(ctx: Context<ClaimRewards>) -> Result<()> {
    let config = &ctx.accounts.config;
    let stake_account = &mut ctx.accounts.stake_account;

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // Seconds elapsed since stake or last claim
    let elapsed = now
        .checked_sub(stake_account.last_update)
        .ok_or(StakeError::Overflow)? as u64;

    let reward = elapsed
        .checked_mul(config.points_per_stake as u64)
        .ok_or(StakeError::Overflow)?;

    require!(reward > 0, StakeError::NoRewards);

    // Mint reward tokens to user's ATA, signed by config PDA
    let config_seeds: &[&[u8]] = &[b"config", &[config.bump]];
    mint_to(
        CpiContext::new_with_signer(
            Token::id(),
            MintTo {
                mint: ctx.accounts.reward_mint.to_account_info(),
                to: ctx.accounts.user_reward_ata.to_account_info(),
                authority: ctx.accounts.config.to_account_info(),
            },
            &[config_seeds],
        ),
        reward,
    )?;

    // Advance the reward cursor so the same seconds cannot be claimed twice
    stake_account.last_update = now;

    // Accumulate on the user account for off-chain reference
    ctx.accounts.user_account.points = ctx
        .accounts
        .user_account
        .points
        .checked_add(reward)
        .ok_or(StakeError::Overflow)?;

    Ok(())
}

#[derive(Accounts)]
pub struct ClaimRewards<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, StakeConfig>,

    #[account(
        mut,
        seeds = [b"rewards", config.key().as_ref()],
        bump = config.reward_bump,
    )]
    pub reward_mint: Account<'info, Mint>,

    /// CHECK: Validated as the staked asset via stake_account.asset
    pub asset: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [b"stake", asset.key().as_ref(), config.key().as_ref()],
        bump = stake_account.bump,
        constraint = stake_account.owner == user.key() @ StakeError::NotOwner,
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = reward_mint,
        associated_token::authority = user,
    )]
    pub user_reward_ata: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
