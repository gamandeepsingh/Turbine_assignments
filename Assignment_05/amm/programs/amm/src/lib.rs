use anchor_lang::prelude::*;

pub mod error;
pub mod instructions;
pub mod math;
pub mod state;

use instructions::*;

declare_id!("6ZtrXUvY1n883iQ9Jq1mG8UH5fx2mt3tCNYVQZhTiJ6v");

#[program]
pub mod amm {
    use super::*;

    /// Seed the pool with initial liquidity and mint the first LP tokens.
    pub fn initialize(ctx: Context<Initialize>, initial_a: u64, initial_b: u64) -> Result<()> {
        initialize::handler(ctx, initial_a, initial_b)
    }

    /// Add liquidity proportionally; receive LP tokens.
    pub fn deposit(ctx: Context<Deposit>, amount_a: u64, amount_b: u64, min_lp: u64) -> Result<()> {
        deposit::handler(ctx, amount_a, amount_b, min_lp)
    }

    /// Swap token A ↔ token B via constant-product formula (0.3 % fee).
    pub fn swap(ctx: Context<Swap>, amount_in: u64, min_out: u64, a_to_b: bool) -> Result<()> {
        swap::handler(ctx, amount_in, min_out, a_to_b)
    }

    /// Burn LP tokens.  This instruction MUST immediately precede `withdraw`
    /// in the same transaction; `withdraw` reads its data via instruction
    /// introspection to know how much to pay out.
    pub fn burn_lp(ctx: Context<BurnLp>, lp_amount: u64) -> Result<()> {
        burn_lp::handler(ctx, lp_amount)
    }

    /// Pay out pool tokens proportional to the LP amount burned in the
    /// immediately preceding `burn_lp` instruction (verified via the
    /// sysvar::instructions sysvar).
    pub fn withdraw(ctx: Context<Withdraw>, min_a: u64, min_b: u64) -> Result<()> {
        withdraw::handler(ctx, min_a, min_b)
    }
}
