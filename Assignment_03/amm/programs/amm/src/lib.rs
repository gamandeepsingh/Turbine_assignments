use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::get_associated_token_address,
    token::{
        burn, mint_to, transfer_checked, Burn, Mint, MintTo, Token, TokenAccount,
        TransferChecked,
    },
};

declare_id!("4UC5N3xPNPMg5qBzrbkPosXXiWgxNowkp5vxBLetGy2B");

fn integer_sqrt(n: u128) -> u64 {
    if n == 0 { return 0; }
    let mut x = n;
    let mut y = (x + 1) >> 1;
    while y < x { x = y; y = (x + n / x) >> 1; }
    x as u64
}

#[account]
#[derive(InitSpace)]
pub struct Config {
    pub seed:        u64,
    pub mint_x:      Pubkey,
    pub mint_y:      Pubkey,
    pub fee:         u16,
    pub lp_bump:     u8,
    pub config_bump: u8,
}

#[error_code]
pub enum AmmError {
    #[msg("Fee must be 0–10000 bps")]
    InvalidFee,
    #[msg("Output below minimum — slippage")]
    SlippageExceeded,
    #[msg("Liquidity amount is zero")]
    ZeroLiquidity,
    #[msg("Amount in is zero")]
    ZeroAmount,
    #[msg("Pool reserves are empty")]
    EmptyPool,
    #[msg("Arithmetic overflow")]
    Overflow,
    #[msg("Invalid token account")]
    InvalidTokenAccount,
}

#[program]
pub mod amm {
    use super::*;

    pub fn initialize(ctx: Context<Initialize>, seed: u64, fee: u16) -> Result<()> {
        require!(fee <= 10_000, AmmError::InvalidFee);
        let cfg = &mut ctx.accounts.config;
        cfg.seed        = seed;
        cfg.mint_x      = ctx.accounts.mint_x.key();
        cfg.mint_y      = ctx.accounts.mint_y.key();
        cfg.fee         = fee;
        cfg.lp_bump     = ctx.bumps.lp_mint;
        cfg.config_bump = ctx.bumps.config;
        Ok(())
    }

    pub fn add_liquidity(
        ctx: Context<AddLiquidity>,
        seed: u64,
        amount_x: u64,
        amount_y: u64,
        min_lp: u64,
    ) -> Result<()> {
        require!(amount_x > 0 && amount_y > 0, AmmError::ZeroLiquidity);

        let expected_lp = Pubkey::find_program_address(
            &[b"lp", ctx.accounts.config.key().as_ref()], &crate::ID,
        ).0;
        require_keys_eq!(ctx.accounts.lp_mint.key(), expected_lp, AmmError::InvalidTokenAccount);

        let reserve_x  = ctx.accounts.vault_x.amount;
        let reserve_y  = ctx.accounts.vault_y.amount;
        let lp_supply  = ctx.accounts.lp_mint.supply;
        let config_key = ctx.accounts.config.key();

        let lp_amount: u64 = if lp_supply == 0 {
            integer_sqrt(
                (amount_x as u128).checked_mul(amount_y as u128).ok_or(AmmError::Overflow)?,
            )
        } else {
            let lp_x = (amount_x as u128)
                .checked_mul(lp_supply as u128).ok_or(AmmError::Overflow)?
                .checked_div(reserve_x as u128).ok_or(AmmError::Overflow)? as u64;
            let lp_y = (amount_y as u128)
                .checked_mul(lp_supply as u128).ok_or(AmmError::Overflow)?
                .checked_div(reserve_y as u128).ok_or(AmmError::Overflow)? as u64;
            lp_x.min(lp_y)
        };

        require!(lp_amount >= min_lp, AmmError::SlippageExceeded);
        require!(lp_amount > 0, AmmError::ZeroLiquidity);

        let seed_bytes = seed.to_le_bytes();
        let bump_byte  = [ctx.accounts.config.config_bump];
        let config_seeds: &[&[u8]] = &[b"config", &seed_bytes, &bump_byte];

        transfer_checked(
            CpiContext::new(
                Token::id(),
                TransferChecked {
                    from:      ctx.accounts.user_x.to_account_info(),
                    mint:      ctx.accounts.mint_x.to_account_info(),
                    to:        ctx.accounts.vault_x.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount_x,
            ctx.accounts.mint_x.decimals,
        )?;

        transfer_checked(
            CpiContext::new(
                Token::id(),
                TransferChecked {
                    from:      ctx.accounts.user_y.to_account_info(),
                    mint:      ctx.accounts.mint_y.to_account_info(),
                    to:        ctx.accounts.vault_y.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            amount_y,
            ctx.accounts.mint_y.decimals,
        )?;

        let lp_bump_byte = [ctx.accounts.config.lp_bump];
        let lp_seeds: &[&[u8]] = &[b"lp", config_key.as_ref(), &lp_bump_byte];

        mint_to(
            CpiContext::new_with_signer(
                Token::id(),
                MintTo {
                    mint:      ctx.accounts.lp_mint.to_account_info(),
                    to:        ctx.accounts.user_lp.to_account_info(),
                    authority: ctx.accounts.lp_mint.to_account_info(),
                },
                &[lp_seeds],
            ),
            lp_amount,
        )
    }

    pub fn remove_liquidity(
        ctx: Context<RemoveLiquidity>,
        seed: u64,
        lp_amount: u64,
        min_x: u64,
        min_y: u64,
    ) -> Result<()> {
        require!(lp_amount > 0, AmmError::ZeroLiquidity);

        let expected_lp = Pubkey::find_program_address(
            &[b"lp", ctx.accounts.config.key().as_ref()], &crate::ID,
        ).0;
        require_keys_eq!(ctx.accounts.lp_mint.key(), expected_lp, AmmError::InvalidTokenAccount);

        let reserve_x  = ctx.accounts.vault_x.amount;
        let reserve_y  = ctx.accounts.vault_y.amount;
        let lp_supply  = ctx.accounts.lp_mint.supply;

        require!(lp_supply > 0, AmmError::EmptyPool);

        let amount_x = (lp_amount as u128)
            .checked_mul(reserve_x as u128).ok_or(AmmError::Overflow)?
            .checked_div(lp_supply as u128).ok_or(AmmError::Overflow)? as u64;

        let amount_y = (lp_amount as u128)
            .checked_mul(reserve_y as u128).ok_or(AmmError::Overflow)?
            .checked_div(lp_supply as u128).ok_or(AmmError::Overflow)? as u64;

        require!(amount_x >= min_x, AmmError::SlippageExceeded);
        require!(amount_y >= min_y, AmmError::SlippageExceeded);

        burn(
            CpiContext::new(
                Token::id(),
                Burn {
                    mint:      ctx.accounts.lp_mint.to_account_info(),
                    from:      ctx.accounts.user_lp.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                },
            ),
            lp_amount,
        )?;

        let seed_bytes = seed.to_le_bytes();
        let bump_byte  = [ctx.accounts.config.config_bump];
        let config_seeds: &[&[u8]] = &[b"config", &seed_bytes, &bump_byte];
        let signer = &[config_seeds];

        transfer_checked(
            CpiContext::new_with_signer(
                Token::id(),
                TransferChecked {
                    from:      ctx.accounts.vault_x.to_account_info(),
                    mint:      ctx.accounts.mint_x.to_account_info(),
                    to:        ctx.accounts.user_x.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                signer,
            ),
            amount_x,
            ctx.accounts.mint_x.decimals,
        )?;

        transfer_checked(
            CpiContext::new_with_signer(
                Token::id(),
                TransferChecked {
                    from:      ctx.accounts.vault_y.to_account_info(),
                    mint:      ctx.accounts.mint_y.to_account_info(),
                    to:        ctx.accounts.user_y.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                },
                signer,
            ),
            amount_y,
            ctx.accounts.mint_y.decimals,
        )
    }

    pub fn swap(
        ctx: Context<Swap>,
        seed: u64,
        is_x_to_y: bool,
        amount_in: u64,
        min_amount_out: u64,
    ) -> Result<()> {
        require!(amount_in > 0, AmmError::ZeroAmount);

        let fee       = ctx.accounts.config.fee as u128;
        let fee_denom = 10_000u128;
        let reserve_x = ctx.accounts.vault_x.amount as u128;
        let reserve_y = ctx.accounts.vault_y.amount as u128;

        require!(reserve_x > 0 && reserve_y > 0, AmmError::EmptyPool);

        let (reserve_in, reserve_out) = if is_x_to_y { (reserve_x, reserve_y) } else { (reserve_y, reserve_x) };

        let amount_in_with_fee = (amount_in as u128)
            .checked_mul(fee_denom - fee).ok_or(AmmError::Overflow)?;
        let numerator   = amount_in_with_fee.checked_mul(reserve_out).ok_or(AmmError::Overflow)?;
        let denominator = reserve_in.checked_mul(fee_denom).ok_or(AmmError::Overflow)?
            .checked_add(amount_in_with_fee).ok_or(AmmError::Overflow)?;
        let amount_out = (numerator / denominator) as u64;

        require!(amount_out >= min_amount_out, AmmError::SlippageExceeded);
        require!(amount_out > 0, AmmError::ZeroAmount);

        let seed_bytes = seed.to_le_bytes();
        let bump_byte  = [ctx.accounts.config.config_bump];
        let config_seeds: &[&[u8]] = &[b"config", &seed_bytes, &bump_byte];
        let signer = &[config_seeds];

        if is_x_to_y {
            transfer_checked(
                CpiContext::new(Token::id(), TransferChecked {
                    from:      ctx.accounts.user_in.to_account_info(),
                    mint:      ctx.accounts.mint_x.to_account_info(),
                    to:        ctx.accounts.vault_x.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                }),
                amount_in, ctx.accounts.mint_x.decimals,
            )?;
            transfer_checked(
                CpiContext::new_with_signer(Token::id(), TransferChecked {
                    from:      ctx.accounts.vault_y.to_account_info(),
                    mint:      ctx.accounts.mint_y.to_account_info(),
                    to:        ctx.accounts.user_out.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                }, signer),
                amount_out, ctx.accounts.mint_y.decimals,
            )
        } else {
            transfer_checked(
                CpiContext::new(Token::id(), TransferChecked {
                    from:      ctx.accounts.user_in.to_account_info(),
                    mint:      ctx.accounts.mint_y.to_account_info(),
                    to:        ctx.accounts.vault_y.to_account_info(),
                    authority: ctx.accounts.user.to_account_info(),
                }),
                amount_in, ctx.accounts.mint_y.decimals,
            )?;
            transfer_checked(
                CpiContext::new_with_signer(Token::id(), TransferChecked {
                    from:      ctx.accounts.vault_x.to_account_info(),
                    mint:      ctx.accounts.mint_x.to_account_info(),
                    to:        ctx.accounts.user_out.to_account_info(),
                    authority: ctx.accounts.config.to_account_info(),
                }, signer),
                amount_out, ctx.accounts.mint_x.decimals,
            )
        }
    }
}

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Initialize<'info> {
    #[account(mut)]
    pub admin: Signer<'info>,

    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,

    #[account(
        init,
        payer = admin,
        space = 8 + Config::INIT_SPACE,
        seeds = [b"config", seed.to_le_bytes().as_ref()],
        bump,
    )]
    pub config: Account<'info, Config>,

    #[account(
        init,
        payer = admin,
        mint::decimals = 6,
        mint::authority = lp_mint,
        seeds = [b"lp", config.key().as_ref()],
        bump,
    )]
    pub lp_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = admin,
        associated_token::mint      = mint_x,
        associated_token::authority = config,
    )]
    pub vault_x: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = admin,
        associated_token::mint      = mint_y,
        associated_token::authority = config,
    )]
    pub vault_y: Account<'info, TokenAccount>,

    pub system_program:           Program<'info, System>,
    pub token_program:            Program<'info, Token>,
    pub associated_token_program: Program<'info, anchor_spl::associated_token::AssociatedToken>,
}

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct AddLiquidity<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,

    #[account(
        seeds = [b"config", seed.to_le_bytes().as_ref()],
        bump  = config.config_bump,
        has_one = mint_x,
        has_one = mint_y,
    )]
    pub config: Account<'info, Config>,

    #[account(mut)]
    pub lp_mint: Account<'info, Mint>,

    #[account(mut)]
    pub vault_x: Account<'info, TokenAccount>,

    #[account(mut)]
    pub vault_y: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_x: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_y: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_lp: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program:  Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct RemoveLiquidity<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,

    #[account(
        seeds = [b"config", seed.to_le_bytes().as_ref()],
        bump  = config.config_bump,
        has_one = mint_x,
        has_one = mint_y,
    )]
    pub config: Account<'info, Config>,

    #[account(mut)]
    pub lp_mint: Account<'info, Mint>,

    #[account(mut)]
    pub vault_x: Account<'info, TokenAccount>,

    #[account(mut)]
    pub vault_y: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_x: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_y: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_lp: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}

#[derive(Accounts)]
#[instruction(seed: u64)]
pub struct Swap<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    pub mint_x: Account<'info, Mint>,
    pub mint_y: Account<'info, Mint>,

    #[account(
        seeds = [b"config", seed.to_le_bytes().as_ref()],
        bump  = config.config_bump,
        has_one = mint_x,
        has_one = mint_y,
    )]
    pub config: Account<'info, Config>,

    #[account(mut)]
    pub vault_x: Account<'info, TokenAccount>,

    #[account(mut)]
    pub vault_y: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_in: Account<'info, TokenAccount>,

    #[account(mut)]
    pub user_out: Account<'info, TokenAccount>,

    pub token_program: Program<'info, Token>,
}
