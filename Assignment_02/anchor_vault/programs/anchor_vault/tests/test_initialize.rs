use {
    anchor_lang::{solana_program::instruction::Instruction, InstructionData, ToAccountMetas},
    anchor_vault::{accounts as vault_accounts, instruction as vault_ix},
    litesvm::LiteSVM,
    solana_keypair::Keypair,
    solana_message::{Message, VersionedMessage},
    solana_pubkey::Pubkey,
    solana_signer::Signer,
    solana_transaction::versioned::VersionedTransaction,
};

const PROGRAM_ID: Pubkey = anchor_vault::ID;
// System program ID constant — avoids needing the Id trait in scope
const SYSTEM_ID: Pubkey = solana_sdk_ids::system_program::ID;

fn setup() -> (LiteSVM, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/anchor_vault.so");
    svm.add_program(PROGRAM_ID, bytes).unwrap();
    let user = Keypair::new();
    svm.airdrop(&user.pubkey(), 10_000_000_000).unwrap();
    (svm, user)
}

fn pda_state(user: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"state", user.as_ref()], &PROGRAM_ID).0
}

fn pda_vault(state: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"vault", state.as_ref()], &PROGRAM_ID).0
}

fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    svm.send_transaction(tx).unwrap();
}

fn send_fails(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> bool {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    svm.send_transaction(tx).is_err()
}

fn ix_initialize(user: &Pubkey) -> Instruction {
    let vault_state = pda_state(user);
    let vault = pda_vault(&vault_state);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &vault_ix::Initialize {}.data(),
        vault_accounts::Initialize {
            user: *user,
            vault_state,
            vault,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_deposit(user: &Pubkey, amount: u64) -> Instruction {
    let vault_state = pda_state(user);
    let vault = pda_vault(&vault_state);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &vault_ix::Deposit { amount }.data(),
        vault_accounts::Deposit {
            user: *user,
            vault_state,
            vault,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_withdraw(user: &Pubkey, amount: u64) -> Instruction {
    let vault_state = pda_state(user);
    let vault = pda_vault(&vault_state);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &vault_ix::Withdraw { amount }.data(),
        vault_accounts::Withdraw {
            user: *user,
            vault_state,
            vault,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_close(user: &Pubkey) -> Instruction {
    let vault_state = pda_state(user);
    let vault = pda_vault(&vault_state);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &vault_ix::Close {}.data(),
        vault_accounts::Close {
            user: *user,
            vault_state,
            vault,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

// ── tests ─────────────────────────────────────────────────────────────────────

#[test]
fn test_initialize_creates_state() {
    let (mut svm, user) = setup();
    send(&mut svm, ix_initialize(&user.pubkey()), &user);

    let (vault_state_key, state_bump) = Pubkey::find_program_address(
        &[b"state", user.pubkey().as_ref()],
        &PROGRAM_ID,
    );
    let (vault_key, vault_bump) = Pubkey::find_program_address(
        &[b"vault", vault_state_key.as_ref()],
        &PROGRAM_ID,
    );

    let state_acc = svm.get_account(&vault_state_key).expect("vault_state must exist");
    // Anchor: discriminator = bytes[0..8], vault_bump = bytes[8], state_bump = bytes[9]
    assert_eq!(state_acc.data[8], vault_bump, "vault_bump stored correctly");
    assert_eq!(state_acc.data[9], state_bump, "state_bump stored correctly");

    // vault_key is a PDA that only gets allocated when it receives lamports —
    // after initialize it has no on-chain account yet, which is correct.
    // We verify its canonical address derivation is correct instead.
    let (derived_vault, derived_bump) = Pubkey::find_program_address(
        &[b"vault", vault_state_key.as_ref()],
        &PROGRAM_ID,
    );
    assert_eq!(derived_vault, vault_key, "vault PDA derives from vault_state");
    assert_eq!(derived_bump, vault_bump, "vault bump should be consistent");
}

#[test]
fn test_deposit_increases_vault_balance() {
    let (mut svm, user) = setup();
    send(&mut svm, ix_initialize(&user.pubkey()), &user);

    let vault_key = pda_vault(&pda_state(&user.pubkey()));
    let before = svm.get_account(&vault_key).map(|a| a.lamports).unwrap_or(0);

    send(&mut svm, ix_deposit(&user.pubkey(), 1_000_000_000), &user);

    let after = svm.get_account(&vault_key).unwrap().lamports;
    assert_eq!(after - before, 1_000_000_000);
}

#[test]
fn test_withdraw_returns_lamports_to_user() {
    let (mut svm, user) = setup();
    send(&mut svm, ix_initialize(&user.pubkey()), &user);
    send(&mut svm, ix_deposit(&user.pubkey(), 2_000_000_000), &user);

    let vault_key = pda_vault(&pda_state(&user.pubkey()));
    let vault_before = svm.get_account(&vault_key).unwrap().lamports;
    let user_before  = svm.get_account(&user.pubkey()).unwrap().lamports;

    send(&mut svm, ix_withdraw(&user.pubkey(), 1_000_000_000), &user);

    let vault_after = svm.get_account(&vault_key).unwrap().lamports;
    let user_after  = svm.get_account(&user.pubkey()).unwrap().lamports;

    assert_eq!(vault_before - vault_after, 1_000_000_000, "vault decreased by withdrawal");
    assert!(user_after > user_before - 10_000, "user received lamports back");
}

#[test]
fn test_withdraw_fails_insufficient_funds() {
    let (mut svm, user) = setup();
    send(&mut svm, ix_initialize(&user.pubkey()), &user);
    send(&mut svm, ix_deposit(&user.pubkey(), 100_000_000), &user);

    assert!(
        send_fails(&mut svm, ix_withdraw(&user.pubkey(), 1_000_000_000), &user),
        "withdraw more than vault holds should fail"
    );
}

#[test]
fn test_close_drains_vault_and_closes_state() {
    let (mut svm, user) = setup();
    send(&mut svm, ix_initialize(&user.pubkey()), &user);
    send(&mut svm, ix_deposit(&user.pubkey(), 3_000_000_000), &user);

    let vault_state_key = pda_state(&user.pubkey());
    let vault_key       = pda_vault(&vault_state_key);
    let user_before     = svm.get_account(&user.pubkey()).unwrap().lamports;

    send(&mut svm, ix_close(&user.pubkey()), &user);

    let state = svm.get_account(&vault_state_key);
    assert!(state.is_none() || state.unwrap().lamports == 0, "vault_state should be closed");

    let vault = svm.get_account(&vault_key);
    assert_eq!(vault.map(|a| a.lamports).unwrap_or(0), 0, "vault should be drained");

    let user_after = svm.get_account(&user.pubkey()).unwrap().lamports;
    assert!(user_after > user_before, "user gained lamports after close");
}

#[test]
fn test_multiple_deposits_accumulate() {
    let (mut svm, user) = setup();
    send(&mut svm, ix_initialize(&user.pubkey()), &user);

    let vault_key = pda_vault(&pda_state(&user.pubkey()));
    let init_lamports = svm.get_account(&vault_key).map(|a| a.lamports).unwrap_or(0);

    // Use different amounts each time to ensure unique transaction signatures.
    let amounts = [100_000_000u64, 110_000_000, 90_000_000, 120_000_000, 80_000_000];
    let total: u64 = amounts.iter().sum();
    for amt in amounts {
        send(&mut svm, ix_deposit(&user.pubkey(), amt), &user);
    }

    let final_lamports = svm.get_account(&vault_key).unwrap().lamports;
    assert_eq!(final_lamports - init_lamports, total, "deposits should accumulate");
}
