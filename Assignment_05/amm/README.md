# Assignment 05 — AMM with Instruction Introspection

**Challenge 2**: An AMM that uses instruction introspection to verify a token burn before paying out liquidity. The `burn_lp` and `withdraw` instructions are **separate**, but `withdraw` refuses to execute unless `burn_lp` immediately precedes it in the same transaction.

---

## Architecture

### State

```
Pool PDA  [b"pool", mint_a, mint_b]
  token_a_mint: Pubkey
  token_b_mint: Pubkey
  lp_mint:      Pubkey
  lp_supply:    u64
  bump/lp_mint_bump/vault_a_bump/vault_b_bump: u8
```

### PDAs

| Account | Seeds |
|---------|-------|
| Pool    | `[b"pool", mint_a, mint_b]` |
| Vault A | `[b"vault_a", pool]` |
| Vault B | `[b"vault_b", pool]` |
| LP Mint | `[b"lp_mint", pool]` |
| LP Account | `[b"lp_account", lp_mint, user]` |

Vault and LP accounts are PDA-owned token accounts — no ATA program needed, which makes testing with LiteSVM straightforward.

---

## Instructions

### `initialize(initial_a, initial_b)`

Creates the pool, both vaults, the LP mint, and the initial depositor's LP account in one transaction. Seeds the vaults and mints `√initial_a × √initial_b` LP tokens to the depositor.

### `deposit(amount_a, amount_b, min_lp)`

Proportional deposit: mints `min(Δa/a × LP, Δb/b × LP)` new LP tokens. If `lp_supply == 0` (pool initialized with no initial liquidity) uses the geometric mean formula. Creates the depositor's LP account on first use (`init_if_needed`).

### `swap(amount_in, min_out, a_to_b)`

Constant-product swap with **0.3% fee**:

```
amount_out = (amount_in × 997 × out_reserve) / (in_reserve × 1000 + amount_in × 997)
```

### `burn_lp(lp_amount)`

Burns `lp_amount` LP tokens from the caller's PDA-based LP account. Does **not** update `pool.lp_supply` — `withdraw` owns that update. This instruction must appear immediately before `withdraw` in the same transaction.

### `withdraw(min_a, min_b)` ← instruction introspection

Pays out proportional token A and B. Before doing anything, it:

1. Reads `sysvar::instructions` to find the current instruction index.
2. Looks back one slot and loads that instruction.
3. Checks `program_id == this_program`.
4. Checks the 8-byte discriminator matches `BurnLp::DISCRIMINATOR`.
5. Extracts `lp_amount` from bytes 8–16 of the instruction data.
6. Verifies `accounts[0]` (the signer of burn_lp) matches the current caller.
7. Verifies `accounts[2]` (the LP mint in burn_lp) matches this pool's LP mint.

Only after all checks pass are tokens transferred out of the vaults. This is the core of Challenge 2: two separate instructions that must co-exist in the same transaction, with the second enforcing the first happened.

---

## Key Design Decisions

### Why separate `burn_lp` and `withdraw`?

Instruction introspection is the point of the exercise. Burning LP and receiving tokens are logically atomic but implemented as distinct Anchor entry points. `withdraw` cannot be called standalone — it will always error unless `burn_lp` preceded it in the same atomic transaction.

### Why PDA-based LP accounts instead of ATAs?

LiteSVM does not include the ATA program by default. Using seeds `[b"lp_account", lp_mint, user]` for LP token accounts means the SPL Token program (which IS loaded by LiteSVM) handles all token operations, with no dependency on the ATA program.

### Why box all `Account<T>` in `#[derive(Accounts)]`?

Solana BPF stack frames are capped at 4096 bytes. `Account<TokenAccount>` stores 165 bytes of deserialized data on the stack; with 5–7 such accounts per instruction, the `try_accounts` frame exceeded the limit. `Box<Account<T>>` moves the data to the heap (8-byte pointer on stack), keeping each frame well under the limit.

### u128 arithmetic

Deposit/swap/withdraw use `u128` intermediates for overflow-safe multiplication (e.g., `amount × lp_supply / reserve`). The `integer_sqrt` function uses **u64 only** to avoid calling `__udivti3` (compiler-rt's 128-bit division helper), which would push the call depth past the SBF limit.

---

## Build & Test

```bash
# Build the SBF binary
cargo build-sbf

# Run all integration tests (LiteSVM, no validator needed)
cargo test
```

Expected output:
```
running 9 tests
test test_initialize_creates_pool_and_mints_lp ... ok
test test_deposit_mints_proportional_lp        ... ok
test test_deposit_slippage_fails               ... ok
test test_swap_a_to_b_constant_product         ... ok
test test_swap_b_to_a_constant_product         ... ok
test test_swap_slippage_exceeded_fails         ... ok
test test_burn_and_withdraw_in_same_tx         ... ok
test test_withdraw_without_burn_fails          ... ok
test test_withdraw_wrong_user_burn_fails       ... ok

test result: ok. 9 passed; 0 failed
```

---

## Test Coverage

| Test | What it verifies |
|------|-----------------|
| `test_initialize_creates_pool_and_mints_lp` | Pool created, vaults funded, LP supply = √a × √b |
| `test_deposit_mints_proportional_lp` | Proportional LP minting; user balances decrease |
| `test_deposit_slippage_fails` | `min_lp` guard rejects when slippage too high |
| `test_swap_a_to_b_constant_product` | Correct output amount and vault balance change |
| `test_swap_b_to_a_constant_product` | Reverse direction swap |
| `test_swap_slippage_exceeded_fails` | `min_out` guard rejects |
| `test_burn_and_withdraw_in_same_tx` | Atomic `[burn_lp, withdraw]` tx pays out correct amounts |
| `test_withdraw_without_burn_fails` | Standalone `withdraw` errors with `MissingBurnInstruction` |
| `test_withdraw_wrong_user_burn_fails` | `withdraw` errors when `burn_lp` was for a different user |
