use anchor_lang::{prelude::*, system_program};
use mpl_core::{
    instructions::{RemovePluginV1CpiBuilder, TransferV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{FreezeDelegate, Plugin, PluginType},
    ID as MPL_CORE_ID,
};

use crate::{
    error::MarketplaceError,
    state::{Listing, Marketplace},
};

pub(crate) fn handler(ctx: Context<Buy>) -> Result<()> {
    let mp = &ctx.accounts.marketplace;
    let listing = &ctx.accounts.listing;

    // Only valid for SOL listings.
    require!(
        listing.payment_mint == system_program::ID,
        MarketplaceError::NotSolListing
    );

    let fee_lamports = (listing.price as u128)
        .checked_mul(mp.fee as u128)
        .ok_or(MarketplaceError::Overflow)?
        .checked_div(10_000)
        .ok_or(MarketplaceError::Overflow)? as u64;
    let seller_lamports = listing
        .price
        .checked_sub(fee_lamports)
        .ok_or(MarketplaceError::Overflow)?;

    let mp_seeds: &[&[u8]] = &[b"marketplace", mp.name.as_bytes(), &[mp.bump]];

    // 1. Pay fee → treasury.
    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.buyer.to_account_info(),
                to: ctx.accounts.treasury.to_account_info(),
            },
        ),
        fee_lamports,
    )?;

    // 2. Pay rest → seller.
    system_program::transfer(
        CpiContext::new(
            system_program::ID,
            system_program::Transfer {
                from: ctx.accounts.buyer.to_account_info(),
                to: ctx.accounts.seller.to_account_info(),
            },
        ),
        seller_lamports,
    )?;

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
pub struct Buy<'info> {
    #[account(mut)]
    pub buyer: Signer<'info>,

    /// CHECK: seller receives SOL, identity confirmed via listing.seller
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

    /// CHECK: treasury PDA — SOL fees land here
    #[account(
        mut,
        seeds = [b"treasury", marketplace.key().as_ref()],
        bump = marketplace.treasury_bump,
    )]
    pub treasury: UncheckedAccount<'info>,

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

    pub system_program: Program<'info, System>,
}
