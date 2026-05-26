use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::get_associated_token_address,
    token::{
        close_account, transfer_checked, CloseAccount, Mint, Token, TokenAccount,
        TransferChecked,
    },
};

declare_id!("BywbcryXckDcAEAP6DTjB5AYNjepKZHeW4RtfgR9wK92");

// ── State ─────────────────────────────────────────────────────────────────────

#[account]
#[derive(InitSpace)]
pub struct Escrow {
    pub seed:    u64,
    pub maker:   Pubkey,
    pub mint_a:  Pubkey,
    pub mint_b:  Pubkey,
    pub receive: u64,
    pub bump:    u8,
}

// ── Errors ────────────────────────────────────────────────────────────────────

#[error_code]
pub enum EscrowError {
    #[msg("Deposit amount must be greater than zero")]
    ZeroDeposit,
    #[msg("Receive amount must be greater than zero")]
    ZeroReceive,
    #[msg("Maker does not match the escrow")]
    InvalidMaker,
    #[msg("Mint does not match the escrow")]
    InvalidMint,
    #[msg("Seed does not match the escrow")]
    InvalidSeed,
    #[msg("Token account owner mismatch")]
    InvalidTokenAccount,
}

// ── Program ───────────────────────────────────────────────────────────────────

#[program]
pub mod anchor_escrow {
    use super::*;

    /// Create escrow: maker deposits Token A and records the desired Token B amount.
    pub fn make(ctx: Context<Make>, seed: u64, deposit: u64, receive: u64) -> Result<()> {
        require!(deposit > 0, EscrowError::ZeroDeposit);
        require!(receive > 0, EscrowError::ZeroReceive);

        ctx.accounts.escrow.set_inner(Escrow {
            seed,
            maker:   ctx.accounts.maker.key(),
            mint_a:  ctx.accounts.mint_a.key(),
            mint_b:  ctx.accounts.mint_b.key(),
            receive,
            bump:    ctx.bumps.escrow,
        });

        transfer_checked(
            CpiContext::new(
                Token::id(),
                TransferChecked {
                    from:      ctx.accounts.maker_ata_a.to_account_info(),
                    mint:      ctx.accounts.mint_a.to_account_info(),
                    to:        ctx.accounts.vault.to_account_info(),
                    authority: ctx.accounts.maker.to_account_info(),
                },
            ),
            deposit,
            ctx.accounts.mint_a.decimals,
        )
    }

    /// Take: taker sends Token B to maker, receives Token A from vault.
    /// Seed is passed explicitly to avoid SBF cross-frame stack access violations.
    pub fn take(ctx: Context<Take>, seed: u64) -> Result<()> {
        let escrow = &ctx.accounts.escrow;

        // Validate seed matches stored escrow state.
        require_eq!(escrow.seed, seed, EscrowError::InvalidSeed);
        // Validate maker.
        require_keys_eq!(escrow.maker, ctx.accounts.maker.key(), EscrowError::InvalidMaker);

        // Validate token account addresses via canonical ATA derivation.
        let expected_taker_ata_a = get_associated_token_address(
            &ctx.accounts.taker.key(),
            &escrow.mint_a,
        );
        let expected_maker_ata_b = get_associated_token_address(
            &ctx.accounts.maker.key(),
            &escrow.mint_b,
        );
        require_keys_eq!(ctx.accounts.taker_ata_a.key(), expected_taker_ata_a, EscrowError::InvalidTokenAccount);
        require_keys_eq!(ctx.accounts.maker_ata_b.key(), expected_maker_ata_b, EscrowError::InvalidTokenAccount);

        // Signer seeds for escrow PDA.
        let seed_bytes = seed.to_le_bytes();
        let bump_bytes = [escrow.bump];
        let maker_key  = ctx.accounts.maker.key();
        let escrow_seeds: &[&[u8]] = &[b"escrow", maker_key.as_ref(), &seed_bytes, &bump_bytes];
        let signer_seeds = &[escrow_seeds];

        // Transfer Token B from taker → maker.
        transfer_checked(
            CpiContext::new(
                Token::id(),
                TransferChecked {
                    from:      ctx.accounts.taker_ata_b.to_account_info(),
                    mint:      ctx.accounts.mint_b.to_account_info(),
                    to:        ctx.accounts.maker_ata_b.to_account_info(),
                    authority: ctx.accounts.taker.to_account_info(),
                },
            ),
            escrow.receive,
            ctx.accounts.mint_b.decimals,
        )?;

        // Transfer all Token A from vault → taker.
        let vault_amount = ctx.accounts.vault.amount;
        transfer_checked(
            CpiContext::new_with_signer(
                Token::id(),
                TransferChecked {
                    from:      ctx.accounts.vault.to_account_info(),
                    mint:      ctx.accounts.mint_a.to_account_info(),
                    to:        ctx.accounts.taker_ata_a.to_account_info(),
                    authority: ctx.accounts.escrow.to_account_info(),
                },
                signer_seeds,
            ),
            vault_amount,
            ctx.accounts.mint_a.decimals,
        )?;

        // Close vault — rent to maker.
        close_account(CpiContext::new_with_signer(
            Token::id(),
            CloseAccount {
                account:     ctx.accounts.vault.to_account_info(),
                destination: ctx.accounts.maker.to_account_info(),
                authority:   ctx.accounts.escrow.to_account_info(),
            },
            signer_seeds,
        ))
    }

    /// Refund: maker cancels, Token A returned from vault.
    pub fn refund(ctx: Context<Refund>, seed: u64) -> Result<()> {
        let escrow = &ctx.accounts.escrow;
        require_eq!(escrow.seed, seed, EscrowError::InvalidSeed);

        let seed_bytes = seed.to_le_bytes();
        let bump_bytes = [escrow.bump];
        let maker_key  = ctx.accounts.maker.key();
        let escrow_seeds: &[&[u8]] = &[b"escrow", maker_key.as_ref(), &seed_bytes, &bump_bytes];
        let signer_seeds = &[escrow_seeds];

        let vault_amount = ctx.accounts.vault.amount;
        transfer_checked(
            CpiContext::new_with_signer(
                Token::id(),
                TransferChecked {
                    from:      ctx.accounts.vault.to_account_info(),
                    mint:      ctx.accounts.mint_a.to_account_info(),
                    to:        ctx.accounts.maker_ata_a.to_account_info(),
                    authority: ctx.accounts.escrow.to_account_info(),
                },
                signer_seeds,
            ),
            vault_amount,
            ctx.accounts.mint_a.decimals,
        )?;

        close_account(CpiContext::new_with_signer(
            Token::id(),
            CloseAccount {
                account:     ctx.accounts.vault.to_account_info(),
                destination: ctx.accounts.maker.to_account_info(),
                authority:   ctx.accounts.escrow.to_account_info(),
            },
            signer_seeds,
        ))
    }
}

// ── Account contexts ──────────────────────────────────────────────────────────

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Make<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,

    pub mint_a: Account<'info, Mint>,
    pub mint_b: Account<'info, Mint>,

    #[account(
        mut,
        associated_token::mint      = mint_a,
        associated_token::authority = maker,
    )]
    pub maker_ata_a: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = maker,
        space = 8 + Escrow::INIT_SPACE,
        seeds = [b"escrow", maker.key().as_ref(), seed.to_le_bytes().as_ref()],
        bump,
    )]
    pub escrow: Account<'info, Escrow>,

    #[account(
        init,
        payer = maker,
        associated_token::mint      = mint_a,
        associated_token::authority = escrow,
    )]
    pub vault: Account<'info, TokenAccount>,

    pub system_program:           Program<'info, System>,
    pub token_program:            Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
}

/// Simplified Take — validates escrow seeds only; ATA key checks done in handler.
#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Take<'info> {
    #[account(mut)]
    pub taker: Signer<'info>,

    /// CHECK: key validated against escrow.maker in the handler.
    #[account(mut)]
    pub maker: UncheckedAccount<'info>,

    pub mint_a: Account<'info, Mint>,
    pub mint_b: Account<'info, Mint>,

    /// Taker's ATA for mint_b (pays).
    #[account(mut)]
    pub taker_ata_b: Account<'info, TokenAccount>,

    /// Taker's ATA for mint_a (receives) — key validated in handler.
    #[account(mut)]
    pub taker_ata_a: Account<'info, TokenAccount>,

    /// Maker's ATA for mint_b (receives) — key validated in handler.
    #[account(mut)]
    pub maker_ata_b: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [b"escrow", maker.key().as_ref(), seed.to_le_bytes().as_ref()],
        bump  = escrow.bump,
        close = maker,
    )]
    pub escrow: Account<'info, Escrow>,

    /// Vault: ATA of escrow for mint_a.
    #[account(
        mut,
        associated_token::mint      = mint_a,
        associated_token::authority = escrow,
    )]
    pub vault: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program:  Program<'info, Token>,
}

/// Simplified Refund — only validates escrow seeds; maker is signer so self-authorizes.
#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Refund<'info> {
    #[account(mut)]
    pub maker: Signer<'info>,

    pub mint_a: Account<'info, Mint>,

    #[account(
        mut,
        associated_token::mint      = mint_a,
        associated_token::authority = maker,
    )]
    pub maker_ata_a: Account<'info, TokenAccount>,

    #[account(
        mut,
        has_one = maker  @ EscrowError::InvalidMaker,
        has_one = mint_a @ EscrowError::InvalidMint,
        seeds = [b"escrow", maker.key().as_ref(), seed.to_le_bytes().as_ref()],
        bump  = escrow.bump,
        close = maker,
    )]
    pub escrow: Account<'info, Escrow>,

    #[account(
        mut,
        associated_token::mint      = mint_a,
        associated_token::authority = escrow,
    )]
    pub vault: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program:  Program<'info, Token>,
}
