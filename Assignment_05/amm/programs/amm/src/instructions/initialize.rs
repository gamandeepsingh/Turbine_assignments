use anchor_lang::prelude::*;
use anchor_spl::token::{mint_to, transfer, Mint, MintTo, Token, TokenAccount, Transfer};

use crate::{error::AmmError, math::integer_sqrt, state::Pool};

pub(crate) fn handler(ctx: Context<Initialize>, initial_a: u64, initial_b: u64) -> Result<()> {
    require!(initial_a > 0 && initial_b > 0, AmmError::InvalidAmounts);

    let pool = &mut ctx.accounts.pool;
    pool.token_a_mint = ctx.accounts.token_a_mint.key();
    pool.token_b_mint = ctx.accounts.token_b_mint.key();
    pool.lp_mint = ctx.accounts.lp_mint.key();
    pool.bump = ctx.bumps.pool;
    pool.lp_mint_bump = ctx.bumps.lp_mint;
    pool.vault_a_bump = ctx.bumps.vault_a;
    pool.vault_b_bump = ctx.bumps.vault_b;

    transfer(
        CpiContext::new(
            Token::id(),
            Transfer {
                from: ctx.accounts.depositor_a.to_account_info(),
                to: ctx.accounts.vault_a.to_account_info(),
                authority: ctx.accounts.depositor.to_account_info(),
            },
        ),
        initial_a,
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
        initial_b,
    )?;

    // LP = √initial_a × √initial_b — pure u64, no compiler-rt u128 helpers.
    let lp_amount = integer_sqrt(initial_a)
        .checked_mul(integer_sqrt(initial_b))
        .ok_or(AmmError::Overflow)?;
    require!(lp_amount > 0, AmmError::ZeroLp);

    let mint_a = ctx.accounts.token_a_mint.key();
    let mint_b = ctx.accounts.token_b_mint.key();
    let bump = ctx.bumps.pool;
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
        lp_amount,
    )?;

    ctx.accounts.pool.lp_supply = lp_amount;
    Ok(())
}

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub depositor: Signer<'info>,

    pub token_a_mint: Box<Account<'info, Mint>>,
    pub token_b_mint: Box<Account<'info, Mint>>,

    /// Pool state PDA — [b"pool", mint_a, mint_b]
    #[account(
        init,
        payer = depositor,
        space = 8 + Pool::INIT_SPACE,
        seeds = [b"pool", token_a_mint.key().as_ref(), token_b_mint.key().as_ref()],
        bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    #[account(
        init,
        payer = depositor,
        token::mint = token_a_mint,
        token::authority = pool,
        seeds = [b"vault_a", pool.key().as_ref()],
        bump,
    )]
    pub vault_a: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = depositor,
        token::mint = token_b_mint,
        token::authority = pool,
        seeds = [b"vault_b", pool.key().as_ref()],
        bump,
    )]
    pub vault_b: Box<Account<'info, TokenAccount>>,

    #[account(
        init,
        payer = depositor,
        mint::decimals = 6,
        mint::authority = pool,
        seeds = [b"lp_mint", pool.key().as_ref()],
        bump,
    )]
    pub lp_mint: Box<Account<'info, Mint>>,

    #[account(mut, token::mint = token_a_mint, token::authority = depositor)]
    pub depositor_a: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::mint = token_b_mint, token::authority = depositor)]
    pub depositor_b: Box<Account<'info, TokenAccount>>,

    /// LP token account — PDA [b"lp_account", lp_mint, depositor], no ATA needed.
    #[account(
        init,
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
