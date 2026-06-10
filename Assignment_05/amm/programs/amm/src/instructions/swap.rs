use anchor_lang::prelude::*;
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

use crate::{error::AmmError, state::Pool};

/// Constant-product swap with 0.3% fee.
/// `a_to_b = true` means user sends token A, receives token B.
pub(crate) fn handler(
    ctx: Context<Swap>,
    amount_in: u64,
    min_out: u64,
    a_to_b: bool,
) -> Result<()> {
    require!(amount_in > 0, AmmError::InvalidAmounts);

    let (in_reserve, out_reserve) = if a_to_b {
        (ctx.accounts.vault_in.amount, ctx.accounts.vault_out.amount)
    } else {
        (ctx.accounts.vault_out.amount, ctx.accounts.vault_in.amount)
    };

    require!(out_reserve > 0, AmmError::InsufficientLiquidity);

    // amount_out = (amount_in × 997 × out_reserve) / (in_reserve × 1000 + amount_in × 997)
    let effective_in = (amount_in as u128).checked_mul(997).ok_or(AmmError::Overflow)?;
    let numerator = effective_in
        .checked_mul(out_reserve as u128)
        .ok_or(AmmError::Overflow)?;
    let denominator = (in_reserve as u128)
        .checked_mul(1_000)
        .ok_or(AmmError::Overflow)?
        .checked_add(effective_in)
        .ok_or(AmmError::Overflow)?;
    let amount_out = (numerator / denominator) as u64;

    require!(amount_out >= min_out, AmmError::SlippageExceeded);
    require!(amount_out > 0, AmmError::InsufficientLiquidity);

    let pool = &ctx.accounts.pool;
    let mint_a = pool.token_a_mint;
    let mint_b = pool.token_b_mint;
    let bump = pool.bump;
    let pool_seeds: &[&[u8]] = &[b"pool", mint_a.as_ref(), mint_b.as_ref(), &[bump]];

    transfer(
        CpiContext::new(
            Token::id(),
            Transfer {
                from: ctx.accounts.user_in.to_account_info(),
                to: ctx.accounts.vault_in.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        amount_in,
    )?;

    transfer(
        CpiContext::new_with_signer(
            Token::id(),
            Transfer {
                from: ctx.accounts.vault_out.to_account_info(),
                to: ctx.accounts.user_out.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[pool_seeds],
        ),
        amount_out,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [b"pool", pool.token_a_mint.as_ref(), pool.token_b_mint.as_ref()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    /// Vault user is depositing into.
    #[account(mut, token::authority = pool)]
    pub vault_in: Box<Account<'info, TokenAccount>>,

    /// Vault user is withdrawing from.
    #[account(mut, token::authority = pool)]
    pub vault_out: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::authority = user)]
    pub user_in: Box<Account<'info, TokenAccount>>,

    #[account(mut)]
    pub user_out: Box<Account<'info, TokenAccount>>,

    pub token_program: Program<'info, Token>,
}
