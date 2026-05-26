use anchor_lang::prelude::*;
use anchor_lang::system_program::{transfer, Transfer};

declare_id!("2zbDjDG8bwssjgr9CT8ensjwmTBYnR48XUXahfWZyQBK");

// ── State ─────────────────────────────────────────────────────────────────────

#[account]
#[derive(InitSpace)]
pub struct VaultState {
    /// Bump for the vault SOL-holding PDA (seeds: ["vault", vault_state])
    pub vault_bump: u8,
    /// Bump for this state PDA (seeds: ["state", user])
    pub state_bump: u8,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[error_code]
pub enum VaultError {
    #[msg("Amount must be greater than zero")]
    ZeroAmount,
    #[msg("Insufficient funds in the vault")]
    InsufficientFunds,
}

// ── Program ───────────────────────────────────────────────────────────────────

#[program]
pub mod anchor_vault {
    use super::*;

    /// Create a new vault for the signer.
    pub fn initialize(ctx: Context<Initialize>) -> Result<()> {
        ctx.accounts.vault_state.state_bump = ctx.bumps.vault_state;
        ctx.accounts.vault_state.vault_bump  = ctx.bumps.vault;
        Ok(())
    }

    /// Deposit `amount` lamports into the vault.
    pub fn deposit(ctx: Context<Deposit>, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::ZeroAmount);
        transfer(
            CpiContext::new(
                System::id(),
                Transfer {
                    from: ctx.accounts.user.to_account_info(),
                    to:   ctx.accounts.vault.to_account_info(),
                },
            ),
            amount,
        )
    }

    /// Withdraw `amount` lamports from the vault back to the signer.
    pub fn withdraw(ctx: Context<Withdraw>, amount: u64) -> Result<()> {
        require!(amount > 0, VaultError::ZeroAmount);
        require!(
            ctx.accounts.vault.lamports() >= amount,
            VaultError::InsufficientFunds
        );

        let vault_state_key = ctx.accounts.vault_state.key();
        let seeds = &[
            b"vault" as &[u8],
            vault_state_key.as_ref(),
            &[ctx.accounts.vault_state.vault_bump],
        ];
        transfer(
            CpiContext::new_with_signer(
                System::id(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to:   ctx.accounts.user.to_account_info(),
                },
                &[seeds],
            ),
            amount,
        )
    }

    /// Close the vault and return all lamports to the signer.
    pub fn close(ctx: Context<Close>) -> Result<()> {
        let lamports = ctx.accounts.vault.lamports();
        if lamports == 0 {
            return Ok(());
        }
        let vault_state_key = ctx.accounts.vault_state.key();
        let seeds = &[
            b"vault" as &[u8],
            vault_state_key.as_ref(),
            &[ctx.accounts.vault_state.vault_bump],
        ];
        transfer(
            CpiContext::new_with_signer(
                System::id(),
                Transfer {
                    from: ctx.accounts.vault.to_account_info(),
                    to:   ctx.accounts.user.to_account_info(),
                },
                &[seeds],
            ),
            lamports,
        )
    }
}

// ── Account contexts ──────────────────────────────────────────────────────────

#[derive(Accounts)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        init,
        payer = user,
        space = 8 + VaultState::INIT_SPACE,
        seeds = [b"state", user.key().as_ref()],
        bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    /// CHECK: System-owned lamport-holding PDA — validated by seeds.
    #[account(
        seeds = [b"vault", vault_state.key().as_ref()],
        bump,
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Deposit<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"state", user.key().as_ref()],
        bump = vault_state.state_bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"vault", vault_state.key().as_ref()],
        bump = vault_state.vault_bump,
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Withdraw<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        seeds = [b"state", user.key().as_ref()],
        bump = vault_state.state_bump,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"vault", vault_state.key().as_ref()],
        bump = vault_state.vault_bump,
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}

#[derive(Accounts)]
pub struct Close<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    #[account(
        mut,
        seeds = [b"state", user.key().as_ref()],
        bump = vault_state.state_bump,
        close = user,
    )]
    pub vault_state: Account<'info, VaultState>,

    #[account(
        mut,
        seeds = [b"vault", vault_state.key().as_ref()],
        bump = vault_state.vault_bump,
    )]
    pub vault: SystemAccount<'info>,

    pub system_program: Program<'info, System>,
}
