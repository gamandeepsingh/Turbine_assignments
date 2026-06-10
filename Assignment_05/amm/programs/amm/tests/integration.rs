/// Integration tests for the constant-product AMM program.
///
/// Challenge 2: instruction introspection to link `burn_lp` and `withdraw`.
/// The key test is that `withdraw` works ONLY when `burn_lp` is the
/// immediately preceding instruction in the same transaction.
use amm::{accounts as amm_accounts, instruction as amm_ix, state::Pool};
use anchor_lang::{AccountDeserialize, InstructionData, ToAccountMetas};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use spl_token_interface::ID as TOKEN_ID;
use std::path::PathBuf;

use anchor_lang::solana_program::{
    instruction::{AccountMeta, Instruction},
    system_instruction,
};

const PROGRAM_ID: Pubkey = amm::ID;
const INSTRUCTIONS_SYSVAR_ID: Pubkey =
    solana_pubkey::pubkey!("Sysvar1nstructions1111111111111111111111111");
const SYSTEM_ID: Pubkey = solana_sdk_ids::system_program::ID;

// ─── binary loaders ───────────────────────────────────────────────────────────

fn amm_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy/amm.so");
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!("amm.so not found — run `cargo build-sbf` first")
    })
}

fn fresh_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    svm.add_program(PROGRAM_ID, &amm_bytes()).unwrap();
    svm
}

// ─── transaction helpers ──────────────────────────────────────────────────────

fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &bh);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    svm.send_transaction(tx).expect("transaction failed");
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
    svm.send_transaction(tx).expect("multi-signer tx failed");
}

fn send_fails(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, extra: &[&Keypair]) -> bool {
    let bh = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &bh);
    let mut signers: Vec<&Keypair> = vec![payer];
    for e in extra {
        if e.pubkey() != payer.pubkey() {
            signers.push(e);
        }
    }
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &signers).unwrap();
    svm.send_transaction(tx).is_err()
}

// ─── SPL Token helpers (raw instruction bytes, no extra crate needed) ─────────

/// Create a token mint (allocate + InitializeMint2).
fn create_mint(svm: &mut LiteSVM, payer: &Keypair, decimals: u8) -> Keypair {
    let mint_kp = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(82);

    let create_ix = system_instruction::create_account(
        &payer.pubkey(),
        &mint_kp.pubkey(),
        rent,
        82,
        &TOKEN_ID,
    );

    // InitializeMint2 = opcode 20
    let mut data = vec![20u8];
    data.push(decimals);
    data.extend_from_slice(payer.pubkey().as_ref()); // mint authority = payer
    data.push(0u8); // no freeze authority
    let init_ix = Instruction {
        program_id: TOKEN_ID,
        accounts: vec![AccountMeta::new(mint_kp.pubkey(), false)],
        data,
    };

    send_multi(svm, &[create_ix, init_ix], payer, &[&mint_kp]);
    mint_kp
}

/// Create a token account (allocate + InitializeAccount3).
fn create_token_account(svm: &mut LiteSVM, payer: &Keypair, mint: &Pubkey, owner: &Pubkey) -> Keypair {
    let acc_kp = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(165);

    let create_ix = system_instruction::create_account(
        &payer.pubkey(),
        &acc_kp.pubkey(),
        rent,
        165,
        &TOKEN_ID,
    );

    // InitializeAccount3 = opcode 18
    let mut data = vec![18u8];
    data.extend_from_slice(owner.as_ref());
    let init_ix = Instruction {
        program_id: TOKEN_ID,
        accounts: vec![
            AccountMeta::new(acc_kp.pubkey(), false),
            AccountMeta::new_readonly(*mint, false),
        ],
        data,
    };

    send_multi(svm, &[create_ix, init_ix], payer, &[&acc_kp]);
    acc_kp
}

/// Mint tokens to a token account (MintTo = opcode 7).
fn mint_tokens(svm: &mut LiteSVM, mint_authority: &Keypair, mint: &Pubkey, dest: &Pubkey, amount: u64) {
    let mut data = vec![7u8];
    data.extend_from_slice(&amount.to_le_bytes());
    let ix = Instruction {
        program_id: TOKEN_ID,
        accounts: vec![
            AccountMeta::new(*mint, false),
            AccountMeta::new(*dest, false),
            AccountMeta::new_readonly(mint_authority.pubkey(), true),
        ],
        data,
    };
    send(svm, ix, mint_authority);
}

fn token_balance(svm: &LiteSVM, account: &Pubkey) -> u64 {
    let data = svm.get_account(account).expect("token account not found").data;
    u64::from_le_bytes(data[64..72].try_into().unwrap())
}

// ─── PDA helpers ──────────────────────────────────────────────────────────────

fn pool_pda(mint_a: &Pubkey, mint_b: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"pool", mint_a.as_ref(), mint_b.as_ref()], &PROGRAM_ID)
}

fn vault_a_pda(pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault_a", pool.as_ref()], &PROGRAM_ID)
}

fn vault_b_pda(pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"vault_b", pool.as_ref()], &PROGRAM_ID)
}

fn lp_mint_pda(pool: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"lp_mint", pool.as_ref()], &PROGRAM_ID)
}

fn lp_account_pda(lp_mint: &Pubkey, user: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"lp_account", lp_mint.as_ref(), user.as_ref()], &PROGRAM_ID)
}

// ─── instruction builders ─────────────────────────────────────────────────────

fn ix_initialize(depositor: &Pubkey, mint_a: &Pubkey, mint_b: &Pubkey, initial_a: u64, initial_b: u64,
    depositor_a: &Pubkey, depositor_b: &Pubkey) -> Instruction {
    let (pool, _) = pool_pda(mint_a, mint_b);
    let (vault_a, _) = vault_a_pda(&pool);
    let (vault_b, _) = vault_b_pda(&pool);
    let (lp_mint, _) = lp_mint_pda(&pool);
    let (depositor_lp, _) = lp_account_pda(&lp_mint, depositor);

    Instruction::new_with_bytes(
        PROGRAM_ID,
        &amm_ix::Initialize { initial_a, initial_b }.data(),
        amm_accounts::Initialize {
            depositor: *depositor,
            token_a_mint: *mint_a,
            token_b_mint: *mint_b,
            pool,
            vault_a,
            vault_b,
            lp_mint,
            depositor_a: *depositor_a,
            depositor_b: *depositor_b,
            depositor_lp,
            token_program: TOKEN_ID,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_deposit(depositor: &Pubkey, mint_a: &Pubkey, mint_b: &Pubkey, amount_a: u64, amount_b: u64,
    min_lp: u64, depositor_a: &Pubkey, depositor_b: &Pubkey) -> Instruction {
    let (pool, _) = pool_pda(mint_a, mint_b);
    let (vault_a, _) = vault_a_pda(&pool);
    let (vault_b, _) = vault_b_pda(&pool);
    let (lp_mint, _) = lp_mint_pda(&pool);
    let (depositor_lp, _) = lp_account_pda(&lp_mint, depositor);

    Instruction::new_with_bytes(
        PROGRAM_ID,
        &amm_ix::Deposit { amount_a, amount_b, min_lp }.data(),
        amm_accounts::Deposit {
            depositor: *depositor,
            pool,
            vault_a,
            vault_b,
            lp_mint,
            depositor_a: *depositor_a,
            depositor_b: *depositor_b,
            depositor_lp,
            token_program: TOKEN_ID,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_swap(user: &Pubkey, mint_a: &Pubkey, mint_b: &Pubkey, amount_in: u64, min_out: u64,
    a_to_b: bool, user_in: &Pubkey, user_out: &Pubkey) -> Instruction {
    let (pool, _) = pool_pda(mint_a, mint_b);
    let (vault_a, _) = vault_a_pda(&pool);
    let (vault_b, _) = vault_b_pda(&pool);
    let (vault_in, vault_out) = if a_to_b { (vault_a, vault_b) } else { (vault_b, vault_a) };

    Instruction::new_with_bytes(
        PROGRAM_ID,
        &amm_ix::Swap { amount_in, min_out, a_to_b }.data(),
        amm_accounts::Swap {
            user: *user,
            pool,
            vault_in,
            vault_out,
            user_in: *user_in,
            user_out: *user_out,
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_burn_lp(user: &Pubkey, mint_a: &Pubkey, mint_b: &Pubkey, lp_amount: u64) -> Instruction {
    let (pool, _) = pool_pda(mint_a, mint_b);
    let (lp_mint, _) = lp_mint_pda(&pool);
    let (user_lp, _) = lp_account_pda(&lp_mint, user);

    Instruction::new_with_bytes(
        PROGRAM_ID,
        &amm_ix::BurnLp { lp_amount }.data(),
        amm_accounts::BurnLp {
            user: *user,
            user_lp,
            lp_mint,
            pool,
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_withdraw(user: &Pubkey, mint_a: &Pubkey, mint_b: &Pubkey, user_a: &Pubkey, user_b: &Pubkey,
    min_a: u64, min_b: u64) -> Instruction {
    let (pool, _) = pool_pda(mint_a, mint_b);
    let (vault_a, _) = vault_a_pda(&pool);
    let (vault_b, _) = vault_b_pda(&pool);

    Instruction::new_with_bytes(
        PROGRAM_ID,
        &amm_ix::Withdraw { min_a, min_b }.data(),
        amm_accounts::Withdraw {
            user: *user,
            pool,
            vault_a,
            vault_b,
            user_a: *user_a,
            user_b: *user_b,
            instructions: INSTRUCTIONS_SYSVAR_ID,
            token_program: TOKEN_ID,
        }
        .to_account_metas(None),
    )
}

// ─── test fixture ─────────────────────────────────────────────────────────────

struct Fixture {
    svm: LiteSVM,
    admin: Keypair,
    user: Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
    pool: Pubkey,
    lp_mint: Pubkey,
    admin_a: Pubkey,
    admin_b: Pubkey,
    admin_lp: Pubkey, // PDA [b"lp_account", lp_mint, admin]
    user_a: Pubkey,
    user_b: Pubkey,
    user_lp: Pubkey, // PDA [b"lp_account", lp_mint, user]
}

fn setup() -> Fixture {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let user = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();

    // Create mints (admin is mint authority).
    let mint_a_kp = create_mint(&mut svm, &admin, 6);
    let mint_b_kp = create_mint(&mut svm, &admin, 6);
    // Sort mints so pool PDA is deterministic (smaller pubkey first).
    let (mint_a, mint_b) = {
        let a = mint_a_kp.pubkey();
        let b = mint_b_kp.pubkey();
        if a < b { (a, b) } else { (b, a) }
    };

    // Create token A and B accounts.
    let admin_a_kp = create_token_account(&mut svm, &admin, &mint_a, &admin.pubkey());
    let admin_b_kp = create_token_account(&mut svm, &admin, &mint_b, &admin.pubkey());
    let user_a_kp = create_token_account(&mut svm, &admin, &mint_a, &user.pubkey());
    let user_b_kp = create_token_account(&mut svm, &admin, &mint_b, &user.pubkey());

    mint_tokens(&mut svm, &admin, &mint_a, &admin_a_kp.pubkey(), 1_000_000_000_000);
    mint_tokens(&mut svm, &admin, &mint_b, &admin_b_kp.pubkey(), 1_000_000_000_000);
    mint_tokens(&mut svm, &admin, &mint_a, &user_a_kp.pubkey(), 500_000_000_000);
    mint_tokens(&mut svm, &admin, &mint_b, &user_b_kp.pubkey(), 500_000_000_000);

    // Derive pool + LP PDAs.
    let (pool, _) = pool_pda(&mint_a, &mint_b);
    let (lp_mint, _) = lp_mint_pda(&pool);
    let (admin_lp, _) = lp_account_pda(&lp_mint, &admin.pubkey());
    let (user_lp, _) = lp_account_pda(&lp_mint, &user.pubkey());

    // Initialize pool — creates vaults, lp_mint PDA, and admin's lp_account PDA.
    send(
        &mut svm,
        ix_initialize(
            &admin.pubkey(), &mint_a, &mint_b,
            100_000_000, 100_000_000,
            &admin_a_kp.pubkey(), &admin_b_kp.pubkey(),
        ),
        &admin,
    );

    Fixture {
        svm, admin, user,
        mint_a, mint_b, pool, lp_mint,
        admin_a: admin_a_kp.pubkey(),
        admin_b: admin_b_kp.pubkey(),
        admin_lp,
        user_a: user_a_kp.pubkey(),
        user_b: user_b_kp.pubkey(),
        user_lp,
    }
}

fn read_pool(svm: &LiteSVM, pool: &Pubkey) -> Pool {
    let data = svm.get_account(pool).expect("pool not found").data;
    Pool::try_deserialize(&mut &data[..]).unwrap()
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_creates_pool_and_mints_lp() {
    let f = setup();
    let pool = read_pool(&f.svm, &f.pool);
    assert_eq!(pool.token_a_mint, f.mint_a);
    assert_eq!(pool.token_b_mint, f.mint_b);
    // LP supply = sqrt(100M) * sqrt(100M) = 10_000 * 10_000 = 100_000_000
    assert_eq!(pool.lp_supply, 100_000_000);
    assert_eq!(token_balance(&f.svm, &f.admin_lp), 100_000_000);
}

#[test]
fn test_deposit_mints_proportional_lp() {
    let mut f = setup();
    let pool_before = read_pool(&f.svm, &f.pool);

    send(
        &mut f.svm,
        ix_deposit(
            &f.user.pubkey(), &f.mint_a, &f.mint_b,
            50_000_000, 50_000_000, 0,
            &f.user_a, &f.user_b,
        ),
        &f.user,
    );

    let pool_after = read_pool(&f.svm, &f.pool);
    // LP minted = min(50M * 100M / 100M, same) = 50M
    assert_eq!(pool_after.lp_supply, pool_before.lp_supply + 50_000_000);
    assert_eq!(token_balance(&f.svm, &f.user_lp), 50_000_000);
}

#[test]
fn test_swap_a_to_b_constant_product() {
    let mut f = setup();
    let user_b_before = token_balance(&f.svm, &f.user_b);

    // Swap 1M of token A for token B.
    let amount_in = 1_000_000u64;
    send(
        &mut f.svm,
        ix_swap(
            &f.user.pubkey(), &f.mint_a, &f.mint_b,
            amount_in, 1, true,
            &f.user_a, &f.user_b,
        ),
        &f.user,
    );

    let user_b_after = token_balance(&f.svm, &f.user_b);
    let received = user_b_after - user_b_before;
    // Expected ≈ (1M * 997 * 100M) / (100M * 1000 + 1M * 997) = ~990_019
    assert!(received > 900_000 && received < 1_000_000, "unexpected swap output: {received}");
}

#[test]
fn test_swap_b_to_a_constant_product() {
    let mut f = setup();
    let user_a_before = token_balance(&f.svm, &f.user_a);

    send(
        &mut f.svm,
        ix_swap(
            &f.user.pubkey(), &f.mint_a, &f.mint_b,
            1_000_000, 1, false,
            &f.user_b, &f.user_a,
        ),
        &f.user,
    );

    let user_a_after = token_balance(&f.svm, &f.user_a);
    assert!(user_a_after > user_a_before);
}

#[test]
fn test_swap_slippage_exceeded_fails() {
    let mut f = setup();
    // Request an unrealistically high min_out.
    let ix = ix_swap(
        &f.user.pubkey(), &f.mint_a, &f.mint_b,
        1_000_000, 999_999_999, true,
        &f.user_a, &f.user_b,
    );
    assert!(send_fails(&mut f.svm, &[ix], &f.user, &[]));
}

#[test]
fn test_burn_and_withdraw_in_same_tx() {
    let mut f = setup();

    // Give the user LP tokens by depositing.
    send(
        &mut f.svm,
        ix_deposit(
            &f.user.pubkey(), &f.mint_a, &f.mint_b,
            50_000_000, 50_000_000, 0,
            &f.user_a, &f.user_b,
        ),
        &f.user,
    );
    let lp_bal = token_balance(&f.svm, &f.user_lp);
    assert!(lp_bal > 0, "user should have LP tokens");

    let user_a_before = token_balance(&f.svm, &f.user_a);
    let user_b_before = token_balance(&f.svm, &f.user_b);

    // THE KEY TEST: tx = [burn_lp, withdraw] — withdraw uses instruction introspection.
    let burn_ix = ix_burn_lp(&f.user.pubkey(), &f.mint_a, &f.mint_b, lp_bal);
    let withdraw_ix = ix_withdraw(
        &f.user.pubkey(), &f.mint_a, &f.mint_b,
        &f.user_a, &f.user_b, 1, 1,
    );
    send_multi(&mut f.svm, &[burn_ix, withdraw_ix], &f.user, &[]);

    let user_a_after = token_balance(&f.svm, &f.user_a);
    let user_b_after = token_balance(&f.svm, &f.user_b);
    assert!(user_a_after > user_a_before, "user should receive token A");
    assert!(user_b_after > user_b_before, "user should receive token B");

    // LP supply decremented.
    let pool = read_pool(&f.svm, &f.pool);
    assert_eq!(pool.lp_supply, 100_000_000, "only admin's LP should remain");
}

#[test]
fn test_withdraw_without_burn_fails() {
    let mut f = setup();

    send(
        &mut f.svm,
        ix_deposit(
            &f.user.pubkey(), &f.mint_a, &f.mint_b,
            50_000_000, 50_000_000, 0,
            &f.user_a, &f.user_b,
        ),
        &f.user,
    );

    // withdraw alone (no burn_lp preceding it) must fail.
    let withdraw_ix = ix_withdraw(
        &f.user.pubkey(), &f.mint_a, &f.mint_b,
        &f.user_a, &f.user_b, 0, 0,
    );
    assert!(
        send_fails(&mut f.svm, &[withdraw_ix], &f.user, &[]),
        "withdraw without burn_lp must fail"
    );
}

#[test]
fn test_withdraw_wrong_user_burn_fails() {
    let mut f = setup();
    let other = Keypair::new();
    f.svm.airdrop(&other.pubkey(), 5_000_000_000).unwrap();

    // Both user and other deposit.
    send(
        &mut f.svm,
        ix_deposit(
            &f.user.pubkey(), &f.mint_a, &f.mint_b,
            50_000_000, 50_000_000, 0,
            &f.user_a, &f.user_b,
        ),
        &f.user,
    );

    let other_a_kp = create_token_account(&mut f.svm, &f.admin, &f.mint_a, &other.pubkey());
    let other_b_kp = create_token_account(&mut f.svm, &f.admin, &f.mint_b, &other.pubkey());
    mint_tokens(&mut f.svm, &f.admin, &f.mint_a, &other_a_kp.pubkey(), 200_000_000);
    mint_tokens(&mut f.svm, &f.admin, &f.mint_b, &other_b_kp.pubkey(), 200_000_000);
    send(
        &mut f.svm,
        ix_deposit(
            &other.pubkey(), &f.mint_a, &f.mint_b,
            50_000_000, 50_000_000, 0,
            &other_a_kp.pubkey(), &other_b_kp.pubkey(),
        ),
        &other,
    );

    let (_, lp_mint, other_lp, _) = {
        let (pool, _) = pool_pda(&f.mint_a, &f.mint_b);
        let (lp_mint, _) = lp_mint_pda(&pool);
        let (other_lp, _) = lp_account_pda(&lp_mint, &other.pubkey());
        (pool, lp_mint, other_lp, ())
    };
    let other_lp_bal = token_balance(&f.svm, &other_lp);

    // "other" burns their LP but "user" tries to withdraw — must fail (wrong user).
    let burn_ix = ix_burn_lp(&other.pubkey(), &f.mint_a, &f.mint_b, other_lp_bal);
    let withdraw_ix = ix_withdraw(
        &f.user.pubkey(), &f.mint_a, &f.mint_b,
        &f.user_a, &f.user_b, 0, 0,
    );
    assert!(
        send_fails(&mut f.svm, &[burn_ix, withdraw_ix], &f.user, &[&other]),
        "withdraw should fail when burn was from a different user"
    );
}

#[test]
fn test_deposit_slippage_fails() {
    let mut f = setup();
    let ix = ix_deposit(
        &f.user.pubkey(), &f.mint_a, &f.mint_b,
        1_000, 1_000, 999_999_999,
        &f.user_a, &f.user_b,
    );
    assert!(send_fails(&mut f.svm, &[ix], &f.user, &[]));
}
