import { sha3_256 } from "@noble/hashes/sha3.js";
import { fetchAccount } from "@/lib/api";
import { signDilithium5 } from "@/lib/dilithium-wasm";
import {
  hexToBytes,
  type LocalWalletKeyMaterial,
} from "@/lib/wallet-crypto";
import {
  ULTRA_WALLET_SIGN_VALIDATOR_PROPOSAL,
  ULTRA_WALLET_SIGNING_ENVELOPE_VERSION,
  type UltraWalletProvider,
} from "@/lib/ultra-wallet";

const PUBLIC_KEY_HEX_LENGTH = 2_592 * 2;
const PROPOSAL_RECIPIENT = "0x0";
const PROPOSAL_AMOUNT = 0;
const PROPOSAL_FEE = 0;
const PROPOSAL_GAS_LIMIT = 1_000_000;
const PROPOSAL_GAS_PRICE = 1;
const PROPOSAL_CHAIN_ID = 0;
const PROPOSAL_DOMAIN = "UltraNet/transaction-signing-envelope/v2";
const textEncoder = new TextEncoder();

function appendBytes(target: number[], bytes: Uint8Array): void {
  for (const byte of bytes) target.push(byte);
}

function appendUint32LE(target: number[], value: number): void {
  if (!Number.isSafeInteger(value) || value < 0 || value > 0xffffffff) {
    throw new Error("Proposal integer fields must be valid unsigned 32-bit integers.");
  }
  target.push(value & 0xff, (value >>> 8) & 0xff, (value >>> 16) & 0xff, (value >>> 24) & 0xff);
}

function appendUint64LE(target: number[], value: number): void {
  if (!Number.isSafeInteger(value) || value < 0) {
    throw new Error("Proposal integer fields must be non-negative safe integers.");
  }
  let remaining = BigInt(value);
  for (let index = 0; index < 8; index += 1) {
    target.push(Number(remaining & 0xffn));
    remaining >>= 8n;
  }
}

function appendString(target: number[], value: string): void {
  appendBytes(target, textEncoder.encode(value));
}

export function validateProposalPublicKey(value: string): string | null {
  const normalized = value.trim().replace(/^0x/i, "");
  if (normalized.length !== PUBLIC_KEY_HEX_LENGTH) {
    return "The public key must be exactly 5,184 hexadecimal characters.";
  }
  if (!/^[0-9a-f]+$/i.test(normalized)) {
    return "The public key may contain only hexadecimal characters.";
  }
  return null;
}

export function normalizeProposalPublicKey(value: string): string {
  const error = validateProposalPublicKey(value);
  if (error) throw new Error(error);
  return value.trim().replace(/^0x/i, "").toLowerCase();
}

export function buildValidatorProposalDigest(input: {
  sender: string;
  proposalPublicKey: Uint8Array;
  metadata: string;
  nonce: number;
  timestamp: number;
  nullifier: Uint8Array;
}): Uint8Array {
  if (input.nullifier.length !== 32) throw new Error("A proposal nullifier must contain 32 bytes.");
  if (input.proposalPublicKey.length !== 2_592) throw new Error("A proposal public key must contain 2,592 bytes.");

  const preimage: number[] = [];
  appendString(preimage, input.sender);
  appendString(preimage, PROPOSAL_RECIPIENT);
  appendUint64LE(preimage, PROPOSAL_AMOUNT);
  appendUint64LE(preimage, PROPOSAL_FEE);
  appendUint64LE(preimage, input.timestamp);
  appendBytes(preimage, input.nullifier);
  appendUint64LE(preimage, input.nonce);
  appendUint64LE(preimage, PROPOSAL_GAS_LIMIT);
  appendUint64LE(preimage, PROPOSAL_GAS_PRICE);
  appendString(preimage, PROPOSAL_DOMAIN);
  appendUint32LE(preimage, ULTRA_WALLET_SIGNING_ENVELOPE_VERSION);
  appendUint32LE(preimage, PROPOSAL_CHAIN_ID);
  appendUint64LE(preimage, input.proposalPublicKey.length);
  appendBytes(preimage, input.proposalPublicKey);
  const metadataBytes = textEncoder.encode(input.metadata);
  appendUint64LE(preimage, metadataBytes.length);
  appendBytes(preimage, metadataBytes);
  metadataBytes.fill(0);

  const digest = sha3_256(new Uint8Array(preimage));
  preimage.fill(0);
  return digest;
}

export function createLocalValidatorProvider(
  getMaterial: () => LocalWalletKeyMaterial | null,
): UltraWalletProvider {
  return {
    async request(request) {
      if (request.method !== ULTRA_WALLET_SIGN_VALIDATOR_PROPOSAL) {
        throw new Error("This local wallet only signs validator proposals through the proposal form.");
      }

      const material = getMaterial();
      if (!material) throw new Error("The local wallet is locked. Unlock it before signing.");
      const metadata = request.params.metadata.trim();
      if (!metadata) throw new Error("A node alias is required before signing.");

      const proposalPublicKeyHex = normalizeProposalPublicKey(request.params.proposalPublicKey);
      const proposalPublicKey = hexToBytes(proposalPublicKeyHex);
      const account = await fetchAccount(material.address);
      const nullifier = crypto.getRandomValues(new Uint8Array(32));
      const timestamp = Math.floor(Date.now() / 1000);
      const digest = buildValidatorProposalDigest({
        sender: material.address,
        proposalPublicKey,
        metadata,
        nonce: account.nonce,
        timestamp,
        nullifier,
      });
      const secretKey = new Uint8Array(material.secretKey);
      let signature: Uint8Array | null = null;

      try {
        signature = await signDilithium5(digest, secretKey);
        return {
          sender: material.address,
          sender_public_key: Array.from(material.publicKey),
          proposal_public_key: Array.from(proposalPublicKey),
          nonce: account.nonce,
          timestamp,
          nullifier: Array.from(nullifier),
          signature: Array.from(signature),
          version: ULTRA_WALLET_SIGNING_ENVELOPE_VERSION,
        };
      } finally {
        digest.fill(0);
        nullifier.fill(0);
        proposalPublicKey.fill(0);
        secretKey.fill(0);
        signature?.fill(0);
      }
    },
  };
}