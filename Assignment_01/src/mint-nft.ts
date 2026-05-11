import { Connection, clusterApiUrl } from "@solana/web3.js";
import { createUmi } from "@metaplex-foundation/umi-bundle-defaults";
import {
  generateSigner,
  keypairIdentity,
} from "@metaplex-foundation/umi";
import { base58 } from "@metaplex-foundation/umi/serializers";
import { create, mplCore } from "@metaplex-foundation/mpl-core";
import { loadOrCreateKeypair, ensureFunded } from "./utils/wallet";

// In production: upload JSON metadata to Arweave/IPFS first, use the resulting URI here.
const NFT_URI = "https://arweave.net/metadata-placeholder-devnet-test";

async function main() {
  // Fund check via web3.js connection (same wallet as SPL script)
  const connection = new Connection(clusterApiUrl("devnet"), "confirmed");
  const web3Keypair = loadOrCreateKeypair();

  console.log("Signer:", web3Keypair.publicKey.toBase58());
  await ensureFunded(connection, web3Keypair, 1);

  // --- Wire up Umi with the same keypair ---
  const umi = createUmi("https://api.devnet.solana.com");
  umi.use(mplCore());

  // Convert web3.js Keypair → Umi keypair
  const umiKeypair = umi.eddsa.createKeypairFromSecretKey(web3Keypair.secretKey);
  umi.use(keypairIdentity(umiKeypair));

  // --- Mint MPL Core NFT with Attributes plugin ---
  const asset = generateSigner(umi);
  console.log("\nAsset address:", asset.publicKey);
  console.log("Creating MPL Core NFT with Attributes plugin...");

  const { signature } = await create(umi, {
    asset,
    name: "Turbine NFT #01",
    uri: NFT_URI,
    // Plugin: Attributes stores arbitrary on-chain key/value traits
    plugins: [
      {
        type: "Attributes",
        attributeList: [
          { key: "background", value: "ocean blue" },
          { key: "rarity",     value: "legendary"  },
          { key: "power",      value: "9000"        },
          { key: "assignment", value: "01"           },
        ],
      },
    ],
  }).sendAndConfirm(umi);

  const txSignature = base58.deserialize(signature)[0];

  console.log("\n========== MPL Core NFT ==========");
  console.log("Asset address :", asset.publicKey);
  console.log("Plugin used   : Attributes (on-chain key/value traits)");
  console.log("Tx signature  :", txSignature);
  console.log(
    "Asset Explorer:",
    `https://explorer.solana.com/address/${asset.publicKey}?cluster=devnet`
  );
  console.log(
    "Tx Explorer   :",
    `https://explorer.solana.com/tx/${txSignature}?cluster=devnet`
  );
}

main().catch(console.error);
