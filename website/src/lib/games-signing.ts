import { sha3_256 } from "@noble/hashes/sha3.js";
import { signDilithium5 } from "@/lib/dilithium-wasm";
import { bytesToHex, type LocalWalletKeyMaterial } from "@/lib/wallet-crypto";
import {
  ULTRA_WALLET_SIGN_GAME_ADMIN,
  ULTRA_WALLET_SIGN_GAMES_CHALLENGE,
  type SignedGamesDigest,
  type UltraWalletProvider,
} from "@/lib/ultra-wallet";

/**
 * Signatures for the Games service, mirroring `challenge_digest` in `src/games/auth.rs` and `admin_digest` in
 * `src/games/catalog.rs`. The same vectors are pinned in the Rust tests and in the Android tests, so the three
 * implementations cannot drift apart without a test failing.
 *
 * Both digests have their own domain strings: a signature made here can never be mistaken for a transaction.
 */

const CHALLENGE_DOMAIN = "UltraNet/wallet-challenge/v1";
const ADMIN_DOMAIN = "UltraNet/game-admin/v1";
const textEncoder = new TextEncoder();
const ADDRESS_PATTERN = /^[0-9a-f]{64}$/;
const ID_PATTERN = /^[0-9a-f]{64}$/;
const NONCE_PATTERN = /^[0-9a-f]{16,64}$/;

export const GAMES_PURPOSE_PLAY = "play" as const;
export const GAME_ACTION = {
  register: "register",
  fund: "pool-fund",
  withdraw: "pool-withdraw",
  policyPropose: "policy-propose",
} as const;

function hexToBytes(value: string): Uint8Array {
  const out = new Uint8Array(value.length / 2);
  for (let index = 0; index < out.length; index += 1) out[index] = parseInt(value.slice(index * 2, index * 2 + 2), 16);
  return out;
}

function u32le(value: number): Uint8Array {
  if (!Number.isInteger(value) || value < 0 || value > 0xffffffff) throw new Error("A 32-bit field is out of range.");
  const out = new Uint8Array(4);
  new DataView(out.buffer).setUint32(0, value, true);
  return out;
}

function u64le(value: number): Uint8Array {
  if (!Number.isSafeInteger(value) || value < 0) throw new Error("A 64-bit field is out of range.");
  const out = new Uint8Array(8);
  new DataView(out.buffer).setBigUint64(0, BigInt(value), true);
  return out;
}

function lengthPrefixed(text: string): Uint8Array {
  const bytes = textEncoder.encode(text);
  const out = new Uint8Array(4 + bytes.length);
  out.set(u32le(bytes.length), 0);
  out.set(bytes, 4);
  return out;
}

function concat(parts: Uint8Array[]): Uint8Array {
  const out = new Uint8Array(parts.reduce((total, part) => total + part.length, 0));
  let offset = 0;
  for (const part of parts) {
    out.set(part, offset);
    offset += part.length;
  }
  return out;
}

export function buildChallengeDigest(input: {
  purpose: string;
  address: string;
  challengeId: string;
  issuedAt: number;
  expiresAt: number;
}): Uint8Array {
  if (input.purpose !== GAMES_PURPOSE_PLAY) throw new Error("This wallet signs the Games sign-in for the purpose \"play\" only.");
  if (!ADDRESS_PATTERN.test(input.address)) throw new Error("The address must be 64 lowercase hexadecimal characters.");
  if (!ID_PATTERN.test(input.challengeId)) throw new Error("The challenge id must be 64 lowercase hexadecimal characters.");
  if (!(input.issuedAt >= 0 && input.issuedAt <= input.expiresAt)) throw new Error("The challenge times are not valid.");
  return sha3_256(
    concat([
      textEncoder.encode(CHALLENGE_DOMAIN),
      lengthPrefixed(input.purpose),
      lengthPrefixed(input.address),
      hexToBytes(input.challengeId),
      u64le(input.issuedAt),
      u64le(input.expiresAt),
    ]),
  );
}

export function buildAdminDigest(input: {
  action: string;
  chainId: number;
  gameId: string;
  name: string;
  description: string;
  status?: string;
  nonce: string;
  timestamp: number;
}): Uint8Array {
  if (!(Number.isInteger(input.chainId) && input.chainId > 0)) throw new Error("Choose an AppChain.");
  if (!NONCE_PATTERN.test(input.nonce)) throw new Error("The nonce must be 16 to 64 lowercase hexadecimal characters.");
  return sha3_256(
    concat([
      textEncoder.encode(ADMIN_DOMAIN),
      lengthPrefixed(input.action),
      u32le(input.chainId),
      lengthPrefixed(input.gameId),
      lengthPrefixed(input.name),
      lengthPrefixed(input.description),
      lengthPrefixed(input.status ?? ""),
      lengthPrefixed(input.nonce),
      u64le(input.timestamp),
    ]),
  );
}

/** The text a policy proposal is signed over. It must match what the service rebuilds from the terms. */
export function policySummary(rewardUnits: number, outcomes: string, perPlayerPerDay: number, perGamePerDay: number, effectiveFrom: number): string {
  return `${rewardUnits}|${outcomes}|${perPlayerPerDay}|${perGamePerDay}|${effectiveFrom}`;
}

async function signDigest(material: LocalWalletKeyMaterial, digest: Uint8Array): Promise<SignedGamesDigest> {
  const secretKey = new Uint8Array(material.secretKey);
  try {
    const signature = await signDilithium5(digest, secretKey);
    try {
      return { publicKeyHex: bytesToHex(material.publicKey), signatureHex: bytesToHex(signature) };
    } finally {
      signature.fill(0);
    }
  } finally {
    secretKey.fill(0);
  }
}

function randomNonce(): string {
  return bytesToHex(crypto.getRandomValues(new Uint8Array(16)));
}

/**
 * A provider that signs only Games requests, with the unlocked local wallet. It recomputes the sign-in digest from
 * the challenge's own fields and refuses to sign if the service's offered digest differs.
 */
export function createLocalGamesProvider(getMaterial: () => LocalWalletKeyMaterial | null): UltraWalletProvider {
  return {
    async request(request) {
      const material = getMaterial();
      if (!material) throw new Error("The local wallet is locked. Unlock it before signing.");

      if (request.method === ULTRA_WALLET_SIGN_GAMES_CHALLENGE) {
        const { challengeId, address, purpose, issuedAt, expiresAt, offeredDigest } = request.params;
        if (address !== material.address) throw new Error("This challenge is for a different wallet. Nothing was signed.");
        const digest = buildChallengeDigest({ purpose, address, challengeId, issuedAt, expiresAt });
        try {
          if (bytesToHex(digest) !== offeredDigest) {
            throw new Error("The Games service sent a sign-in challenge that does not match its own fields. Nothing was signed.");
          }
          return await signDigest(material, digest);
        } finally {
          digest.fill(0);
        }
      }

      if (request.method === ULTRA_WALLET_SIGN_GAME_ADMIN) {
        const { action, chainId, gameId, name, description } = request.params;
        const nonce = randomNonce();
        const timestamp = Math.floor(Date.now() / 1000);
        const digest = buildAdminDigest({ action, chainId, gameId, name, description, nonce, timestamp });
        try {
          return { ...(await signDigest(material, digest)), nonce, timestamp };
        } finally {
          digest.fill(0);
        }
      }

      throw new Error("This local wallet signs Games requests only through the Games pages.");
    },
  };
}
