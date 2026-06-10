use anchor_lang::prelude::*;
use mpl_core::{
    instructions::{RemovePluginV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{FreezeDelegate, Plugin, PluginType},
    ID as MPL_CORE_ID,
};

use crate::{error::MarketplaceError, state::{Listing, Marketplace}};

pub(crate) fn handler(ctx: Context<Delist>) -> Result<()> {
    let mp = &ctx.accounts.marketplace;
    let mp_seeds: &[&[u8]] = &[b"marketplace", mp.name.as_bytes(), &[mp.bump]];

    // 1. Unfreeze the asset (marketplace PDA signs as FreezeDelegate authority).
    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.seller)
        .authority(Some(&ctx.accounts.marketplace.to_account_info()))
        .system_program(&ctx.accounts.system_program)
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
        .invoke_signed(&[mp_seeds])?;

    // 2. Remove FreezeDelegate — seller (owner) signs.
    RemovePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.seller)
        .authority(Some(&ctx.accounts.seller))
        .system_program(&ctx.accounts.system_program)
        .plugin_type(PluginType::FreezeDelegate)
        .invoke()?;

    // 3. Remove TransferDelegate — seller (owner) signs.
    RemovePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.seller)
        .authority(Some(&ctx.accounts.seller))
        .system_program(&ctx.accounts.system_program)
        .plugin_type(PluginType::TransferDelegate)
        .invoke()?;

    Ok(())
}

#[derive(Accounts)]
pub struct Delist<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,

    /// CHECK: Validated by MPL Core CPI
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,

    /// CHECK: Validated by MPL Core CPI
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,

    #[account(seeds = [b"marketplace", marketplace.name.as_bytes()], bump = marketplace.bump)]
    pub marketplace: Account<'info, Marketplace>,

    #[account(
        mut,
        close = seller,
        seeds = [b"listing", marketplace.key().as_ref(), asset.key().as_ref()],
        bump = listing.bump,
        constraint = listing.seller == seller.key() @ MarketplaceError::NotSeller,
    )]
    pub listing: Account<'info, Listing>,

    /// CHECK: Verified by address constraint
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}
