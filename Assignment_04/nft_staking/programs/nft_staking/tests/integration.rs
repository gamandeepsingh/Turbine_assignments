/// Integration tests for the nft_staking program using LiteSVM.
///
/// The MPL Core program binary is required.  On the first run the test harness
/// tries to download it with `solana program dump`.  Make sure the Solana CLI
/// is available and you have RPC access to mainnet (or pass the env var
/// `MPL_CORE_SO` pointing to a local copy).
use anchor_lang::{
    solana_program::{clock::Clock, instruction::Instruction},
    AccountDeserialize, InstructionData, ToAccountMetas,
};
use litesvm::LiteSVM;
use nft_staking::{
    accounts as staking_accounts, instruction as staking_ix, StakeAccount, StakeConfig, UserAccount,
};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use spl_associated_token_account_interface::{
    address::get_associated_token_address,
    program::id as ata_id,
};
use spl_token_interface::ID as TOKEN_ID;
use std::path::PathBuf;

const PROGRAM_ID: Pubkey = nft_staking::ID;
const MPL_CORE_ID: Pubkey = solana_pubkey::pubkey!("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");

// ─── helpers ──────────────────────────────────────────────────────────────────

fn system_id() -> Pubkey {
    solana_sdk_ids::system_program::ID
}

fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    svm.send_transaction(tx).unwrap();
}

fn send_multi(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, extra: &[&Keypair]) {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    let mut signers: Vec<&Keypair> = vec![payer];
    for e in extra {
        if e.pubkey() != payer.pubkey() {
            signers.push(e);
        }
    }
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &signers).unwrap();
    svm.send_transaction(tx).unwrap();
}

fn send_fails(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> bool {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    svm.send_transaction(tx).is_err()
}

/// Download (once) and cache the MPL Core SBF binary.
fn mpl_core_bytes() -> Vec<u8> {
    // Allow an env override for CI / offline environments.
    if let Ok(path) = std::env::var("MPL_CORE_SO") {
        return std::fs::read(&path)
            .unwrap_or_else(|_| panic!("MPL_CORE_SO={path} not readable"));
    }

    let fixture: PathBuf = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/programs/mpl_core.so");

    if fixture.exists() {
        return std::fs::read(&fixture).expect("failed to read mpl_core.so fixture");
    }

    // Try to download via solana CLI.
    eprintln!("Downloading MPL Core binary (one-time)…");
    let status = std::process::Command::new("solana")
        .args([
            "program",
            "dump",
            "--url",
            "mainnet-beta",
            "CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d",
            fixture.to_str().unwrap(),
        ])
        .status()
        .expect("solana CLI not found — install the Solana tool suite");

    assert!(
        status.success(),
        "Failed to download MPL Core binary. \
         Set MPL_CORE_SO=/path/to/mpl_core.so or ensure mainnet RPC is reachable."
    );

    std::fs::read(&fixture).unwrap()
}

fn nft_staking_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy/nft_staking.so");
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "nft_staking.so not found at {path:?}. \
             Run `cargo build-sbf` (or `anchor build`) first."
        )
    })
}

fn fresh_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    // Load our staking program (built with `cargo build-sbf`)
    svm.add_program(PROGRAM_ID, &nft_staking_bytes()).unwrap();
    // Load MPL Core program
    let core_bytes = mpl_core_bytes();
    svm.add_program(MPL_CORE_ID, &core_bytes).unwrap();
    svm
}

// ─── PDA helpers ─────────────────────────────────────────────────────────────

fn config_pda() -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"config"], &PROGRAM_ID)
}

fn reward_mint_pda(config: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"rewards", config.as_ref()], &PROGRAM_ID)
}

fn user_pda(user: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"user", user.as_ref()], &PROGRAM_ID)
}

fn stake_pda(asset: &Pubkey, config: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"stake", asset.as_ref(), config.as_ref()], &PROGRAM_ID)
}

// ─── MPL Core helper CPIs (called from test harness, not the program) ────────

/// Create a minimal MPL Core collection asset.
fn create_collection(svm: &mut LiteSVM, payer: &Keypair) -> Pubkey {
    let collection_kp = Keypair::new();
    // MPL Core CreateCollectionV1 discriminator + minimal data
    // We use the mpl_core CPI builder indirectly via raw instruction construction.
    // For LiteSVM tests we build the instruction bytes manually.
    use mpl_core::instructions::CreateCollectionV1Builder;
    let ix = CreateCollectionV1Builder::new()
        .collection(collection_kp.pubkey())
        .payer(payer.pubkey())
        .update_authority(Some(payer.pubkey()))
        .name("Test Collection".to_string())
        .uri("https://example.com/collection.json".to_string())
        .instruction();

    send_multi(svm, &[ix], payer, &[&collection_kp]);
    collection_kp.pubkey()
}

/// Add Attributes plugin to the collection so the staking program can update it.
fn add_collection_attributes(svm: &mut LiteSVM, payer: &Keypair, collection: &Pubkey, config: &Pubkey) {
    use mpl_core::{
        instructions::AddCollectionPluginV1Builder,
        types::{Attribute, Attributes, Plugin, PluginAuthority},
    };
    let ix = AddCollectionPluginV1Builder::new()
        .collection(*collection)
        .payer(payer.pubkey())
        .authority(Some(payer.pubkey()))
        .plugin(Plugin::Attributes(Attributes {
            attribute_list: vec![Attribute {
                key: "staked_count".to_string(),
                value: "0".to_string(),
            }],
        }))
        .init_authority(PluginAuthority::Address { address: *config })
        .instruction();

    send(svm, ix, payer);
}

/// Create an MPL Core asset belonging to `collection`, owned by `owner`.
fn create_asset(svm: &mut LiteSVM, payer: &Keypair, owner: &Pubkey, collection: &Pubkey) -> Pubkey {
    use mpl_core::instructions::CreateV2Builder;
    let asset_kp = Keypair::new();
    let ix = CreateV2Builder::new()
        .asset(asset_kp.pubkey())
        .collection(Some(*collection))
        .payer(payer.pubkey())
        .owner(Some(*owner))
        .authority(Some(payer.pubkey()))
        .name("Test NFT".to_string())
        .uri("https://example.com/nft.json".to_string())
        .instruction();

    send_multi(svm, &[ix], payer, &[&asset_kp]);
    asset_kp.pubkey()
}

// ─── instruction builders ────────────────────────────────────────────────────

fn ix_initialize_config(admin: &Pubkey, points: u8, max_stake: u8, freeze_period: u32) -> Instruction {
    let (config, _) = config_pda();
    let (reward_mint, _) = reward_mint_pda(&config);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &staking_ix::InitializeConfig { points_per_stake: points, max_stake, freeze_period }.data(),
        staking_accounts::InitializeConfig {
            admin: *admin,
            config,
            reward_mint,
            system_program: system_id(),
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_initialize_user(user: &Pubkey) -> Instruction {
    let (user_account, _) = user_pda(user);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &staking_ix::InitializeUser {}.data(),
        staking_accounts::InitializeUser {
            user: *user,
            user_account,
            system_program: system_id(),
        }
        .to_account_metas(None),
    )
}

fn ix_stake(user: &Pubkey, asset: &Pubkey, collection: &Pubkey) -> Instruction {
    let (config, _) = config_pda();
    let (stake_account, _) = stake_pda(asset, &config);
    let (user_account, _) = user_pda(user);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &staking_ix::Stake {}.data(),
        staking_accounts::Stake {
            user: *user,
            asset: *asset,
            collection: *collection,
            config,
            stake_account,
            user_account,
            mpl_core_program: MPL_CORE_ID,
            system_program: system_id(),
        }
        .to_account_metas(None),
    )
}

fn ix_claim_rewards(user: &Pubkey, asset: &Pubkey) -> Instruction {
    let (config, _) = config_pda();
    let (reward_mint, _) = reward_mint_pda(&config);
    let (stake_account, _) = stake_pda(asset, &config);
    let (user_account, _) = user_pda(user);
    let user_reward_ata = get_associated_token_address(user, &reward_mint);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &staking_ix::ClaimRewards {}.data(),
        staking_accounts::ClaimRewards {
            user: *user,
            config,
            reward_mint,
            asset: *asset,
            stake_account,
            user_account,
            user_reward_ata,
            token_program: TOKEN_ID,
            associated_token_program: ata_id(),
            system_program: system_id(),
        }
        .to_account_metas(None),
    )
}

fn ix_unstake(user: &Pubkey, asset: &Pubkey, collection: &Pubkey) -> Instruction {
    let (config, _) = config_pda();
    let (stake_account, _) = stake_pda(asset, &config);
    let (user_account, _) = user_pda(user);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &staking_ix::Unstake {}.data(),
        staking_accounts::Unstake {
            user: *user,
            asset: *asset,
            collection: *collection,
            config,
            stake_account,
            user_account,
            mpl_core_program: MPL_CORE_ID,
            system_program: system_id(),
        }
        .to_account_metas(None),
    )
}

// ─── test fixtures ────────────────────────────────────────────────────────────

struct Fixture {
    svm: LiteSVM,
    admin: Keypair,
    user: Keypair,
    collection: Pubkey,
    asset: Pubkey,
    config: Pubkey,
}

fn setup(freeze_period: u32) -> Fixture {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

    // 1. Initialize config
    send(
        &mut svm,
        ix_initialize_config(&admin.pubkey(), 1, 10, freeze_period),
        &admin,
    );
    let (config, _) = config_pda();

    // 2. Create collection + add Attributes plugin
    let collection = create_collection(&mut svm, &admin);
    add_collection_attributes(&mut svm, &admin, &collection, &config);

    // 3. Create NFT asset owned by user
    let asset = create_asset(&mut svm, &admin, &user.pubkey(), &collection);

    // 4. Initialize user account
    send(&mut svm, ix_initialize_user(&user.pubkey()), &user);

    Fixture { svm, admin, user, collection, asset, config }
}

fn token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    match svm.get_account(ata) {
        Some(acc) if acc.data.len() >= 72 => {
            u64::from_le_bytes(acc.data[64..72].try_into().unwrap())
        }
        _ => 0,
    }
}

/// Advance the Clock sysvar's unix_timestamp by `seconds`.
/// LiteSVM's `warp_to_slot` does not advance unix_timestamp, so we set it directly.
fn warp_time(svm: &mut LiteSVM, seconds: i64) {
    let mut clock: Clock = svm.get_sysvar();
    clock.unix_timestamp += seconds;
    svm.set_sysvar(&clock);
}

fn read_config(svm: &LiteSVM) -> StakeConfig {
    let (config, _) = config_pda();
    let acc = svm.get_account(&config).unwrap();
    StakeConfig::try_deserialize(&mut &acc.data[..]).unwrap()
}

fn read_user(svm: &LiteSVM, user: &Pubkey) -> UserAccount {
    let (pda, _) = user_pda(user);
    let acc = svm.get_account(&pda).unwrap();
    UserAccount::try_deserialize(&mut &acc.data[..]).unwrap()
}

fn read_stake(svm: &LiteSVM, asset: &Pubkey) -> StakeAccount {
    let (config, _) = config_pda();
    let (pda, _) = stake_pda(asset, &config);
    let acc = svm.get_account(&pda).unwrap();
    StakeAccount::try_deserialize(&mut &acc.data[..]).unwrap()
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_config_stores_values() {
    let f = setup(3600);
    let cfg = read_config(&f.svm);
    assert_eq!(cfg.points_per_stake, 1);
    assert_eq!(cfg.max_stake, 10);
    assert_eq!(cfg.freeze_period, 3600);
}

#[test]
fn test_initialize_config_creates_reward_mint() {
    let f = setup(0);
    let (config, _) = config_pda();
    let (reward_mint, _) = reward_mint_pda(&config);
    assert!(f.svm.get_account(&reward_mint).is_some(), "reward mint must exist");
}

#[test]
fn test_initialize_user_creates_account() {
    let f = setup(0);
    let ua = read_user(&f.svm, &f.user.pubkey());
    assert_eq!(ua.points, 0);
    assert_eq!(ua.amount_staked, 0);
}

#[test]
fn test_stake_creates_stake_account() {
    let mut f = setup(0);
    send(
        &mut f.svm,
        ix_stake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    let sa = read_stake(&f.svm, &f.asset);
    assert_eq!(sa.owner, f.user.pubkey());
    assert_eq!(sa.asset, f.asset);
}

#[test]
fn test_stake_increments_user_amount_staked() {
    let mut f = setup(0);
    send(
        &mut f.svm,
        ix_stake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    let ua = read_user(&f.svm, &f.user.pubkey());
    assert_eq!(ua.amount_staked, 1);
}

#[test]
fn test_claim_rewards_mints_tokens() {
    let mut f = setup(0);
    send(
        &mut f.svm,
        ix_stake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    // Warp time forward so there are rewards to claim
    warp_time(&mut f.svm, 500);

    send(
        &mut f.svm,
        ix_claim_rewards(&f.user.pubkey(), &f.asset),
        &f.user,
    );

    let (config, _) = config_pda();
    let (reward_mint, _) = reward_mint_pda(&config);
    let user_ata = get_associated_token_address(&f.user.pubkey(), &reward_mint);
    assert!(token_balance(&f.svm, &user_ata) > 0, "user should receive reward tokens");
}

#[test]
fn test_claim_rewards_updates_last_update() {
    let mut f = setup(0);
    send(
        &mut f.svm,
        ix_stake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    let before = read_stake(&f.svm, &f.asset).last_update;

    warp_time(&mut f.svm, 200);

    send(
        &mut f.svm,
        ix_claim_rewards(&f.user.pubkey(), &f.asset),
        &f.user,
    );

    let after = read_stake(&f.svm, &f.asset).last_update;
    assert!(after > before, "last_update must advance after claim");
}

#[test]
fn test_claim_rewards_does_not_unstake() {
    let mut f = setup(0);
    send(
        &mut f.svm,
        ix_stake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    warp_time(&mut f.svm, 100);

    send(
        &mut f.svm,
        ix_claim_rewards(&f.user.pubkey(), &f.asset),
        &f.user,
    );

    // stake account must still exist
    let (config, _) = config_pda();
    let (stake_pda, _) = stake_pda(&f.asset, &config);
    assert!(
        f.svm.get_account(&stake_pda).is_some(),
        "NFT must remain staked after claim_rewards"
    );
    assert_eq!(
        read_user(&f.svm, &f.user.pubkey()).amount_staked,
        1,
        "amount_staked must stay 1 after claim_rewards"
    );
}

#[test]
fn test_unstake_after_claim_rewards_succeeds() {
    let mut f = setup(0); // freeze_period = 0 so we can unstake immediately
    send(
        &mut f.svm,
        ix_stake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    warp_time(&mut f.svm, 100);

    // Claim first
    send(
        &mut f.svm,
        ix_claim_rewards(&f.user.pubkey(), &f.asset),
        &f.user,
    );

    // Then unstake — must succeed with 0 pending rewards
    send(
        &mut f.svm,
        ix_unstake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    let (config, _) = config_pda();
    let (stake_pda_key, _) = stake_pda(&f.asset, &config);
    assert!(
        f.svm.get_account(&stake_pda_key).is_none(),
        "stake account must be closed after unstake"
    );
}

#[test]
fn test_unstake_closes_stake_account() {
    let mut f = setup(0);
    send(
        &mut f.svm,
        ix_stake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    warp_time(&mut f.svm, 50);

    send(
        &mut f.svm,
        ix_unstake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    let ua = read_user(&f.svm, &f.user.pubkey());
    assert_eq!(ua.amount_staked, 0, "amount_staked should be 0 after unstake");
}

#[test]
fn test_unstake_before_freeze_period_fails() {
    let mut f = setup(86400); // 24-hour lock
    send(
        &mut f.svm,
        ix_stake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    // Do NOT advance time — should fail
    let ix = ix_unstake(&f.user.pubkey(), &f.asset, &f.collection);
    assert!(
        send_fails(&mut f.svm, ix, &f.user),
        "unstake before freeze_period must fail"
    );
}

#[test]
fn test_max_stake_enforced() {
    let _f = setup(0);
    // Re-init config with max_stake = 1 to trigger the limit on second stake
    // (config already created in setup with max_stake=10, so we create a
    //  second user for a fresh config isn't possible without re-deploying;
    //  instead we directly exhaust the limit by staking max_stake=1 from a
    //  fresh setup)
    let mut svm2 = fresh_svm();
    let admin2 = Keypair::new();
    let user2 = Keypair::new();
    svm2.airdrop(&admin2.pubkey(), 10_000_000_000).unwrap();
    svm2.airdrop(&user2.pubkey(), 10_000_000_000).unwrap();

    send(
        &mut svm2,
        ix_initialize_config(&admin2.pubkey(), 1, 1, 0), // max_stake = 1
        &admin2,
    );
    let (config2, _) = config_pda();
    let collection2 = create_collection(&mut svm2, &admin2);
    add_collection_attributes(&mut svm2, &admin2, &collection2, &config2);

    let asset1 = create_asset(&mut svm2, &admin2, &user2.pubkey(), &collection2);
    let asset2 = create_asset(&mut svm2, &admin2, &user2.pubkey(), &collection2);

    send(&mut svm2, ix_initialize_user(&user2.pubkey()), &user2);
    send(&mut svm2, ix_stake(&user2.pubkey(), &asset1, &collection2), &user2);

    let ix2 = ix_stake(&user2.pubkey(), &asset2, &collection2);
    assert!(
        send_fails(&mut svm2, ix2, &user2),
        "second stake must fail when max_stake = 1"
    );
}

#[test]
fn test_claim_by_non_owner_fails() {
    let mut f = setup(0);
    let attacker = Keypair::new();
    f.svm.airdrop(&attacker.pubkey(), 1_000_000_000).unwrap();
    send(
        &mut f.svm,
        ix_initialize_user(&attacker.pubkey()),
        &attacker,
    );
    send(
        &mut f.svm,
        ix_stake(&f.user.pubkey(), &f.asset, &f.collection),
        &f.user,
    );

    warp_time(&mut f.svm, 100);

    let ix = ix_claim_rewards(&attacker.pubkey(), &f.asset);
    assert!(
        send_fails(&mut f.svm, ix, &attacker),
        "non-owner must not be able to claim rewards"
    );
}
