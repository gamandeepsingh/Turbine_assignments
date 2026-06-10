# Assignment 04 — NFT Marketplace

An Anchor 1.0 program that lets users list, delist, and purchase MPL Core NFTs. Supports SOL payments, SPL token payments (e.g. USDC), and a make/accept/cancel offer flow.

## Instructions

| Instruction | Description |
|---|---|
| `initialize` | Deploy the marketplace config and SOL treasury PDA |
| `list` | Freeze and delegate an MPL Core asset for sale; create a Listing |
| `delist` | Unfreeze and undelegate the asset; close the Listing |
| `buy` | Pay the listed SOL price; marketplace transfers NFT to buyer |
| `buy_with_token` | Pay with an SPL token (e.g. USDC); fee split via `token_interface::transfer_checked` |
| `make_offer` | Escrow SOL into an Offer PDA; seller does not have to accept |
| `accept_offer` | Seller accepts an open offer; NFT transfers to buyer at offer price |
| `cancel_offer` | Buyer cancels their offer; escrowed SOL is refunded |

## Challenges

### Challenge 1 — Delist

`delist` lets the seller reclaim their NFT at any time before a sale. It:
1. Unfreezes the asset (marketplace PDA signs as FreezeDelegate authority).
2. Removes the `FreezeDelegate` plugin (seller/owner signs).
3. Removes the `TransferDelegate` plugin (seller/owner signs).
4. Closes the `Listing` account, returning rent to the seller.

### Challenge 2 — SPL Token Payments (`buy_with_token`)

The `Listing` account carries a `payment_mint: Pubkey` field:
- `payment_mint == system_program::ID` → SOL listing (use `buy`)
- Anything else → token listing (use `buy_with_token`)

`buy_with_token` uses `token_interface::transfer_checked` for Token-2022 compatibility. The fee goes to a treasury ATA (`[b"treasury", marketplace]` as the ATA owner) and the remainder goes to the seller's ATA. Both ATAs are created `init_if_needed`.

### Challenge 3 — Make / Accept / Cancel Offer

An alternative purchase path where the buyer names their own price:

- **`make_offer(asset, amount)`** — creates an `Offer` PDA (`[b"offer", asset, buyer]`) and escrows `amount` lamports into it via `system_program::transfer`.
- **`accept_offer`** — the seller (and buyer, for plugin removal) sign. The marketplace PDA unfreezes and `TransferV1` moves the NFT to the buyer. SOL drains from the Offer PDA: fee → treasury, remainder → seller. Both `Listing` and `Offer` accounts are closed.
- **`cancel_offer`** — the buyer signs; the `Offer` account is closed with `close = buyer`, returning all escrowed SOL plus rent.

## Architecture

```
Marketplace  (PDA: ["marketplace", name])
  ├── admin         : Pubkey
  ├── fee           : u16     — basis points (e.g. 200 = 2%)
  ├── bump          : u8
  └── treasury_bump : u8

Treasury     (PDA: ["treasury", marketplace])  — SystemAccount (SOL fees)
             (ATA of treasury PDA)             — token fees for buy_with_token

Listing      (PDA: ["listing", marketplace, asset])
  ├── seller       : Pubkey
  ├── asset        : Pubkey
  ├── price        : u64    — lamports or token base units
  └── payment_mint : Pubkey — system_program::ID for SOL; mint address otherwise

Offer        (PDA: ["offer", asset, buyer])
  ├── buyer  : Pubkey
  ├── asset  : Pubkey
  └── amount : u64    — escrowed SOL in lamports
```

### MPL Core plugin lifecycle

```
list:        AddPlugin FreezeDelegate (frozen=true,  authority=marketplace PDA)
             AddPlugin TransferDelegate             (authority=marketplace PDA)

buy / accept_offer:
             UpdatePlugin FreezeDelegate (frozen=false) — marketplace signs
             TransferV1                               — marketplace signs (TransferDelegate)
             RemovePlugin FreezeDelegate              — new owner (buyer) signs
             RemovePlugin TransferDelegate            — new owner (buyer) signs

delist:
             UpdatePlugin FreezeDelegate (frozen=false) — marketplace signs
             RemovePlugin FreezeDelegate                — seller (owner) signs
             RemovePlugin TransferDelegate              — seller (owner) signs
```

## Running Tests

### Prerequisites

- Rust toolchain `1.89.0` (managed by `rust-toolchain.toml`)
- Solana CLI ≥ 2.0 with `cargo-build-sbf`

### 1. Build the program

```bash
cd Assignment_04/nft_marketplace
cargo build-sbf
```

### 2. Obtain the MPL Core binary (first run only)

The tests automatically download the binary from Solana mainnet on the first run. To pre-fetch it manually:

```bash
solana program dump --url mainnet-beta \
  CoREENxT6tW1HoK8ypY1SxRMZTcVPm7R94rH4PZNhX7d \
  programs/nft_marketplace/tests/programs/mpl_core.so
```

Or set `MPL_CORE_SO=/path/to/mpl_core.so` before running tests.

### 3. Run all tests

```bash
cargo test
```

Expected output: **10 passed; 0 failed**

## Test Coverage

| Test | What it verifies |
|---|---|
| `test_initialize` | Marketplace PDA is created on-chain |
| `test_list_creates_listing` | Listing PDA holds correct seller, price, and payment mint |
| `test_delist_removes_listing` | Listing is closed and NFT is undelegate after delist |
| `test_delist_wrong_seller_fails` | Non-seller cannot delist |
| `test_buy_transfers_sol_and_nft` | Buyer pays price; fee reaches treasury; listing is closed |
| `test_make_offer_escrows_sol` | Offer PDA holds `amount` lamports |
| `test_cancel_offer_refunds_buyer` | Buyer gets SOL back and Offer is closed |
| `test_accept_offer_pays_seller_and_transfers_nft` | Seller receives offer price minus fee; NFT transfers; both Listing and Offer are closed |
| `test_cannot_list_twice` | Listing the same asset twice fails |
| `test_cancel_offer_wrong_buyer_fails` | Wrong signer cannot cancel another buyer's offer |

## Dependencies

| Crate | Version | Purpose |
|---|---|---|
| `anchor-lang` | 1.0.2 | Anchor framework (Solana 2.x, `init-if-needed` feature) |
| `anchor-spl` | 1.0.2 | SPL Token + Associated Token CPI helpers |
| `mpl-core` | 0.12 | MPL Core NFT standard (FreezeDelegate, TransferDelegate plugins) |
| `litesvm` | 0.12 | Fast SBF test runtime |
