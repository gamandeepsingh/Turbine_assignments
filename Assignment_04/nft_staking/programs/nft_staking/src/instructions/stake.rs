use anchor_lang::prelude::*;
use mpl_core::{
    instructions::{AddPluginV1CpiBuilder, UpdateCollectionPluginV1CpiBuilder, UpdatePluginV1CpiBuilder},
    types::{Attribute, Attributes, FreezeDelegate, Plugin, PluginAuthority},
    ID as MPL_CORE_ID,
};

use crate::{error::StakeError, state::{StakeAccount, StakeConfig, UserAccount}};

pub(crate) fn handler(ctx: Context<Stake>) -> Result<()> {
    let config = &ctx.accounts.config;

    require!(
        ctx.accounts.user_account.amount_staked < config.max_stake,
        StakeError::MaxStakeReached
    );

    let clock = Clock::get()?;
    let now = clock.unix_timestamp;

    // Step 1: Add a FreezeDelegate plugin to the asset.
    //         The user (current owner) must sign this. The config PDA is set as
    //         the plugin authority so only our program can freeze/unfreeze.
    AddPluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.user)
        .authority(Some(&ctx.accounts.user))
        .system_program(&ctx.accounts.system_program)
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: false }))
        .init_authority(PluginAuthority::Address {
            address: config.key(),
        })
        .invoke()?;

    // Step 2: Freeze the asset using our config PDA as the FreezeDelegate authority.
    let config_seeds: &[&[u8]] = &[b"config", &[config.bump]];
    UpdatePluginV1CpiBuilder::new(&ctx.accounts.mpl_core_program)
        .asset(&ctx.accounts.asset)
        .collection(Some(&ctx.accounts.collection))
        .payer(&ctx.accounts.user)
        .authority(Some(&ctx.accounts.config.to_account_info()))
        .system_program(&ctx.accounts.system_program)
        .plugin(Plugin::FreezeDelegate(FreezeDelegate { frozen: true }))
        .invoke_signed(&[config_seeds])?;

    // Step 3: Increment the staked_count attribute on the collection.
    update_staked_count(&ctx, 1, config_seeds)?;

    // Step 4: Initialise the stake record.
    let stake_account = &mut ctx.accounts.stake_account;
    stake_account.owner = ctx.accounts.user.key();
    stake_account.asset = ctx.accounts.asset.key();
    stake_account.last_update = now;
    stake_account.bump = ctx.bumps.stake_account;

    // Step 5: Update user counters.
    let user_account = &mut ctx.accounts.user_account;
    user_account.amount_staked = user_account
        .amount_staked
        .checked_add(1)
        .ok_or(StakeError::Overflow)?;

    Ok(())
}

/// Read the current `staked_count` from the collection Attributes plugin, apply
/// `delta` (+1 or -1), and write the new value back.
fn update_staked_count(ctx: &Context<Stake>, delta: i64, config_seeds: &[&[u8]]) -> Result<()> {
    let collection_data = ctx.accounts.collection.data.borrow();
    let current: i64 = if collection_data.len() > 0 {
        // Parse the staked_count attribute value stored as a decimal string.
        parse_staked_count(&collection_data).unwrap_or(0)
    } else {
        0
    };
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

/// Walk the raw account bytes to find the `staked_count` attribute value.
/// MPL Core stores plugins after the base asset data; we look for the key
/// string and read the adjacent value string without a full deserialise.
fn parse_staked_count(data: &[u8]) -> Option<i64> {
    let key = b"staked_count";
    let pos = data
        .windows(key.len())
        .position(|w| w == key)?;
    // The length-prefixed value string immediately follows the key bytes +
    // a 4-byte LE length prefix for the key itself plus its content.
    // A simpler (safe) approach: search for the value after the key.
    let after_key = pos + key.len();
    if after_key + 4 > data.len() {
        return None;
    }
    let val_len = u32::from_le_bytes(data[after_key..after_key + 4].try_into().ok()?) as usize;
    let val_start = after_key + 4;
    if val_start + val_len > data.len() {
        return None;
    }
    let val_bytes = &data[val_start..val_start + val_len];
    std::str::from_utf8(val_bytes)
        .ok()
        .and_then(|s| s.parse::<i64>().ok())
}

#[derive(Accounts)]
pub struct Stake<'info> {
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: Validated by MPL Core CPI (asset owner = user)
    #[account(mut)]
    pub asset: UncheckedAccount<'info>,

    /// CHECK: Validated by MPL Core CPI (collection matches asset)
    #[account(mut)]
    pub collection: UncheckedAccount<'info>,

    #[account(
        seeds = [b"config"],
        bump = config.bump,
    )]
    pub config: Account<'info, StakeConfig>,

    #[account(
        init,
        payer = user,
        space = 8 + StakeAccount::INIT_SPACE,
        seeds = [b"stake", asset.key().as_ref(), config.key().as_ref()],
        bump,
    )]
    pub stake_account: Account<'info, StakeAccount>,

    #[account(
        mut,
        seeds = [b"user", user.key().as_ref()],
        bump = user_account.bump,
    )]
    pub user_account: Account<'info, UserAccount>,

    /// CHECK: Verified by the constraint below
    #[account(address = MPL_CORE_ID)]
    pub mpl_core_program: UncheckedAccount<'info>,

    pub system_program: Program<'info, System>,
}
