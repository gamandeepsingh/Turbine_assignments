use anchor_lang::{solana_program::instruction::Instruction, AccountDeserialize, InstructionData, ToAccountMetas};
use amm::{accounts as amm_accounts, instruction as amm_ix, Config};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use spl_associated_token_account_interface::{
    address::get_associated_token_address,
    instruction::create_associated_token_account,
    program::id as ata_id,
};
use spl_token_interface::{instruction as token_ix, ID as TOKEN_ID};

const PROGRAM_ID: Pubkey = amm::ID;
const MINT_SIZE: u64 = 82;

fn system_id() -> Pubkey {
    solana_sdk_ids::system_program::ID
}

fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    svm.send_transaction(tx).unwrap();
}

fn send_multi(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, extra: Option<&Keypair>) {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    let tx = match extra {
        Some(e) if e.pubkey() != payer.pubkey() => {
            VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer, e]).unwrap()
        }
        _ => VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap(),
    };
    svm.send_transaction(tx).unwrap();
}

fn send_fails(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> bool {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    svm.send_transaction(tx).is_err()
}

fn create_mint(svm: &mut LiteSVM, authority: &Keypair, decimals: u8) -> Pubkey {
    let kp = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(MINT_SIZE as usize);
    let ixs = [
        anchor_lang::solana_program::system_instruction::create_account(
            &authority.pubkey(), &kp.pubkey(), rent, MINT_SIZE, &TOKEN_ID,
        ),
        token_ix::initialize_mint(&TOKEN_ID, &kp.pubkey(), &authority.pubkey(), None, decimals).unwrap(),
    ];
    send_multi(svm, &ixs, authority, Some(&kp));
    kp.pubkey()
}

fn create_ata_mint_to(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Pubkey,
    mint_auth: &Keypair,
    owner: &Pubkey,
    amount: u64,
) -> Pubkey {
    let ata = get_associated_token_address(owner, mint);
    let ixs = [
        create_associated_token_account(&payer.pubkey(), owner, mint, &TOKEN_ID),
        token_ix::mint_to(&TOKEN_ID, mint, &ata, &mint_auth.pubkey(), &[], amount).unwrap(),
    ];
    send_multi(svm, &ixs, payer, Some(mint_auth));
    ata
}

fn create_ata_empty(svm: &mut LiteSVM, payer: &Keypair, mint: &Pubkey, owner: &Pubkey) -> Pubkey {
    let ata = get_associated_token_address(owner, mint);
    send(svm, create_associated_token_account(&payer.pubkey(), owner, mint, &TOKEN_ID), payer);
    ata
}

fn token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    let acc = svm.get_account(ata).unwrap();
    u64::from_le_bytes(acc.data[64..72].try_into().unwrap())
}

fn lp_supply(svm: &LiteSVM, lp_mint: &Pubkey) -> u64 {
    let acc = svm.get_account(lp_mint).unwrap();
    u64::from_le_bytes(acc.data[36..44].try_into().unwrap())
}

fn config_pda(seed: u64) -> Pubkey {
    Pubkey::find_program_address(&[b"config", &seed.to_le_bytes()], &PROGRAM_ID).0
}

fn lp_mint_pda(config: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"lp", config.as_ref()], &PROGRAM_ID).0
}

fn ix_initialize(admin: &Pubkey, mint_x: &Pubkey, mint_y: &Pubkey, seed: u64, fee: u16) -> Instruction {
    let config = config_pda(seed);
    let lp_mint = lp_mint_pda(&config);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &amm_ix::Initialize { seed, fee }.data(),
        amm_accounts::Initialize {
            admin: *admin,
            mint_x: *mint_x,
            mint_y: *mint_y,
            config,
            lp_mint,
            vault_x: get_associated_token_address(&config, mint_x),
            vault_y: get_associated_token_address(&config, mint_y),
            system_program: system_id(),
            token_program: TOKEN_ID,
            associated_token_program: ata_id(),
        }
        .to_account_metas(None),
    )
}

fn ix_add_liquidity(
    user: &Pubkey,
    mint_x: &Pubkey,
    mint_y: &Pubkey,
    seed: u64,
    amount_x: u64,
    amount_y: u64,
    min_lp: u64,
) -> Instruction {
    let config  = config_pda(seed);
    let lp_mint = lp_mint_pda(&config);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &amm_ix::AddLiquidity { seed, amount_x, amount_y, min_lp }.data(),
        amm_accounts::AddLiquidity {
            user: *user,
            mint_x: *mint_x,
            mint_y: *mint_y,
            config,
            lp_mint,
            vault_x: get_associated_token_address(&config, mint_x),
            vault_y: get_associated_token_address(&config, mint_y),
            user_x:  get_associated_token_address(user, mint_x),
            user_y:  get_associated_token_address(user, mint_y),
            user_lp: get_associated_token_address(user, &lp_mint),
            system_program: system_id(),
            token_program:  TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_remove_liquidity(
    user: &Pubkey,
    mint_x: &Pubkey,
    mint_y: &Pubkey,
    seed: u64,
    lp_amount: u64,
    min_x: u64,
    min_y: u64,
) -> Instruction {
    let config  = config_pda(seed);
    let lp_mint = lp_mint_pda(&config);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &amm_ix::RemoveLiquidity { seed, lp_amount, min_x, min_y }.data(),
        amm_accounts::RemoveLiquidity {
            user: *user,
            mint_x: *mint_x,
            mint_y: *mint_y,
            config,
            lp_mint,
            vault_x: get_associated_token_address(&config, mint_x),
            vault_y: get_associated_token_address(&config, mint_y),
            user_x:  get_associated_token_address(user, mint_x),
            user_y:  get_associated_token_address(user, mint_y),
            user_lp: get_associated_token_address(user, &lp_mint),
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_swap(
    user: &Pubkey,
    mint_x: &Pubkey,
    mint_y: &Pubkey,
    seed: u64,
    is_x_to_y: bool,
    amount_in: u64,
    min_amount_out: u64,
) -> Instruction {
    let config = config_pda(seed);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &amm_ix::Swap { seed, is_x_to_y, amount_in, min_amount_out }.data(),
        amm_accounts::Swap {
            user: *user,
            mint_x: *mint_x,
            mint_y: *mint_y,
            config,
            vault_x: get_associated_token_address(&config, mint_x),
            vault_y: get_associated_token_address(&config, mint_y),
            user_in:  if is_x_to_y { get_associated_token_address(user, mint_x) }
                      else         { get_associated_token_address(user, mint_y) },
            user_out: if is_x_to_y { get_associated_token_address(user, mint_y) }
                      else         { get_associated_token_address(user, mint_x) },
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

fn fresh_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/amm.so");
    svm.add_program(PROGRAM_ID, bytes).unwrap();
    svm
}

fn init_pool(svm: &mut LiteSVM, admin: &Keypair, seed: u64, fee: u16) -> (Pubkey, Pubkey) {
    let mint_x = create_mint(svm, admin, 6);
    let mint_y = create_mint(svm, admin, 6);
    send(svm, ix_initialize(&admin.pubkey(), &mint_x, &mint_y, seed, fee), admin);
    (mint_x, mint_y)
}

fn fund_user(svm: &mut LiteSVM, admin: &Keypair, user: &Keypair, mint_x: &Pubkey, mint_y: &Pubkey) {
    create_ata_mint_to(svm, admin, mint_x, admin, &user.pubkey(), 10_000_000);
    create_ata_mint_to(svm, admin, mint_y, admin, &user.pubkey(), 10_000_000);
    let config  = config_pda(42);
    let lp_mint = lp_mint_pda(&config);
    create_ata_empty(svm, user, &lp_mint, &user.pubkey());
}

#[test]
fn test_initialize_stores_config() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 1, 30);

    let config_key = config_pda(1);
    let acc = svm.get_account(&config_key).expect("config must exist");
    let cfg: Config = Config::try_deserialize(&mut &acc.data[..]).unwrap();
    assert_eq!(cfg.mint_x, mint_x);
    assert_eq!(cfg.mint_y, mint_y);
    assert_eq!(cfg.fee, 30);
    assert_eq!(cfg.seed, 1);
}

#[test]
fn test_initialize_creates_vaults_and_lp_mint() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 2, 30);

    let config  = config_pda(2);
    let lp_mint = lp_mint_pda(&config);
    assert!(svm.get_account(&lp_mint).is_some(), "lp_mint must exist");
    assert!(svm.get_account(&get_associated_token_address(&config, &mint_x)).is_some());
    assert!(svm.get_account(&get_associated_token_address(&config, &mint_y)).is_some());
}

#[test]
fn test_add_liquidity_mints_lp_tokens() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 42, 30);
    fund_user(&mut svm, &admin, &user, &mint_x, &mint_y);

    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 1_000_000, 4_000_000, 1), &user);

    let config  = config_pda(42);
    let lp_mint = lp_mint_pda(&config);
    let user_lp = get_associated_token_address(&user.pubkey(), &lp_mint);
    assert!(token_balance(&svm, &user_lp) > 0, "user should receive LP tokens");
    assert_eq!(token_balance(&svm, &user_lp), lp_supply(&svm, &lp_mint));
}

#[test]
fn test_add_liquidity_deposits_into_vaults() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 42, 30);
    fund_user(&mut svm, &admin, &user, &mint_x, &mint_y);

    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 1_000_000, 2_000_000, 1), &user);

    let config  = config_pda(42);
    assert_eq!(token_balance(&svm, &get_associated_token_address(&config, &mint_x)), 1_000_000);
    assert_eq!(token_balance(&svm, &get_associated_token_address(&config, &mint_y)), 2_000_000);
}

#[test]
fn test_add_liquidity_slippage() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 42, 30);
    fund_user(&mut svm, &admin, &user, &mint_x, &mint_y);

    let ix = ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 1_000_000, 2_000_000, u64::MAX);
    assert!(send_fails(&mut svm, ix, &user), "must fail when min_lp is impossibly high");
}

#[test]
fn test_remove_liquidity_returns_tokens() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 42, 30);
    fund_user(&mut svm, &admin, &user, &mint_x, &mint_y);
    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 2_000_000, 4_000_000, 1), &user);

    let config  = config_pda(42);
    let lp_mint = lp_mint_pda(&config);
    let user_lp = get_associated_token_address(&user.pubkey(), &lp_mint);
    let lp_bal  = token_balance(&svm, &user_lp);

    let ux_before = token_balance(&svm, &get_associated_token_address(&user.pubkey(), &mint_x));
    let uy_before = token_balance(&svm, &get_associated_token_address(&user.pubkey(), &mint_y));

    send(&mut svm, ix_remove_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, lp_bal, 1, 1), &user);

    assert!(token_balance(&svm, &get_associated_token_address(&user.pubkey(), &mint_x)) > ux_before);
    assert!(token_balance(&svm, &get_associated_token_address(&user.pubkey(), &mint_y)) > uy_before);
    assert_eq!(token_balance(&svm, &user_lp), 0, "LP tokens burned");
}

#[test]
fn test_remove_liquidity_slippage() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 42, 30);
    fund_user(&mut svm, &admin, &user, &mint_x, &mint_y);
    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 2_000_000, 4_000_000, 1), &user);

    let config  = config_pda(42);
    let lp_mint = lp_mint_pda(&config);
    let lp_bal  = token_balance(&svm, &get_associated_token_address(&user.pubkey(), &lp_mint));

    let ix = ix_remove_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, lp_bal, u64::MAX, 0);
    assert!(send_fails(&mut svm, ix, &user), "should fail when min_x is impossible");
}

#[test]
fn test_swap_x_to_y() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 42, 30);
    fund_user(&mut svm, &admin, &user, &mint_x, &mint_y);
    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 2_000_000, 4_000_000, 1), &user);

    let config    = config_pda(42);
    let uy_before = token_balance(&svm, &get_associated_token_address(&user.pubkey(), &mint_y));
    let vx_before = token_balance(&svm, &get_associated_token_address(&config, &mint_x));

    send(&mut svm, ix_swap(&user.pubkey(), &mint_x, &mint_y, 42, true, 100_000, 1), &user);

    assert!(token_balance(&svm, &get_associated_token_address(&user.pubkey(), &mint_y)) > uy_before);
    assert!(token_balance(&svm, &get_associated_token_address(&config, &mint_x)) > vx_before);
}

#[test]
fn test_swap_y_to_x() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 42, 30);
    fund_user(&mut svm, &admin, &user, &mint_x, &mint_y);
    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 4_000_000, 2_000_000, 1), &user);

    let config    = config_pda(42);
    let ux_before = token_balance(&svm, &get_associated_token_address(&user.pubkey(), &mint_x));
    let vy_before = token_balance(&svm, &get_associated_token_address(&config, &mint_y));

    send(&mut svm, ix_swap(&user.pubkey(), &mint_x, &mint_y, 42, false, 100_000, 1), &user);

    assert!(token_balance(&svm, &get_associated_token_address(&user.pubkey(), &mint_x)) > ux_before);
    assert!(token_balance(&svm, &get_associated_token_address(&config, &mint_y)) > vy_before);
}

#[test]
fn test_swap_slippage() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 42, 30);
    fund_user(&mut svm, &admin, &user, &mint_x, &mint_y);
    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 2_000_000, 4_000_000, 1), &user);

    let ix = ix_swap(&user.pubkey(), &mint_x, &mint_y, 42, true, 100_000, u64::MAX);
    assert!(send_fails(&mut svm, ix, &user), "must fail when min_out is impossible");
}

#[test]
fn test_swap_constant_product_maintained() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 42, 0);
    fund_user(&mut svm, &admin, &user, &mint_x, &mint_y);
    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 2_000_000, 4_000_000, 1), &user);

    let config = config_pda(42);
    let vx = get_associated_token_address(&config, &mint_x);
    let vy = get_associated_token_address(&config, &mint_y);
    let k_before = token_balance(&svm, &vx) as u128 * token_balance(&svm, &vy) as u128;

    send(&mut svm, ix_swap(&user.pubkey(), &mint_x, &mint_y, 42, true, 200_000, 1), &user);

    let k_after = token_balance(&svm, &vx) as u128 * token_balance(&svm, &vy) as u128;
    assert!(k_after >= k_before, "constant product k must be non-decreasing");
}

#[test]
fn test_second_deposit_proportional_lp() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 42, 30);
    fund_user(&mut svm, &admin, &user, &mint_x, &mint_y);
    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 2_000_000, 4_000_000, 1), &user);

    let config  = config_pda(42);
    let lp_mint = lp_mint_pda(&config);
    let supply_before = lp_supply(&svm, &lp_mint);

    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 42, 1_000_000, 2_000_000, 1), &user);

    let supply_after = lp_supply(&svm, &lp_mint);
    assert!(supply_after > supply_before, "LP supply should grow on second deposit");
    assert!(supply_after > supply_before, "proportional deposit should mint additional LP tokens");
}

#[test]
fn test_zero_fee_swap() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user  = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(),  10_000_000_000).unwrap();

    let (mint_x, mint_y) = init_pool(&mut svm, &admin, 99, 0);
    let config  = config_pda(99);
    let lp_mint = lp_mint_pda(&config);
    create_ata_mint_to(&mut svm, &admin, &mint_x, &admin, &user.pubkey(), 5_000_000);
    create_ata_mint_to(&mut svm, &admin, &mint_y, &admin, &user.pubkey(), 5_000_000);
    create_ata_empty(&mut svm, &user, &lp_mint, &user.pubkey());

    send(&mut svm, ix_add_liquidity(&user.pubkey(), &mint_x, &mint_y, 99, 1_000_000, 1_000_000, 1), &user);
    send(&mut svm, ix_swap(&user.pubkey(), &mint_x, &mint_y, 99, true, 100_000, 1), &user);

    let user_y = token_balance(&svm, &get_associated_token_address(&user.pubkey(), &mint_y));
    assert!(user_y > 4_000_000, "with zero fee should receive close to expected output");
}

#[test]
fn test_invalid_fee_rejected() {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    let mint_x = create_mint(&mut svm, &admin, 6);
    let mint_y = create_mint(&mut svm, &admin, 6);
    let ix = ix_initialize(&admin.pubkey(), &mint_x, &mint_y, 555, 10_001);
    assert!(send_fails(&mut svm, ix, &admin), "fee > 10000 must be rejected");
}
