import { Connection, clusterApiUrl } from "@solana/web3.js";
import {
  createMint,
  getOrCreateAssociatedTokenAccount,
  mintTo,
  getAccount,
  getMint,
} from "@solana/spl-token";
import { loadOrCreateKeypair, ensureFunded } from "./utils/wallet";

const DECIMALS = 9;
const MINT_AMOUNT = 1_000 * 10 ** DECIMALS; // 1,000 tokens

async function main() {
  const connection = new Connection(clusterApiUrl("devnet"), "confirmed");
  const payer = loadOrCreateKeypair();

  console.log("Payer:", payer.publicKey.toBase58());
  await ensureFunded(connection, payer, 1);

  // --- Create mint ---
  console.log("\nCreating SPL token mint...");
  const mint = await createMint(
    connection,
    payer,           // fee payer
    payer.publicKey, // mint authority
    payer.publicKey, // freeze authority
    DECIMALS
  );
  console.log("Mint:", mint.toBase58());

  // --- Create associated token account ---
  console.log("\nCreating associated token account...");
  const tokenAccount = await getOrCreateAssociatedTokenAccount(
    connection,
    payer,
    mint,
    payer.publicKey
  );
  console.log("ATA:", tokenAccount.address.toBase58());

  // --- Mint tokens ---
  console.log("\nMinting 1,000 tokens...");
  const mintTxSig = await mintTo(
    connection,
    payer,
    mint,
    tokenAccount.address,
    payer.publicKey, // mint authority
    MINT_AMOUNT
  );
  console.log("Mint tx:", mintTxSig);

  // --- Verify ---
  const [account, mintInfo] = await Promise.all([
    getAccount(connection, tokenAccount.address),
    getMint(connection, mint),
  ]);

  console.log("\n========== SPL Token ==========");
  console.log("Mint address :", mint.toBase58());
  console.log("Decimals     :", mintInfo.decimals);
  console.log("Supply       :", Number(mintInfo.supply) / 10 ** DECIMALS, "tokens");
  console.log("ATA balance  :", Number(account.amount) / 10 ** DECIMALS, "tokens");
  console.log(
    "Explorer     :",
    `https://explorer.solana.com/address/${mint.toBase58()}?cluster=devnet`
  );
}

main().catch(console.error);
