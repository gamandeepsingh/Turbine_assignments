use anchor_lang::{
    solana_program::instruction::Instruction, AccountDeserialize, InstructionData, ToAccountMetas,
};
use litesvm::LiteSVM;
use nft_marketplace::{
    accounts as mp_accounts, instruction as mp_ix, state::{Listing, Offer},
};
use solana_keypair::Keypair;
use solana_message::{Message, VersionedMessage};
use solana_pubkey::Pubkey;
use solana_signer::Signer;
use solana_transaction::versioned::VersionedTransaction;
use std::path::PathBuf;

const PROGRAM_ID: Pubkey = nft_marketplace::ID;
const MPL_CORE_ID: Pubkey = solana_pubkey::pubkey!("CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d");
const SYSTEM_ID: Pubkey = solana_sdk_ids::system_program::ID;

// ─── binary loaders ───────────────────────────────────────────────────────────

fn marketplace_bytes() -> Vec<u8> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../target/deploy/nft_marketplace.so");
    std::fs::read(&path).unwrap_or_else(|_| {
        panic!(
            "nft_marketplace.so not found at {path:?}. \
             Run `cargo build-sbf` first."
        )
    })
}

fn mpl_core_bytes() -> Vec<u8> {
    if let Ok(p) = std::env::var("MPL_CORE_SO") {
        return std::fs::read(&p).unwrap_or_else(|_| panic!("MPL_CORE_SO={p} not readable"));
    }
    let fixture = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/programs/mpl_core.so");
    if fixture.exists() {
        return std::fs::read(&fixture).expect("failed to read mpl_core.so");
    }
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
        .expect("solana CLI not found");
    assert!(status.success(), "Failed to download MPL Core binary.");
    std::fs::read(&fixture).unwrap()
}

fn fresh_svm() -> LiteSVM {
    let mut svm = LiteSVM::new();
    svm.add_program(PROGRAM_ID, &marketplace_bytes()).unwrap();
    svm.add_program(MPL_CORE_ID, &mpl_core_bytes()).unwrap();
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
    svm.send_transaction(tx).expect("multi-signer transaction failed");
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

// ─── PDA helpers ──────────────────────────────────────────────────────────────

fn marketplace_pda(name: &str) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"marketplace", name.as_bytes()], &PROGRAM_ID)
}

fn treasury_pda(marketplace: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"treasury", marketplace.as_ref()], &PROGRAM_ID)
}

fn listing_pda(marketplace: &Pubkey, asset: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(
        &[b"listing", marketplace.as_ref(), asset.as_ref()],
        &PROGRAM_ID,
    )
}

fn offer_pda(asset: &Pubkey, buyer: &Pubkey) -> (Pubkey, u8) {
    Pubkey::find_program_address(&[b"offer", asset.as_ref(), buyer.as_ref()], &PROGRAM_ID)
}

// ─── MPL Core helpers ─────────────────────────────────────────────────────────

fn create_collection(svm: &mut LiteSVM, payer: &Keypair) -> Pubkey {
    use mpl_core::instructions::CreateCollectionV1Builder;
    let kp = Keypair::new();
    let ix = CreateCollectionV1Builder::new()
        .collection(kp.pubkey())
        .payer(payer.pubkey())
        .update_authority(Some(payer.pubkey()))
        .name("Test Collection".to_string())
        .uri("https://example.com/col.json".to_string())
        .instruction();
    send_multi(svm, &[ix], payer, &[&kp]);
    kp.pubkey()
}

fn create_asset(svm: &mut LiteSVM, payer: &Keypair, owner: &Pubkey, collection: &Pubkey) -> Pubkey {
    use mpl_core::instructions::CreateV2Builder;
    let kp = Keypair::new();
    let ix = CreateV2Builder::new()
        .asset(kp.pubkey())
        .collection(Some(*collection))
        .payer(payer.pubkey())
        .owner(Some(*owner))
        .authority(Some(payer.pubkey()))
        .name("Test NFT".to_string())
        .uri("https://example.com/nft.json".to_string())
        .instruction();
    send_multi(svm, &[ix], payer, &[&kp]);
    kp.pubkey()
}

// ─── instruction builders ─────────────────────────────────────────────────────

fn ix_initialize(admin: &Pubkey, name: &str, fee: u16) -> Instruction {
    let (marketplace, _) = marketplace_pda(name);
    let (treasury, _) = treasury_pda(&marketplace);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &mp_ix::Initialize {
            name: name.to_string(),
            fee,
        }
        .data(),
        mp_accounts::Initialize {
            admin: *admin,
            marketplace,
            treasury,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_list(seller: &Pubkey, asset: &Pubkey, collection: &Pubkey, name: &str, price: u64) -> Instruction {
    let (marketplace, _) = marketplace_pda(name);
    let (listing, _) = listing_pda(&marketplace, asset);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &mp_ix::List {
            price,
            payment_mint: SYSTEM_ID,
        }
        .data(),
        mp_accounts::List {
            seller: *seller,
            asset: *asset,
            collection: *collection,
            marketplace,
            listing,
            mpl_core_program: MPL_CORE_ID,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_delist(seller: &Pubkey, asset: &Pubkey, collection: &Pubkey, name: &str) -> Instruction {
    let (marketplace, _) = marketplace_pda(name);
    let (listing, _) = listing_pda(&marketplace, asset);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &mp_ix::Delist {}.data(),
        mp_accounts::Delist {
            seller: *seller,
            asset: *asset,
            collection: *collection,
            marketplace,
            listing,
            mpl_core_program: MPL_CORE_ID,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_buy(buyer: &Pubkey, seller: &Pubkey, asset: &Pubkey, collection: &Pubkey, name: &str) -> Instruction {
    let (marketplace, _) = marketplace_pda(name);
    let (treasury, _) = treasury_pda(&marketplace);
    let (listing, _) = listing_pda(&marketplace, asset);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &mp_ix::Buy {}.data(),
        mp_accounts::Buy {
            buyer: *buyer,
            seller: *seller,
            asset: *asset,
            collection: *collection,
            marketplace,
            treasury,
            listing,
            mpl_core_program: MPL_CORE_ID,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_make_offer(buyer: &Pubkey, asset: &Pubkey, amount: u64) -> Instruction {
    let (offer, _) = offer_pda(asset, buyer);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &mp_ix::MakeOffer { amount }.data(),
        mp_accounts::MakeOffer {
            buyer: *buyer,
            asset: *asset,
            offer,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_accept_offer(
    seller: &Pubkey,
    buyer: &Pubkey,
    asset: &Pubkey,
    collection: &Pubkey,
    name: &str,
) -> Instruction {
    let (marketplace, _) = marketplace_pda(name);
    let (treasury, _) = treasury_pda(&marketplace);
    let (listing, _) = listing_pda(&marketplace, asset);
    let (offer, _) = offer_pda(asset, buyer);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &mp_ix::AcceptOffer {}.data(),
        mp_accounts::AcceptOffer {
            seller: *seller,
            buyer: *buyer,
            asset: *asset,
            collection: *collection,
            marketplace,
            treasury,
            listing,
            offer,
            mpl_core_program: MPL_CORE_ID,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

fn ix_cancel_offer(buyer: &Pubkey, asset: &Pubkey) -> Instruction {
    let (offer, _) = offer_pda(asset, buyer);
    Instruction::new_with_bytes(
        PROGRAM_ID,
        &mp_ix::CancelOffer {}.data(),
        mp_accounts::CancelOffer {
            buyer: *buyer,
            asset: *asset,
            offer,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    )
}

// ─── account readers ──────────────────────────────────────────────────────────

fn read_listing(svm: &LiteSVM, listing: &Pubkey) -> Listing {
    let data = svm.get_account(listing).expect("listing not found").data;
    Listing::try_deserialize(&mut &data[..]).unwrap()
}

fn read_offer(svm: &LiteSVM, offer: &Pubkey) -> Offer {
    let data = svm.get_account(offer).expect("offer not found").data;
    Offer::try_deserialize(&mut &data[..]).unwrap()
}

// ─── fixture ─────────────────────────────────────────────────────────────────

struct Fixture {
    svm: LiteSVM,
    admin: Keypair,
    seller: Keypair,
    buyer: Keypair,
    collection: Pubkey,
    asset: Pubkey,
    marketplace: Pubkey,
    name: String,
}

fn setup() -> Fixture {
    let mut svm = fresh_svm();
    let admin = Keypair::new();
    let seller = Keypair::new();
    let buyer = Keypair::new();
    svm.airdrop(&admin.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&seller.pubkey(), 10_000_000_000).unwrap();
    svm.airdrop(&buyer.pubkey(), 10_000_000_000).unwrap();

    let name = "TestMarket".to_string();
    send(&mut svm, ix_initialize(&admin.pubkey(), &name, 200), &admin); // 2% fee

    let collection = create_collection(&mut svm, &admin);
    let asset = create_asset(&mut svm, &admin, &seller.pubkey(), &collection);

    let (marketplace, _) = marketplace_pda(&name);
    Fixture { svm, admin, seller, buyer, collection, asset, marketplace, name }
}

// ─── tests ────────────────────────────────────────────────────────────────────

#[test]
fn test_initialize() {
    let f = setup();
    let acc = f.svm.get_account(&f.marketplace).expect("marketplace not found");
    assert!(acc.data.len() > 8);
}

#[test]
fn test_list_creates_listing() {
    let mut f = setup();
    let price = 1_000_000_000u64; // 1 SOL
    send(
        &mut f.svm,
        ix_list(&f.seller.pubkey(), &f.asset, &f.collection, &f.name, price),
        &f.seller,
    );

    let (listing_key, _) = listing_pda(&f.marketplace, &f.asset);
    let listing = read_listing(&f.svm, &listing_key);
    assert_eq!(listing.seller, f.seller.pubkey());
    assert_eq!(listing.price, price);
    assert_eq!(listing.payment_mint, SYSTEM_ID);
}

#[test]
fn test_delist_removes_listing() {
    let mut f = setup();
    send(
        &mut f.svm,
        ix_list(&f.seller.pubkey(), &f.asset, &f.collection, &f.name, 1_000_000_000),
        &f.seller,
    );

    let (listing_key, _) = listing_pda(&f.marketplace, &f.asset);
    assert!(f.svm.get_account(&listing_key).is_some());

    send(
        &mut f.svm,
        ix_delist(&f.seller.pubkey(), &f.asset, &f.collection, &f.name),
        &f.seller,
    );

    // Listing account should be closed (no longer exists or zero data).
    let acc = f.svm.get_account(&listing_key);
    assert!(acc.is_none() || acc.unwrap().data.is_empty());
}

#[test]
fn test_delist_wrong_seller_fails() {
    let mut f = setup();
    send(
        &mut f.svm,
        ix_list(&f.seller.pubkey(), &f.asset, &f.collection, &f.name, 1_000_000_000),
        &f.seller,
    );

    let other = Keypair::new();
    f.svm.airdrop(&other.pubkey(), 1_000_000_000).unwrap();
    let ix = ix_delist(&other.pubkey(), &f.asset, &f.collection, &f.name);
    assert!(send_fails(&mut f.svm, &[ix], &other, &[]));
}

#[test]
fn test_buy_transfers_sol_and_nft() {
    let mut f = setup();
    let price = 1_000_000_000u64; // 1 SOL
    send(
        &mut f.svm,
        ix_list(&f.seller.pubkey(), &f.asset, &f.collection, &f.name, price),
        &f.seller,
    );

    let seller_before = f.svm.get_account(&f.seller.pubkey()).unwrap().lamports;
    let (treasury, _) = treasury_pda(&f.marketplace);
    let treasury_before = f.svm.get_account(&treasury).map(|a| a.lamports).unwrap_or(0);

    send(
        &mut f.svm,
        ix_buy(
            &f.buyer.pubkey(),
            &f.seller.pubkey(),
            &f.asset,
            &f.collection,
            &f.name,
        ),
        &f.buyer,
    );

    let fee = price * 200 / 10_000; // 2%
    let seller_after = f.svm.get_account(&f.seller.pubkey()).unwrap().lamports;
    let treasury_after = f.svm.get_account(&treasury).map(|a| a.lamports).unwrap_or(0);

    // Seller receives listing rent refund + price minus fee; we just check the
    // treasury received the fee and seller received at least (price - fee).
    assert!(
        seller_after >= seller_before + (price - fee),
        "seller did not receive enough SOL"
    );
    assert_eq!(treasury_after - treasury_before, fee, "wrong treasury fee");

    // Listing should be closed.
    let (listing_key, _) = listing_pda(&f.marketplace, &f.asset);
    let acc = f.svm.get_account(&listing_key);
    assert!(acc.is_none() || acc.unwrap().data.is_empty());
}

#[test]
fn test_make_offer_escrows_sol() {
    let mut f = setup();
    let amount = 500_000_000u64; // 0.5 SOL

    send(
        &mut f.svm,
        ix_make_offer(&f.buyer.pubkey(), &f.asset, amount),
        &f.buyer,
    );

    let (offer_key, _) = offer_pda(&f.asset, &f.buyer.pubkey());
    let offer = read_offer(&f.svm, &offer_key);
    assert_eq!(offer.buyer, f.buyer.pubkey());
    assert_eq!(offer.amount, amount);

    // PDA holds at least `amount` lamports.
    let offer_acc = f.svm.get_account(&offer_key).unwrap();
    assert!(offer_acc.lamports >= amount);
}

#[test]
fn test_cancel_offer_refunds_buyer() {
    let mut f = setup();
    let amount = 500_000_000u64;

    send(
        &mut f.svm,
        ix_make_offer(&f.buyer.pubkey(), &f.asset, amount),
        &f.buyer,
    );

    let buyer_before = f.svm.get_account(&f.buyer.pubkey()).unwrap().lamports;

    send(
        &mut f.svm,
        ix_cancel_offer(&f.buyer.pubkey(), &f.asset),
        &f.buyer,
    );

    let buyer_after = f.svm.get_account(&f.buyer.pubkey()).unwrap().lamports;
    assert!(buyer_after > buyer_before, "buyer should get SOL back");

    let (offer_key, _) = offer_pda(&f.asset, &f.buyer.pubkey());
    let acc = f.svm.get_account(&offer_key);
    assert!(acc.is_none() || acc.unwrap().data.is_empty(), "offer should be closed");
}

#[test]
fn test_accept_offer_pays_seller_and_transfers_nft() {
    let mut f = setup();
    let price = 2_000_000_000u64; // list at 2 SOL
    let offer_amount = 1_500_000_000u64; // buyer offers 1.5 SOL

    // Seller lists.
    send(
        &mut f.svm,
        ix_list(&f.seller.pubkey(), &f.asset, &f.collection, &f.name, price),
        &f.seller,
    );

    // Buyer makes an offer below listed price.
    send(
        &mut f.svm,
        ix_make_offer(&f.buyer.pubkey(), &f.asset, offer_amount),
        &f.buyer,
    );

    let seller_before = f.svm.get_account(&f.seller.pubkey()).unwrap().lamports;
    let (treasury, _) = treasury_pda(&f.marketplace);
    let treasury_before = f.svm.get_account(&treasury).map(|a| a.lamports).unwrap_or(0);

    // Seller accepts the offer; both seller and buyer must sign.
    let ix = ix_accept_offer(
        &f.seller.pubkey(),
        &f.buyer.pubkey(),
        &f.asset,
        &f.collection,
        &f.name,
    );
    send_multi(&mut f.svm, &[ix], &f.seller, &[&f.buyer]);

    let fee = offer_amount * 200 / 10_000;
    let seller_after = f.svm.get_account(&f.seller.pubkey()).unwrap().lamports;
    let treasury_after = f.svm.get_account(&treasury).map(|a| a.lamports).unwrap_or(0);

    assert!(
        seller_after >= seller_before + (offer_amount - fee),
        "seller did not receive enough SOL from offer"
    );
    assert_eq!(treasury_after - treasury_before, fee, "wrong treasury fee");

    // Both listing and offer should be closed.
    let (listing_key, _) = listing_pda(&f.marketplace, &f.asset);
    let (offer_key, _) = offer_pda(&f.asset, &f.buyer.pubkey());
    let listing_acc = f.svm.get_account(&listing_key);
    let offer_acc = f.svm.get_account(&offer_key);
    assert!(listing_acc.is_none() || listing_acc.unwrap().data.is_empty());
    assert!(offer_acc.is_none() || offer_acc.unwrap().data.is_empty());
}

#[test]
fn test_cannot_list_twice() {
    let mut f = setup();
    send(
        &mut f.svm,
        ix_list(&f.seller.pubkey(), &f.asset, &f.collection, &f.name, 1_000_000_000),
        &f.seller,
    );
    // Trying to list the same asset again should fail (Listing PDA already init'd).
    let ix = ix_list(&f.seller.pubkey(), &f.asset, &f.collection, &f.name, 2_000_000_000);
    assert!(send_fails(&mut f.svm, &[ix], &f.seller, &[]));
}

#[test]
fn test_cancel_offer_wrong_buyer_fails() {
    let mut f = setup();
    let amount = 500_000_000u64;

    send(
        &mut f.svm,
        ix_make_offer(&f.buyer.pubkey(), &f.asset, amount),
        &f.buyer,
    );

    // A different keypair tries to cancel — should fail.
    let other = Keypair::new();
    f.svm.airdrop(&other.pubkey(), 1_000_000_000).unwrap();

    // We need to build an ix that references the correct offer PDA but signs
    // with a different key — the constraint `offer.buyer == buyer` will reject it.
    let (offer_key, _) = offer_pda(&f.asset, &f.buyer.pubkey());
    let ix = Instruction::new_with_bytes(
        PROGRAM_ID,
        &mp_ix::CancelOffer {}.data(),
        mp_accounts::CancelOffer {
            buyer: other.pubkey(), // wrong signer
            asset: f.asset,
            offer: offer_key,
            system_program: SYSTEM_ID,
        }
        .to_account_metas(None),
    );
    assert!(send_fails(&mut f.svm, &[ix], &other, &[]));
}
