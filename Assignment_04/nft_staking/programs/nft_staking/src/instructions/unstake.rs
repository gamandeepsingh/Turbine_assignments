use anchor_lang::prelude::*;
use mpl_core::{
    instructions::{RemovePluginV1CpiBuilder, UpdateCollectionPluginV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{Attribute, Attributes, FreezeDelegate, Plugin, PluginType},
    ID as MPL_CORE_ID,
};

use crate::{error::StakeError, state::{StakeAccount, StakeConfig, UserAccount}};

pub(crate) fn handler(ctx: Context<Unstake>) -> Result<()> {
    let config = &ctx.accounts.config;
    let stake_account = &ctx.accounts.stake_account;

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // Enforce the minimum lock period
    let elapsed = now
        .checked_sub(stake_account.last_update)
        .ok_or(StakeError::Overflow)?;
    require!(
        elapsed >= config.freeze_period as i64,
        StakeError::FreezePeriodNotElapsed
    );

    let config_seeds: &[&[u8]] = &[b"config", &[config.bump]];

    // Step 1: Unfreeze the asset (set frozen = false)
    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.user)
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .system_program(&ctx.accounts.system_program)
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
        .invoke_signed(&[config_seeds])?;

    // Step 2: Remove the FreezeDelegate plugin.
    //         The config PDA authority only covers freeze/unfreeze updates.
    //         Removal requires the asset owner; the user is the owner so we
    //         use them as authority and call invoke() (no PDA signing needed).
    RemovePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.user)
        .authority(Some(&ctx.accounts.user))
        .system_program(&ctx.accounts.system_program)
        .plugin_type(PluginType::FreezeDelegate)
        .invoke()?;

    // Step 3: Decrement the collection's staked_count attribute
    update_staked_count(&ctx, -1, config_seeds)?;

    // Step 4: Update user account
    let user_account = &mut ctx.accounts.user_account;
    user_account.amount_staked = user_account.amount_staked.saturating_sub(1);

    Ok(())
}

fn update_staked_count(ctx: &Context<Unstake>, delta: i64, config_seeds: &[&[u8]]) -> Result<()> {
    let collection_data = ctx.accounts.collection.data.borrow();
    let current: i64 = parse_staked_count(&collection_data).unwrap_or(0);
    drop(collection_data);

    let new_count = (current + delta).max(0);

    UpdateCollectionPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .collection(&ctx.accounts.collection)
        .payer(&ctx.accounts.user)
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .system_program(&ctx.accounts.system_program)
        .plugin(Plugin::Attributes(Attributes {
            attribute_list: vec![Attribute {
                key: "staked_count".to_string(),
                value: new_count.to_string(),
            }],
        }))
        .invoke_signed(&[config_seeds])?;

    Ok(())
}

fn parse_staked_count(data: &[u8]) -> Option<i64> {
    let key = b"staked_count";
    let pos = data.windows(key.len()).position(|w| w == key)?;
    let after_key = pos + key.len();
    if after_key + 4 > data.len() {
        return None;
    }
    let val_len = u32::from_le_bytes(data[after_key..after_key + 4].try_into().ok()?) as usize;
    let val_start = after_key + 4;
    if val_start + val_len > data.len() {
        return None;
    }
    std::str::from_utf8(&data[val_start..val_start + val_len])
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
}

#[derive(Accounts)]
pub struct Unstake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: Validated by MPL Core CPI
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,

    /// CHECK: Validated by MPL Core CPI
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, StakeConfig>,

    #[account(
        mut,
        close = user,
        seeds = [b"stake", asset.key().as_ref(), config.key().as_ref()],
        bump = stake_account.bump,
        constraint = stake_account.owner == user.key() @ StakeError::NotOwner,
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    /// CHECK: Verified by address constraint
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}
