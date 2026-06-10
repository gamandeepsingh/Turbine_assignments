use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token_interface::{self, Mint, TokenAccount, TokenInterface, TransferChecked},
};
use mpl_core::{
    instructions::{RemovePluginV1CpiBuilder, TransferV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{FreezeDelegate, Plugin, PluginType},
    ID as MPL_CORE_ID,
};

use crate::{
    error::MarketplaceError,
    state::{Listing, Marketplace},
};

pub(crate) fn handler(ctx: Context<BuyWithToken>) -> Result<()> {
    let mp = &ctx.accounts.marketplace;
    let listing = &ctx.accounts.listing;

    // Verify the buyer is paying with the correct mint.
    require!(
        listing.payment_mint == ctx.accounts.payment_mint.key(),
        MarketplaceError::WrongPaymentMint
    );

    let fee_amount = (listing.price as u128)
        .checked_mul(mp.fee as u128)
        .ok_or(MarketplaceError::Overflow)?
        .checked_div(10_000)
        .ok_or(MarketplaceError::Overflow)? as u64;
    let seller_amount = listing
        .price
        .checked_sub(fee_amount)
        .ok_or(MarketplaceError::Overflow)?;

    let decimals = ctx.accounts.payment_mint.decimals;

    // 1. Transfer tokens: buyer → seller ATA.
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.buyer_ata.to_account_info(),
                mint: ctx.accounts.payment_mint.to_account_info(),
                to: ctx.accounts.seller_ata.to_account_info(),
                authority: ctx.accounts.buyer.to_account_info(),
            },
        ),
        seller_amount,
        decimals,
    )?;

    // 2. Transfer tokens: buyer → treasury ATA.
    token_interface::transfer_checked(
        CpiContext::new(
            ctx.accounts.token_program.key(),
            TransferChecked {
                from: ctx.accounts.buyer_ata.to_account_info(),
                mint: ctx.accounts.payment_mint.to_account_info(),
                to: ctx.accounts.treasury_ata.to_account_info(),
                authority: ctx.accounts.buyer.to_account_info(),
            },
        ),
        fee_amount,
        decimals,
    )?;

    let mp_seeds: &[&[u8]] = &[b"marketplace", mp.name.as_bytes(), &[mp.bump]];

    // 3. Unfreeze NFT (marketplace PDA signs).
    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.buyer)
        .authority(Some(&ctx.accounts.marketplace.to_account_info()))
        .system_program(&ctx.accounts.system_program)
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
        .invoke_signed(&[mp_seeds])?;

    // 4. Transfer NFT to buyer using TransferDelegate (marketplace PDA signs).
    TransferV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.buyer)
        .authority(Some(&ctx.accounts.marketplace.to_account_info()))
        .new_owner(&ctx.accounts.buyer)
        .system_program(Some(&ctx.accounts.system_program))
        .invoke_signed(&[mp_seeds])?;

    // 5. Remove FreezeDelegate — new owner (buyer) signs.
    RemovePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.buyer)
        .authority(Some(&ctx.accounts.buyer))
        .system_program(&ctx.accounts.system_program)
        .plugin_type(PluginType::FreezeDelegate)
        .invoke()?;

    // 6. Remove TransferDelegate — new owner (buyer) signs.
    RemovePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.buyer)
        .authority(Some(&ctx.accounts.buyer))
        .system_program(&ctx.accounts.system_program)
        .plugin_type(PluginType::TransferDelegate)
        .invoke()?;

    Ok(())
}

#[derive(Accounts)]
pub struct BuyWithToken<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    /// CHECK: seller receives tokens; identity confirmed via listing.seller
    #[account(mut, address = listing.seller)]
    pub seller: UncheckedAccount<'info>,

    /// CHECK: Validated by MPL Core CPI
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,

    /// CHECK: Validated by MPL Core CPI
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,

    #[account(seeds = [b"marketplace", marketplace.name.as_bytes()], bump = marketplace.bump)]
    pub marketplace: Account<'info, Marketplace>,

    /// Treasury PDA that receives fee tokens.
    /// CHECK: treasury PDA — token fees land in treasury_ata
    #[account(
        seeds = [b"treasury", marketplace.key().as_ref()],
        bump = marketplace.treasury_bump,
    )]
    pub treasury: UncheckedAccount<'info>,

    pub payment_mint: InterfaceAccount<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = payment_mint,
        associated_token::authority = buyer,
        associated_token::token_program = token_program,
    )]
    pub buyer_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = buyer,
        associated_token::mint = payment_mint,
        associated_token::authority = seller,
        associated_token::token_program = token_program,
    )]
    pub seller_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = buyer,
        associated_token::mint = payment_mint,
        associated_token::authority = treasury,
        associated_token::token_program = token_program,
    )]
    pub treasury_ata: InterfaceAccount<'info, TokenAccount>,

    #[account(
        mut,
        close = seller,
        seeds = [b"listing", marketplace.key().as_ref(), asset.key().as_ref()],
        bump = listing.bump,
    )]
    pub listing: Account<'info, Listing>,

    /// CHECK: Verified by address constraint
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub token_program: Interface<'info, TokenInterface>,
    pub associated_token_program: Program<'info, AssociatedToken>,
    pub system_program: Program<'info, System>,
}
