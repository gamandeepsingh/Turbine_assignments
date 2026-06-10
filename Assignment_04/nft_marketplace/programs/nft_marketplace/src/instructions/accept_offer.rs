use anchor_lang::prelude::*;
use mpl_core::{
    instructions::{RemovePluginV1CpiBuilder, TransferV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{FreezeDelegate, Plugin, PluginType},
    ID as MPL_CORE_ID,
};

use crate::{
    error::MarketplaceError,
    state::{Listing, Marketplace, Offer},
};

pub(crate) fn handler(ctx: Context<AcceptOffer>) -> Result<()> {
    let mp = &ctx.accounts.marketplace;
    let offer = &ctx.accounts.offer;

    let fee_lamports = (offer.amount as u128)
        .checked_mul(mp.fee as u128)
        .ok_or(MarketplaceError::Overflow)?
        .checked_div(10_000)
        .ok_or(MarketplaceError::Overflow)? as u64;
    let seller_lamports = offer
        .amount
        .checked_sub(fee_lamports)
        .ok_or(MarketplaceError::Overflow)?;

    let mp_seeds: &[&[u8]] = &[b"marketplace", mp.name.as_bytes(), &[mp.bump]];

    // 1. Unfreeze NFT (marketplace PDA signs).
    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.seller)
        .authority(Some(&ctx.accounts.marketplace.to_account_info()))
        .system_program(&ctx.accounts.system_program)
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
        .invoke_signed(&[mp_seeds])?;

    // 2. Transfer NFT to buyer using TransferDelegate (marketplace PDA signs).
    TransferV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.seller)
        .authority(Some(&ctx.accounts.marketplace.to_account_info()))
        .new_owner(&ctx.accounts.buyer)
        .system_program(Some(&ctx.accounts.system_program))
        .invoke_signed(&[mp_seeds])?;

    // 3. Remove FreezeDelegate — new owner (buyer) signs.
    RemovePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.seller)
        .authority(Some(&ctx.accounts.buyer))
        .system_program(&ctx.accounts.system_program)
        .plugin_type(PluginType::FreezeDelegate)
        .invoke()?;

    // 4. Remove TransferDelegate — new owner (buyer) signs.
    RemovePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.seller)
        .authority(Some(&ctx.accounts.buyer))
        .system_program(&ctx.accounts.system_program)
        .plugin_type(PluginType::TransferDelegate)
        .invoke()?;

    // 5. Transfer SOL from Offer PDA → treasury (fee).
    //    Direct lamport manipulation: Offer PDA is owned by this program.
    {
        let offer_info = ctx.accounts.offer.to_account_info();
        let treasury_info = ctx.accounts.treasury.to_account_info();
        **offer_info.try_borrow_mut_lamports()? -= fee_lamports;
        **treasury_info.try_borrow_mut_lamports()? += fee_lamports;
    }

    // 6. Transfer SOL from Offer PDA → seller (remaining after fee).
    //    The rest drains when the account is closed (close = seller), but we
    //    need to move seller_lamports explicitly before close sweeps the rest.
    {
        let offer_info = ctx.accounts.offer.to_account_info();
        let seller_info = ctx.accounts.seller.to_account_info();
        **offer_info.try_borrow_mut_lamports()? -= seller_lamports;
        **seller_info.try_borrow_mut_lamports()? += seller_lamports;
    }

    Ok(())
}

#[derive(Accounts)]
pub struct AcceptOffer<'info> {
    /// Seller accepts the offer.
    #[account(mut)]
    pub seller: Signer<'info>,

    /// Buyer who made the offer — also signs for removing plugins after transfer.
    #[account(mut)]
    pub buyer: Signer<'info>,

    /// CHECK: Validated by MPL Core CPI
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,

    /// CHECK: Validated by MPL Core CPI
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,

    #[account(seeds = [b"marketplace", marketplace.name.as_bytes()], bump = marketplace.bump)]
    pub marketplace: Account<'info, Marketplace>,

    /// CHECK: treasury receives SOL fee
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
        constraint = listing.seller == seller.key() @ MarketplaceError::NotSeller,
    )]
    pub listing: Account<'info, Listing>,

    #[account(
        mut,
        close = buyer,
        seeds = [b"offer", asset.key().as_ref(), buyer.key().as_ref()],
        bump = offer.bump,
    )]
    pub offer: Account<'info, Offer>,

    /// CHECK: Verified by address constraint
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}
