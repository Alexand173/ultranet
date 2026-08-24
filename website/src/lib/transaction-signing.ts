import { sha3_256 } from "@noble/hashes/sha3.js";
import type {
  SignTransactionParams,
  SignedTransaction,
  UltraWalletProvider,
} from "@/lib/ultra-wallet";
import {
  buildTransactionRequest,
  isSignedTransaction,
  ULTRA_WALLET_SIGN_TRANSACTION,
  ULTRA_WALLET_TRANSACTION_VERSION,
} from "@/lib/ultra-wallet";
import { bytesToHex, type LocalWalletKeyMaterial } from "@/lib/wallet-crypto";
import { signDilithium5 } from "@/lib/dilithium-wasm";

const textEncoder = new TextEncoder();
const UINT64_BYTES = 8;

function appendUint64LE(target: number[], value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error("Transaction integer fields must be non-negative safe integers.");
  }
  let remaining = BigInt(value);
  for (let index = 0; index < UINT64_BYTES; index++) {
    target.push(Number(remaining & 0xffn));
    remaining >>= 8n;
  }
}

function appendBytes(target: number[], value: Uint8Array): void {
  for (const byte of value) target.push(byte);
}

export function buildLegacyTransactionDigest(params: {
  sender: string;
  recipient: string;
  amount: number;
  fee: number;
  timestamp: number;
  nullifier: Uint8Array;
  nonce: number;
  gasLimit: number;
  gasPrice: number;
}): Uint8Array {
  if (params.nullifier.length !== 32) throw new Error("A transaction nullifier must contain 32 bytes.");
  const preimage: number[] = [];
  appendBytes(preimage, textEncoder.encode(params.sender));
  appendBytes(preimage, textEncoder.encode(params.recipient));
  appendUint64LE(preimage, params.amount);
  appendUint64LE(preimage, params.fee);
  appendUint64LE(preimage, params.timestamp);
  appendBytes(preimage, params.nullifier);
  appendUint64LE(preimage, params.nonce);
  appendUint64LE(preimage, params.gasLimit);
  appendUint64LE(preimage, params.gasPrice);
  return sha3_256(new Uint8Array(preimage));
}

export async function signTransactionLocally(
  material: LocalWalletKeyMaterial,
  params: Omit<SignTransactionParams, "chainId" | "version">,
): Promise<SignedTransaction> {
  const publicKey = new Uint8Array(material.publicKey);
  const secretKey = new Uint8Array(material.secretKey);
  const nullifier = new Uint8Array(params.nullifier);
  const digest = buildLegacyTransactionDigest({
    sender: material.address,
    recipient: params.recipient,
    amount: params.amount,
    fee: params.fee,
    timestamp: params.timestamp,
    nullifier,
    nonce: params.nonce,
    gasLimit: params.gasLimit,
    gasPrice: params.gasPrice,
  });

  try {
    const signature = await signDilithium5(digest, secretKey);
    return buildTransactionRequest({
      sender: material.address,
      sender_public_key: Array.from(publicKey),
      recipient: params.recipient,
      amount: params.amount,
      fee: params.fee,
      nonce: params.nonce,
      timestamp: params.timestamp,
      nullifier: Array.from(nullifier),
      gas_limit: params.gasLimit,
      gas_price: params.gasPrice,
      signature: Array.from(signature),
      chain_id: 0,
      version: ULTRA_WALLET_TRANSACTION_VERSION,
    });
  } finally {
    digest.fill(0);
    nullifier.fill(0);
    secretKey.fill(0);
  }
}

export function transactionDigestHex(params: Parameters<typeof buildLegacyTransactionDigest>[0]): string {
  return bytesToHex(buildLegacyTransactionDigest(params));
}

export function createLocalTransactionProvider(material: LocalWalletKeyMaterial): UltraWalletProvider {
  const activeMaterial: LocalWalletKeyMaterial | null = material;
  return {
    async request(request) {
      if (request.method !== ULTRA_WALLET_SIGN_TRANSACTION) {
        throw new Error("This local wallet only signs standard UltraNet transfers.");
      }
      if (!activeMaterial) throw new Error("The local wallet is locked.");
      const signed = await signTransactionLocally(activeMaterial, request.params);
      if (!isSignedTransaction(signed)) throw new Error("The local wallet returned an invalid transaction.");
      return signed;
    },
  };
}
