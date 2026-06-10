use anchor_lang::prelude::*;
use mpl_core::{
    instructions::AddPluginV1CpiBuilder,
    types::{FreezeDelegate, Plugin, PluginAuthority, TransferDelegate},
    ID as MPL_CORE_ID,
};

use crate::state::{Listing, Marketplace};

pub(crate) fn handler(ctx: Context<List>, price: u64, payment_mint: Pubkey) -> Result<()> {
    let mp_key = ctx.accounts.marketplace.key();
    let mp_bump = ctx.accounts.marketplace.bump;

    // 1. Add FreezeDelegate (frozen=true immediately). The seller signs as asset
    //    owner; the marketplace config PDA is set as delegate authority so only
    //    this program can unfreeze later.
    AddPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.seller)
        .authority(Some(&ctx.accounts.seller))
        .system_program(&ctx.accounts.system_program)
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: true }))
        .init_authority(PluginAuthority::Address { address: mp_key })
        .invoke()?;

    // 2. Add TransferDelegate so the marketplace can move the NFT on purchase
    //    without requiring the seller to sign again.
    let _mp_seeds: &[&[u8]] = &[b"marketplace", ctx.accounts.marketplace.name.as_bytes(), &[mp_bump]];
    AddPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.seller)
        .authority(Some(&ctx.accounts.seller))
        .system_program(&ctx.accounts.system_program)
        .plugin(Plugin::TransferDelegate(TransferDelegate {}))
        .init_authority(PluginAuthority::Address { address: mp_key })
        .invoke()?;

    // 3. Record the listing.
    let listing = &mut ctx.accounts.listing;
    listing.seller = ctx.accounts.seller.key();
    listing.asset = ctx.accounts.asset.key();
    listing.price = price;
    listing.payment_mint = payment_mint;
    listing.bump = ctx.bumps.listing;

    Ok(())
}

#[derive(Accounts)]
pub struct List<'info> {
    #[account(mut)]
    pub seller: Signer<'info>,

    /// CHECK: Validated by MPL Core CPI (seller must be asset owner)
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,

    /// CHECK: Validated by MPL Core CPI
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,

    #[account(seeds = [b"marketplace", marketplace.name.as_bytes()], bump = marketplace.bump)]
    pub marketplace: Account<'info, Marketplace>,

    #[account(
        init,
        payer = seller,
        space = 8 + Listing::INIT_SPACE,
        seeds = [b"listing", marketplace.key().as_ref(), asset.key().as_ref()],
        bump,
    )]
    pub listing: Account<'info, Listing>,

    /// CHECK: Verified by address constraint
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}
