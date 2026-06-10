use anchor_lang::prelude::*;
use anchor_spl::token::{burn, Burn, Mint, Token, TokenAccount};

use crate::state::Pool;

/// Burns `lp_amount` LP tokens from the caller's LP account.
///
/// This instruction MUST appear immediately before `withdraw` in the same
/// transaction.  `withdraw` uses the instructions sysvar to look back at this
/// instruction and derive the withdrawal amount via introspection.
///
/// Pool.lp_supply is intentionally NOT updated here; `withdraw` does that.
pub(crate) fn handler(ctx: Context<BurnLp>, lp_amount: u64) -> Result<()> {
    burn(
        CpiContext::new(
            Token::id(),
            Burn {
                mint: ctx.accounts.lp_mint.to_account_info(),
                from: ctx.accounts.user_lp.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        lp_amount,
    )?;
    Ok(())
}

#[derive(Accounts)]
pub struct BurnLp<'info> {
    /// [0] Signer — verified by `withdraw` via the instructions sysvar.
    pub user: Signer<'info>,

    /// [1] User's LP token account. PDA: [b"lp_account", lp_mint, user]
    #[account(
        mut,
        seeds = [b"lp_account", lp_mint.key().as_ref(), user.key().as_ref()],
        bump,
        token::mint = lp_mint,
        token::authority = user,
    )]
    pub user_lp: Box<Account<'info, TokenAccount>>,

    /// [2] LP mint — used by `withdraw` to confirm this burn was for the right pool.
    #[account(
        mut,
        seeds = [b"lp_mint", pool.key().as_ref()],
        bump = pool.lp_mint_bump,
    )]
    pub lp_mint: Box<Account<'info, Mint>>,

    /// [3] Pool — needed to derive lp_mint seed.
    #[account(
        seeds = [b"pool", pool.token_a_mint.as_ref(), pool.token_b_mint.as_ref()],
        bump = pool.bump,
    )]
    pub pool: Box<Account<'info, Pool>>,

    pub token_program: Program<'info, Token>,
}
