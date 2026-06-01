# Assignment 04 — NFT Staking with Separate Claim Rewards

An Anchor 1.0 program that lets users stake MPL Core NFTs, earn reward tokens over time, and claim rewards independently of unstaking.

## Features

| Instruction | Description |
|---|---|
| `initialize_config` | Deploy the staking config and the SPL reward-token mint |
| `initialize_user` | Create a per-user account tracking points and staked count |
| `stake` | Freeze an MPL Core asset via `FreezeDelegate` plugin; increment collection `staked_count` |
| `claim_rewards` | Mint accrued reward tokens **without** unstaking the NFT |
| `unstake` | Unfreeze and undelegate the NFT; decrement collection `staked_count` |

### Challenge requirements

**a) Claim rewards without unstaking** — `claim_rewards` is a fully independent instruction. It advances `last_update` to the current timestamp and mints `elapsed_seconds × points_per_stake` reward tokens to the user's ATA. The NFT stays staked.

**b) Unstake right after claiming rewards** — `unstake` has no implicit reward-claim step. After `claim_rewards`, `elapsed = 0` so there are no pending rewards; `unstake` succeeds immediately (subject to the `freeze_period` guard).

**Attributes plugin** — The MPL Core collection carries an `Attributes` plugin with a `staked_count` key whose value is a decimal string. The `config` PDA is the plugin authority so only this program can modify it. `stake` increments the count; `unstake` decrements it.

## Architecture

```
StakeConfig  (PDA: ["config"])
  ├── points_per_stake : u8   — reward tokens per second per NFT
  ├── max_stake        : u8   — max NFTs staked per user
  ├── freeze_period    : u32  — minimum lock period in seconds
  └── reward_mint      (PDA: ["rewards", config])

UserAccount  (PDA: ["user", user])
  ├── points        : u64  — cumulative reward total (informational)
  └── amount_staked : u8

StakeAccount (PDA: ["stake", asset, config])
  ├── owner       : Pubkey
  ├── asset       : Pubkey   — MPL Core asset address
  └── last_update : i64      — unix timestamp of stake or last claim
```

## Running Tests

### Prerequisites

- Rust toolchain `1.89.0` (managed by `rust-toolchain.toml`)
- Solana CLI ≥ 2.0 with `cargo-build-sbf`

### 1. Build the program

```bash
cd Assignment_04/nft_staking
cargo build-sbf
```

### 2. Obtain the MPL Core binary (first run only)

The tests load the live MPL Core program from a local fixture file. The first run auto-downloads it from Solana mainnet:

```bash
solana program dump --url mainnet-beta \
  CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d \
  programs/nft_staking/tests/programs/mpl_core.so
```

Alternatively, set `MPL_CORE_SO=/path/to/mpl_core.so` before running tests.

### 3. Run all tests

```bash
cargo test
```

Expected output:

<img width="710" height="427" alt="Screenshot 2026-06-01 at 9 52 15 PM" src="https://github.com/user-attachments/assets/e50e3e8b-c63a-4eca-b249-0e98210a0a3b" />


## Test Coverage

| Test | What it verifies |
|---|---|
| `test_initialize_config_stores_values` | Config account holds correct parameters |
| `test_initialize_config_creates_reward_mint` | Reward mint PDA is created on-chain |
| `test_initialize_user_creates_account` | User account is zero-initialised |
| `test_stake_creates_stake_account` | Stake account exists with correct owner/asset |
| `test_stake_increments_user_amount_staked` | `amount_staked` increments on stake |
| `test_claim_rewards_mints_tokens` | Reward tokens appear in user's ATA |
| `test_claim_rewards_updates_last_update` | `last_update` advances after claim |
| `test_claim_rewards_does_not_unstake` | NFT stays staked after `claim_rewards` |
| `test_unstake_after_claim_rewards_succeeds` | Unstake works immediately after claim |
| `test_unstake_closes_stake_account` | Stake account closed; `amount_staked` decrements |
| `test_unstake_before_freeze_period_fails` | Unstake reverts when lock period hasn't elapsed |
| `test_max_stake_enforced` | Second stake fails when `max_stake = 1` |
| `test_claim_by_non_owner_fails` | Non-owner cannot claim rewards |

## Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `anchor-lang` | 1.0.2 | Anchor framework (Solana 2.x) |
| `anchor-spl` | 1.0.2 | SPL Token CPI helpers |
| `mpl-core` | 0.12 | MPL Core NFT standard (FreezeDelegate, Attributes plugins) |
| `litesvm` | 0.12 | Fast SBF test runtime |
