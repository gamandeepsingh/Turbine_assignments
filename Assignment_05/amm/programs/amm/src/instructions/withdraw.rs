use anchor_lang::{prelude::*, Discriminator};
use solana_instructions_sysvar::{load_current_index_checked, load_instruction_at_checked, ID as INSTRUCTIONS_ID};
use anchor_spl::token::{transfer, Token, TokenAccount, Transfer};

use crate::{error::AmmError, state::Pool};

/// Pays out token A and B proportional to the LP amount burned in the
/// immediately preceding `burn_lp` instruction.
///
/// **Instruction introspection** links these two instructions: this handler
/// loads the `sysvar::instructions` account, walks back one slot to the
/// `burn_lp` ix, and verifies:
///   1. It belongs to this program.
///   2. Its discriminator matches `burn_lp`.
///   3. The signing user (accounts[0]) matches the current caller.
///   4. The LP mint (accounts[2]) matches this pool's LP mint.
pub(crate) fn handler(ctx: Context<Withdraw>, min_a: u64, min_b: u64) -> Result<()> {
    // ── 1. Locate the preceding burn_lp instruction ───────────────────────────
    let ix_info = ctx.accounts.instructions.to_account_info();
    let current_idx = load_current_index_checked(&ix_info)? as usize;
    require!(current_idx >= 1, AmmError::MissingBurnInstruction);

    let burn_ix = load_instruction_at_checked(current_idx - 1, &ix_info)
        .map_err(|_| AmmError::MissingBurnInstruction)?;

    // ── 2. Verify program ID ──────────────────────────────────────────────────
    require!(burn_ix.program_id == crate::ID, AmmError::MissingBurnInstruction);

    // ── 3. Verify discriminator = burn_lp ────────────────────────────────────
    let expected_disc = crate::instruction::BurnLp::DISCRIMINATOR;
    require!(
        burn_ix.data.len() >= 8 && burn_ix.data[..8] == expected_disc[..],
        AmmError::MissingBurnInstruction
    );

    // ── 4. Extract lp_amount (bytes 8..16, little-endian u64) ────────────────
    let lp_amount = u64::from_le_bytes(
        burn_ix.data[8..16]
            .try_into()
            .map_err(|_| AmmError::MissingBurnInstruction)?,
    );
    require!(lp_amount > 0, AmmError::ZeroLp);

    // ── 5. Verify same user (accounts[0]) ────────────────────────────────────
    require!(
        !burn_ix.accounts.is_empty() && burn_ix.accounts[0].pubkey == ctx.accounts.user.key(),
        AmmError::WrongUser
    );

    // ── 6. Verify same LP mint (accounts[2]) → same pool ─────────────────────
    require!(
        burn_ix.accounts.len() > 2
            && burn_ix.accounts[2].pubkey == ctx.accounts.pool.lp_mint,
        AmmError::WrongPool
    );

    // ── 7. Proportional payout ────────────────────────────────────────────────
    let a_reserve = ctx.accounts.vault_a.amount;
    let b_reserve = ctx.accounts.vault_b.amount;
    let lp_supply = ctx.accounts.pool.lp_supply;
    require!(lp_supply > 0, AmmError::InsufficientLiquidity);

    let a_out = (lp_amount as u128)
        .checked_mul(a_reserve as u128)
        .ok_or(AmmError::Overflow)?
        .checked_div(lp_supply as u128)
        .ok_or(AmmError::Overflow)? as u64;

    let b_out = (lp_amount as u128)
        .checked_mul(b_reserve as u128)
        .ok_or(AmmError::Overflow)?
        .checked_div(lp_supply as u128)
        .ok_or(AmmError::Overflow)? as u64;

    require!(a_out >= min_a && b_out >= min_b, AmmError::SlippageExceeded);
    require!(a_out > 0 && b_out > 0, AmmError::InsufficientLiquidity);

    // ── 8. Transfer from vaults (pool PDA signs) ─────────────────────────────
    let pool = &ctx.accounts.pool;
    let mint_a = pool.token_a_mint;
    let mint_b = pool.token_b_mint;
    let bump = pool.bump;
    let pool_seeds: &[&[u8]] = &[b"pool", mint_a.as_ref(), mint_b.as_ref(), &[bump]];

    transfer(
        CpiContext::new_with_signer(
            Token::id(),
            Transfer {
                from: ctx.accounts.vault_a.to_account_info(),
                to: ctx.accounts.user_a.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[pool_seeds],
        ),
        a_out,
    )?;

    transfer(
        CpiContext::new_with_signer(
            Token::id(),
            Transfer {
                from: ctx.accounts.vault_b.to_account_info(),
                to: ctx.accounts.user_b.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[pool_seeds],
        ),
        b_out,
    )?;

    // ── 9. Update pool LP supply ──────────────────────────────────────────────
    ctx.accounts.pool.lp_supply = lp_supply
        .checked_sub(lp_amount)
        .ok_or(AmmError::Overflow)?;

    Ok(())
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    pub user: Signer<'info>,

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

    #[account(mut, token::mint = pool.token_a_mint)]
    pub user_a: Box<Account<'info, TokenAccount>>,

    #[account(mut, token::mint = pool.token_b_mint)]
    pub user_b: Box<Account<'info, TokenAccount>>,

    /// CHECK: Instructions sysvar — verified by address constraint.
    #[account(address = INSTRUCTIONS_ID)]
    pub instructions: UncheckedAccount<'info>,

    pub token_program: Program<'info, Token>,
}
