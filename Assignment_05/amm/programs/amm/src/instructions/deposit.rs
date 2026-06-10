use anchor_lang::prelude::*;
use anchor_spl::token::{mint_to, transfer, Mint, MintTo, Token, TokenAccount, Transfer};

use crate::{error::AmmError, math::integer_sqrt, state::Pool};

pub(crate) fn handler(ctx: Context<Deposit>, amount_a: u64, amount_b: u64, min_lp: u64) -> Result<()> {
    require!(amount_a > 0 && amount_b > 0, AmmError::InvalidAmounts);

    let a_reserve = ctx.accounts.vault_a.amount;
    let b_reserve = ctx.accounts.vault_b.amount;
    let lp_supply = ctx.accounts.pool.lp_supply;

    let lp_out = if lp_supply == 0 {
        // First deposit (pool was initialized without initial liquidity): geometric mean.
        integer_sqrt(amount_a)
            .checked_mul(integer_sqrt(amount_b))
            .ok_or(AmmError::Overflow)?
    } else {
        // Proportional — u128 intermediate avoids overflow for large token amounts.
        let lp_a = (amount_a as u128)
            .checked_mul(lp_supply as u128)
            .ok_or(AmmError::Overflow)?
            .checked_div(a_reserve as u128)
            .ok_or(AmmError::Overflow)? as u64;

        let lp_b = (amount_b as u128)
            .checked_mul(lp_supply as u128)
            .ok_or(AmmError::Overflow)?
            .checked_div(b_reserve as u128)
            .ok_or(AmmError::Overflow)? as u64;

        lp_a.min(lp_b)
    };

    require!(lp_out >= min_lp, AmmError::SlippageExceeded);
    require!(lp_out > 0, AmmError::ZeroLp);

    transfer(
        CpiContext::new(
            Token::id(),
            Transfer {
                from: ctx.accounts.depositor_a.to_account_info(),
                to: ctx.accounts.vault_a.to_account_info(),
                authority: ctx.accounts.depositor.to_account_info(),
            },
        ),
        amount_a,
    )?;

    transfer(
        CpiContext::new(
            Token::id(),
            Transfer {
                from: ctx.accounts.depositor_b.to_account_info(),
                to: ctx.accounts.vault_b.to_account_info(),
                authority: ctx.accounts.depositor.to_account_info(),
            },
        ),
        amount_b,
    )?;

    let pool = &ctx.accounts.pool;
    let mint_a = pool.token_a_mint;
    let mint_b = pool.token_b_mint;
    let bump = pool.bump;
    let pool_seeds: &[&[u8]] = &[b"pool", mint_a.as_ref(), mint_b.as_ref(), &[bump]];

    mint_to(
        CpiContext::new_with_signer(
            Token::id(),
            MintTo {
                mint: ctx.accounts.lp_mint.to_account_info(),
                to: ctx.accounts.depositor_lp.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[pool_seeds],
        ),
        lp_out,
    )?;

    ctx.accounts.pool.lp_supply = lp_supply
        .checked_add(lp_out)
        .ok_or(AmmError::Overflow)?;

    Ok(())
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,

    #[account(
        mut,
        seeds = [b"pool", pool.token_a_mint.as_ref(), pool.token_b_mint.as_ref()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        mut,
        seeds = [b"vault_a", pool.key().as_ref()],
        bump = pool.vault_a_bump,
        token::mint = pool.token_a_mint,
        token::authority = pool,
    )]
    pub vault_a: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"vault_b", pool.key().as_ref()],
        bump = pool.vault_b_bump,
        token::mint = pool.token_b_mint,
        token::authority = pool,
    )]
    pub vault_b: Box<Account<'info, TokenAccount>>,

    #[account(
        mut,
        seeds = [b"lp_mint", pool.key().as_ref()],
        bump = pool.lp_mint_bump,
    )]
    pub lp_mint: Box<Account<'info, Mint>>,

    #[account(mut, token::mint = pool.token_a_mint, token::authority = depositor)]
    pub depositor_a: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::mint = pool.token_b_mint, token::authority = depositor)]
    pub depositor_b: Box<Account<'info, TokenAccount>>,

    #[account(
        init_if_needed,
        payer = depositor,
        token::mint = lp_mint,
        token::authority = depositor,
        seeds = [b"lp_account", lp_mint.key().as_ref(), depositor.key().as_ref()],
        bump,
    )]
    pub depositor_lp: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
    pub system_program: Program<'info, System>,
}
