//! LiteSVM integration tests for the anchor_escrow program.
//! Uses solana-program-3.x-compatible interface crates (no spl-token 7.x).

use anchor_lang::{
    solana_program::instruction::Instruction,
    AccountDeserialize, InstructionData, ToAccountMetas,
};
use anchor_escrow::{accounts as escrow_accounts, instruction as escrow_ix, Escrow};
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use spl_associated_token_account_interface::{
    address::get_associated_token_address,
    instruction::create_associated_token_account,
    program::id as ata_program_id,
};
use spl_token_interface::{instruction as token_ix, ID as TOKEN_PROGRAM_ID};

// ── constants ─────────────────────────────────────────────────────────────────

const PROGRAM_ID: Pubkey = anchor_escrow::ID;

// SPL Token Mint account size is always 82 bytes.
const MINT_SIZE: u64 = 82;

// ── helpers ───────────────────────────────────────────────────────────────────

fn system_id() -> Pubkey {
    solana_sdk_ids::system_program::ID
}

fn setup() -> (LiteSVM, Keypair, Keypair) {
    let mut svm = LiteSVM::new();
    let bytes = include_bytes!("../../../target/deploy/anchor_escrow.so");
    svm.add_program(PROGRAM_ID, bytes).unwrap();
    let maker = Keypair::new();
    let taker = Keypair::new();
    svm.airdrop(&maker.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&taker.pubkey(), 10_000_000_000).unwrap();
    (svm, maker, taker)
}

fn create_mint(svm: &mut LiteSVM, authority: &Keypair, decimals: u8) -> Pubkey {
    let mint_kp = Keypair::new();
    let rent = svm.minimum_balance_for_rent_exemption(MINT_SIZE as usize);
    let create_ix = anchor_lang::solana_program::system_instruction::create_account(
        &authority.pubkey(),
        &mint_kp.pubkey(),
        rent,
        MINT_SIZE,
        &TOKEN_PROGRAM_ID,
    );
    let init_ix = token_ix::initialize_mint(
        &TOKEN_PROGRAM_ID,
        &mint_kp.pubkey(),
        &authority.pubkey(),
        None,
        decimals,
    )
    .unwrap();
    send_multi(svm, &[create_ix, init_ix], authority, Some(&mint_kp));
    mint_kp.pubkey()
}

fn create_ata_and_mint_to(
    svm: &mut LiteSVM,
    payer: &Keypair,
    mint: &Pubkey,
    mint_authority: &Keypair,
    owner: &Pubkey,
    amount: u64,
) -> Pubkey {
    let ata = get_associated_token_address(owner, mint);
    let create_ix = create_associated_token_account(
        &payer.pubkey(),
        owner,
        mint,
        &TOKEN_PROGRAM_ID,
    );
    let mint_ix = token_ix::mint_to(
        &TOKEN_PROGRAM_ID,
        mint,
        &ata,
        &mint_authority.pubkey(),
        &[],
        amount,
    )
    .unwrap();
    send_multi(svm, &[create_ix, mint_ix], payer, Some(mint_authority));
    ata
}

/// Read the token balance from raw account bytes.
/// SPL Token account layout: [mint 32][owner 32][amount 8 LE][...]
fn token_balance(svm: &LiteSVM, ata: &Pubkey) -> u64 {
    let acc = svm.get_account(ata).expect("token account must exist");
    u64::from_le_bytes(acc.data[64..72].try_into().unwrap())
}

fn escrow_pda(maker: &Pubkey, seed: u64) -> Pubkey {
    Pubkey::find_program_address(
        &[b"escrow", maker.as_ref(), &seed.to_le_bytes()],
        &PROGRAM_ID,
    )
    .0
}

fn send(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    svm.send_transaction(tx).unwrap();
}

fn send_multi(svm: &mut LiteSVM, ixs: &[Instruction], payer: &Keypair, extra: Option<&Keypair>) {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(ixs, Some(&payer.pubkey()), &blockhash);
    let tx = if let Some(e) = extra {
        VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer, e]).unwrap()
    } else {
        VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[payer]).unwrap()
    };
    svm.send_transaction(tx).unwrap();
}

fn send_fails(svm: &mut LiteSVM, ix: Instruction, signer: &Keypair) -> bool {
    let blockhash = svm.latest_blockhash();
    let msg = Message::new_with_blockhash(&[ix], Some(&signer.pubkey()), &blockhash);
    let tx = VersionedTransaction::try_new(VersionedMessage::Legacy(msg), &[signer]).unwrap();
    svm.send_transaction(tx).is_err()
}

// ── instruction builders ──────────────────────────────────────────────────────

fn ix_make(
    maker: &Pubkey,
    mint_a: &Pubkey,
    mint_b: &Pubkey,
    seed: u64,
    deposit: u64,
    receive: u64,
) -> Instruction {
    let escrow = escrow_pda(maker, seed);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &escrow_ix::Make { seed, deposit, receive }.data(),
        escrow_accounts::Make {
            maker: *maker,
            mint_a: *mint_a,
            mint_b: *mint_b,
            maker_ata_a: get_associated_token_address(maker, mint_a),
            vault:        get_associated_token_address(&escrow, mint_a),
            escrow,
            system_program:           system_id(),
            token_program:            TOKEN_PROGRAM_ID,
            associated_token_program: ata_program_id(),
        }
        .to_account_metas(None),
    )
}

fn ix_take(taker: &Pubkey, maker: &Pubkey, mint_a: &Pubkey, mint_b: &Pubkey, seed: u64) -> Instruction {
    let escrow = escrow_pda(maker, seed);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &escrow_ix::Take { seed }.data(),
        escrow_accounts::Take {
            taker:    *taker,
            maker:    *maker,
            mint_a:   *mint_a,
            mint_b:   *mint_b,
            taker_ata_b: get_associated_token_address(taker, mint_b),
            taker_ata_a: get_associated_token_address(taker, mint_a),
            maker_ata_b: get_associated_token_address(maker, mint_b),
            vault:       get_associated_token_address(&escrow, mint_a),
            escrow,
            system_program: system_id(),
            token_program:  TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_refund(maker: &Pubkey, mint_a: &Pubkey, seed: u64) -> Instruction {
    let escrow = escrow_pda(maker, seed);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &escrow_ix::Refund { seed }.data(),
        escrow_accounts::Refund {
            maker:       *maker,
            mint_a:      *mint_a,
            maker_ata_a: get_associated_token_address(maker, mint_a),
            vault:       get_associated_token_address(&escrow, mint_a),
            escrow,
            system_program: system_id(),
            token_program:  TOKEN_PROGRAM_ID,
        }
        .to_account_metas(None),
    )
}

// ── fixture ───────────────────────────────────────────────────────────────────

struct Fixture {
    svm:    LiteSVM,
    maker:  Keypair,
    taker:  Keypair,
    mint_a: Pubkey,
    mint_b: Pubkey,
    seed:   u64,
}

impl Fixture {
    fn new(deposit: u64, receive: u64) -> Self {
        let (mut svm, maker, taker) = setup();

        let auth = Keypair::new();
        svm.airdrop(&auth.pubkey(), 2_000_000_000).unwrap();

        let mint_a = create_mint(&mut svm, &auth, 6);
        let mint_b = create_mint(&mut svm, &auth, 6);

        create_ata_and_mint_to(&mut svm, &maker, &mint_a, &auth, &maker.pubkey(), deposit + 1_000_000);
        create_ata_and_mint_to(&mut svm, &taker, &mint_b, &auth, &taker.pubkey(), receive + 1_000_000);

        // Pre-create receive ATAs so `take` doesn't need init_if_needed:
        // taker's ATA for mint_a (receives Token A)
        let taker_ata_a_ix = create_associated_token_account(
            &taker.pubkey(), &taker.pubkey(), &mint_a, &TOKEN_PROGRAM_ID,
        );
        send(&mut svm, taker_ata_a_ix, &taker);
        // maker's ATA for mint_b (receives Token B)
        let maker_ata_b_ix = create_associated_token_account(
            &maker.pubkey(), &maker.pubkey(), &mint_b, &TOKEN_PROGRAM_ID,
        );
        send(&mut svm, maker_ata_b_ix, &maker);

        let seed = 1337u64;
        send(&mut svm, ix_make(&maker.pubkey(), &mint_a, &mint_b, seed, deposit, receive), &maker);

        Fixture { svm, maker, taker, mint_a, mint_b, seed }
    }
}

// ── make tests ────────────────────────────────────────────────────────────────

#[test]
fn test_make_stores_escrow_state() {
    let f = Fixture::new(500_000, 1_000_000);
    let escrow_key = escrow_pda(&f.maker.pubkey(), f.seed);
    let acc = f.svm.get_account(&escrow_key).expect("escrow must exist");
    let escrow: Escrow = Escrow::try_deserialize(&mut &acc.data[..]).unwrap();

    assert_eq!(escrow.maker,   f.maker.pubkey(), "maker stored correctly");
    assert_eq!(escrow.mint_a,  f.mint_a, "mint_a stored correctly");
    assert_eq!(escrow.mint_b,  f.mint_b, "mint_b stored correctly");
    assert_eq!(escrow.receive, 1_000_000, "receive stored correctly");
    assert_eq!(escrow.seed,    f.seed, "seed stored correctly");
}

#[test]
fn test_make_deposits_tokens_into_vault() {
    let deposit = 500_000u64;
    let f = Fixture::new(deposit, 1_000_000);
    let vault = get_associated_token_address(&escrow_pda(&f.maker.pubkey(), f.seed), &f.mint_a);
    assert_eq!(token_balance(&f.svm, &vault), deposit, "vault holds the deposited amount");
}

// ── take tests ────────────────────────────────────────────────────────────────

#[test]
fn test_take_transfers_tokens_both_ways() {
    let deposit = 500_000u64;
    let receive = 1_000_000u64;
    let mut f = Fixture::new(deposit, receive);

    let taker_b_before = token_balance(&f.svm, &get_associated_token_address(&f.taker.pubkey(), &f.mint_b));

    send(&mut f.svm, ix_take(&f.taker.pubkey(), &f.maker.pubkey(), &f.mint_a, &f.mint_b, f.seed), &f.taker);

    let taker_ata_a = get_associated_token_address(&f.taker.pubkey(), &f.mint_a);
    let taker_ata_b = get_associated_token_address(&f.taker.pubkey(), &f.mint_b);

    assert_eq!(token_balance(&f.svm, &taker_ata_a), deposit, "taker receives mint_a tokens");
    assert_eq!(taker_b_before - token_balance(&f.svm, &taker_ata_b), receive, "taker spends mint_b tokens");
}

#[test]
fn test_take_sends_mint_b_to_maker() {
    let receive = 1_000_000u64;
    let mut f = Fixture::new(500_000, receive);

    send(&mut f.svm, ix_take(&f.taker.pubkey(), &f.maker.pubkey(), &f.mint_a, &f.mint_b, f.seed), &f.taker);

    let maker_ata_b = get_associated_token_address(&f.maker.pubkey(), &f.mint_b);
    assert_eq!(token_balance(&f.svm, &maker_ata_b), receive, "maker receives agreed receive amount");
}

#[test]
fn test_take_closes_escrow_and_vault() {
    let mut f = Fixture::new(500_000, 1_000_000);
    let escrow_key = escrow_pda(&f.maker.pubkey(), f.seed);
    let vault = get_associated_token_address(&escrow_key, &f.mint_a);

    send(&mut f.svm, ix_take(&f.taker.pubkey(), &f.maker.pubkey(), &f.mint_a, &f.mint_b, f.seed), &f.taker);

    let esc = f.svm.get_account(&escrow_key);
    assert!(esc.is_none() || esc.unwrap().lamports == 0, "escrow closed after take");
    let v = f.svm.get_account(&vault);
    assert!(v.is_none() || v.unwrap().lamports == 0, "vault closed after take");
}

// ── refund tests ──────────────────────────────────────────────────────────────

#[test]
fn test_refund_returns_tokens_to_maker() {
    let deposit = 500_000u64;
    let mut f = Fixture::new(deposit, 1_000_000);

    let maker_ata_a = get_associated_token_address(&f.maker.pubkey(), &f.mint_a);
    let before = token_balance(&f.svm, &maker_ata_a);

    send(&mut f.svm, ix_refund(&f.maker.pubkey(), &f.mint_a, f.seed), &f.maker);

    assert_eq!(token_balance(&f.svm, &maker_ata_a) - before, deposit, "maker gets deposit back");
}

#[test]
fn test_refund_closes_escrow_and_vault() {
    let mut f = Fixture::new(500_000, 1_000_000);
    let escrow_key = escrow_pda(&f.maker.pubkey(), f.seed);
    let vault = get_associated_token_address(&escrow_key, &f.mint_a);

    send(&mut f.svm, ix_refund(&f.maker.pubkey(), &f.mint_a, f.seed), &f.maker);

    let esc = f.svm.get_account(&escrow_key);
    assert!(esc.is_none() || esc.unwrap().lamports == 0, "escrow closed after refund");
    let v = f.svm.get_account(&vault);
    assert!(v.is_none() || v.unwrap().lamports == 0, "vault closed after refund");
}

#[test]
fn test_taker_cannot_take_after_refund() {
    let mut f = Fixture::new(500_000, 1_000_000);
    send(&mut f.svm, ix_refund(&f.maker.pubkey(), &f.mint_a, f.seed), &f.maker);
    assert!(
        send_fails(&mut f.svm, ix_take(&f.taker.pubkey(), &f.maker.pubkey(), &f.mint_a, &f.mint_b, f.seed), &f.taker),
        "take should fail after refund"
    );
}

// ── multi-escrow test ─────────────────────────────────────────────────────────

#[test]
fn test_multiple_escrows_same_maker() {
    let (mut svm, maker, _taker) = setup();

    let auth = Keypair::new();
    svm.airdrop(&auth.pubkey(), 5_000_000_000).unwrap();

    let mint_a = create_mint(&mut svm, &auth, 6);
    let mint_b = create_mint(&mut svm, &auth, 6);
    create_ata_and_mint_to(&mut svm, &maker, &mint_a, &auth, &maker.pubkey(), 1_000_000);

    for seed in [1u64, 2, 3] {
        send(&mut svm, ix_make(&maker.pubkey(), &mint_a, &mint_b, seed, 100_000, 200_000), &maker);

        let escrow_key = escrow_pda(&maker.pubkey(), seed);
        let acc = svm.get_account(&escrow_key).unwrap();
        let escrow: Escrow = Escrow::try_deserialize(&mut &acc.data[..]).unwrap();
        assert_eq!(escrow.seed, seed, "each escrow has its own seed");
    }
}
