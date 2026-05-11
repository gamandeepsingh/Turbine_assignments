import * as fs from "fs";
import * as path from "path";
import { execSync } from "child_process";
import { Keypair, Connection, LAMPORTS_PER_SOL } from "@solana/web3.js";

const WALLET_PATH = path.resolve(__dirname, "../../wallet.json");

export function loadOrCreateKeypair(): Keypair {
  if (fs.existsSync(WALLET_PATH)) {
    const raw = JSON.parse(fs.readFileSync(WALLET_PATH, "utf-8"));
    return Keypair.fromSecretKey(Uint8Array.from(raw));
  }
  const keypair = Keypair.generate();
  fs.writeFileSync(WALLET_PATH, JSON.stringify(Array.from(keypair.secretKey)));
  console.log("Generated new wallet, saved to wallet.json");
  return keypair;
}

export async function ensureFunded(
  connection: Connection,
  keypair: Keypair,
  minSol = 1
): Promise<void> {
  const balance = await connection.getBalance(keypair.publicKey);
  if (balance >= minSol * LAMPORTS_PER_SOL) {
    console.log(`Balance: ${balance / LAMPORTS_PER_SOL} SOL (sufficient)`);
    return;
  }

  console.log(`Balance: ${balance / LAMPORTS_PER_SOL} SOL — requesting airdrop via CLI...`);
  try {
    execSync(
      `solana airdrop 2 ${keypair.publicKey.toBase58()} --url devnet`,
      { stdio: "inherit" }
    );
    const newBalance = await connection.getBalance(keypair.publicKey);
    console.log(`New balance: ${newBalance / LAMPORTS_PER_SOL} SOL`);
  } catch {
    console.error(
      "\nAirdrop failed. Fund manually:\n" +
        `  https://faucet.solana.com  (paste address: ${keypair.publicKey.toBase58()})\n` +
        `  or: solana airdrop 2 ${keypair.publicKey.toBase58()} --url devnet\n` +
        "Then re-run this script."
    );
    process.exit(1);
  }
}
