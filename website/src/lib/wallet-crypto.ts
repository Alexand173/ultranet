import { mnemonicToSeedSync, generateMnemonic, validateMnemonic } from "@scure/bip39";
import { wordlist } from "@scure/bip39/wordlists/english.js";
import { sha3_256 } from "@noble/hashes/sha3.js";
import {
  clearKeyPair,
  generateDilithium5KeyPair,
  signDilithium5,
} from "@/lib/dilithium-wasm";

export const WALLET_CRYPTO_VERSION = 1 as const;
export const WALLET_KDF_ITERATIONS = 310_000 as const;
export const WALLET_KDF_SALT_BYTES = 16 as const;
export const WALLET_AES_IV_BYTES = 12 as const;
export const ULTRA_DECIMALS = 6 as const;
export const ULTRA_BASE_UNITS_PER_ULTRA = 1_000_000 as const;
export const GENESIS_ALLOCATION_ULTRA = 1_000_000 as const;
export const GENESIS_ALLOCATION_BASE_UNITS =
  GENESIS_ALLOCATION_ULTRA * ULTRA_BASE_UNITS_PER_ULTRA;

const KEY_DERIVATION_DOMAIN = new TextEncoder().encode("UltraNet/Dilithium5/keygen/v1");

export interface LocalWalletIdentity {
  readonly address: string;
  readonly publicKey: Uint8Array;
  readonly createdAt: number;
}

export interface LocalWalletKeyMaterial extends LocalWalletIdentity {
  readonly seed: Uint8Array;
  readonly secretKey: Uint8Array;
}

export interface EncryptedWalletSeed {
  readonly ciphertext: string;
  readonly iv: string;
  readonly salt: string;
  readonly iterations: typeof WALLET_KDF_ITERATIONS;
}

export interface StoredWallet {
  readonly version: typeof WALLET_CRYPTO_VERSION;
  readonly address: string;
  readonly publicKey: string;
  readonly encryptedSeed: EncryptedWalletSeed;
  readonly createdAt: number;
  readonly updatedAt: number;
}

function concatBytes(...parts: Uint8Array[]): Uint8Array {
  const length = parts.reduce((total, part) => total + part.length, 0);
  const result = new Uint8Array(length);
  let offset = 0;
  for (const part of parts) {
    result.set(part, offset);
    offset += part.length;
  }
  return result;
}

export function bytesToHex(bytes: Uint8Array): string {
  let result = "";
  for (const byte of bytes) result += byte.toString(16).padStart(2, "0");
  return result;
}

export function hexToBytes(value: string): Uint8Array {
  const normalized = value.trim().replace(/^0x/i, "");
  if (normalized.length === 0 || normalized.length % 2 !== 0 || !/^[0-9a-f]+$/i.test(normalized)) {
    throw new Error("Expected an even-length hexadecimal value.");
  }
  const bytes = new Uint8Array(normalized.length / 2);
  for (let index = 0; index < bytes.length; index++) {
    bytes[index] = Number.parseInt(normalized.slice(index * 2, index * 2 + 2), 16);
  }
  return bytes;
}

function bytesToBase64(bytes: Uint8Array): string {
  let binary = "";
  for (const byte of bytes) binary += String.fromCharCode(byte);
  return btoa(binary);
}

function base64ToBytes(value: string): Uint8Array {
  const binary = atob(value);
  const bytes = new Uint8Array(binary.length);
  for (let index = 0; index < binary.length; index++) bytes[index] = binary.charCodeAt(index);
  return bytes;
}

function ensureBrowserCrypto(): Crypto {
  if (typeof crypto === "undefined" || !crypto.subtle) {
    throw new Error("This browser does not provide the cryptography required for a local wallet.");
  }
  return crypto;
}

export function normalizeRecoveryPhrase(phrase: string): string {
  return phrase.trim().toLowerCase().split(/\s+/).join(" ");
}

export function isRecoveryPhraseValid(phrase: string): boolean {
  return validateMnemonic(normalizeRecoveryPhrase(phrase), wordlist);
}

export function createRecoveryPhrase(): string {
  return generateMnemonic(wordlist, 128);
}

export function deriveDilithiumSeedFromPhrase(phrase: string): Uint8Array {
  const normalized = normalizeRecoveryPhrase(phrase);
  if (!validateMnemonic(normalized, wordlist)) {
    throw new Error("Enter a valid 12-word recovery phrase.");
  }

  const bip39Seed = mnemonicToSeedSync(normalized);
  try {
    return sha3_256(concatBytes(KEY_DERIVATION_DOMAIN, bip39Seed));
  } finally {
    bip39Seed.fill(0);
  }
}

export function addressFromPublicKey(publicKey: Uint8Array): string {
  return bytesToHex(sha3_256(publicKey));
}

export async function createLocalWalletIdentity(
  seed: Uint8Array,
  createdAt = Date.now(),
): Promise<LocalWalletKeyMaterial> {
  if (seed.length !== 32) throw new Error("A wallet seed must contain 32 bytes.");
  const keyPair = await generateDilithium5KeyPair(seed);
  const publicKey = new Uint8Array(keyPair.publicKey);
  const address = addressFromPublicKey(publicKey);
  return {
    address,
    publicKey,
    seed: new Uint8Array(seed),
    secretKey: keyPair.secretKey,
    createdAt,
  };
}

export async function createLocalWalletFromPhrase(
  phrase: string,
  createdAt = Date.now(),
): Promise<LocalWalletKeyMaterial> {
  const seed = deriveDilithiumSeedFromPhrase(phrase);
  try {
    return await createLocalWalletIdentity(seed, createdAt);
  } finally {
    seed.fill(0);
  }
}

export async function createLocalWalletFromRandomSeed(
  createdAt = Date.now(),
): Promise<LocalWalletKeyMaterial> {
  const seed = ensureBrowserCrypto().getRandomValues(new Uint8Array(32));
  try {
    return await createLocalWalletIdentity(seed, createdAt);
  } finally {
    seed.fill(0);
  }
}

export async function deriveIdentityFromStoredSeed(
  seed: Uint8Array,
  publicKey: Uint8Array,
  address: string,
  createdAt: number,
): Promise<LocalWalletKeyMaterial> {
  const identity = await createLocalWalletIdentity(seed, createdAt);
  const expectedAddress = addressFromPublicKey(publicKey);
  if (identity.address !== address || expectedAddress !== address || !identity.publicKey.every((byte, index) => byte === publicKey[index])) {
    clearKeyPair(identity);
    identity.seed.fill(0);
    throw new Error("The stored wallet identity does not match its encrypted key material.");
  }
  return identity;
}

async function deriveEncryptionKey(password: string, salt: Uint8Array, iterations: number): Promise<CryptoKey> {
  if (password.length < 12) throw new Error("Use at least 12 characters for the wallet password.");
  const webCrypto = ensureBrowserCrypto();
  const passwordBytes = new TextEncoder().encode(password);
  let material: CryptoKey;
  try {
    material = await webCrypto.subtle.importKey(
      "raw",
      passwordBytes,
      "PBKDF2",
      false,
      ["deriveKey"],
    );
  } finally {
    passwordBytes.fill(0);
  }
  return webCrypto.subtle.deriveKey(
    { name: "PBKDF2", salt, iterations, hash: "SHA-256" },
    material,
    { name: "AES-GCM", length: 256 },
    false,
    ["encrypt", "decrypt"],
  );
}

export async function encryptWalletSeed(seed: Uint8Array, password: string): Promise<EncryptedWalletSeed> {
  if (seed.length !== 32) throw new Error("A wallet seed must contain 32 bytes.");
  const webCrypto = ensureBrowserCrypto();
  const salt = webCrypto.getRandomValues(new Uint8Array(WALLET_KDF_SALT_BYTES));
  const iv = webCrypto.getRandomValues(new Uint8Array(WALLET_AES_IV_BYTES));
  const key = await deriveEncryptionKey(password, salt, WALLET_KDF_ITERATIONS);
  const ciphertext = await webCrypto.subtle.encrypt({ name: "AES-GCM", iv }, key, seed);
  return {
    ciphertext: bytesToBase64(new Uint8Array(ciphertext)),
    iv: bytesToBase64(iv),
    salt: bytesToBase64(salt),
    iterations: WALLET_KDF_ITERATIONS,
  };
}

export async function decryptWalletSeed(record: EncryptedWalletSeed, password: string): Promise<Uint8Array> {
  const webCrypto = ensureBrowserCrypto();
  const salt = base64ToBytes(record.salt);
  const iv = base64ToBytes(record.iv);
  if (salt.length !== WALLET_KDF_SALT_BYTES || iv.length !== WALLET_AES_IV_BYTES) {
    throw new Error("The stored wallet encryption metadata is invalid.");
  }
  const key = await deriveEncryptionKey(password, salt, record.iterations);
  try {
    const plaintext = await webCrypto.subtle.decrypt(
      { name: "AES-GCM", iv },
      key,
      base64ToBytes(record.ciphertext),
    );
    const seed = new Uint8Array(plaintext);
    if (seed.length !== 32) throw new Error("The decrypted wallet seed has an invalid length.");
    return seed;
  } catch {
    throw new Error("That password did not unlock this wallet.");
  }
}

export function storedPublicKeyToBytes(wallet: StoredWallet): Uint8Array {
  const publicKey = hexToBytes(wallet.publicKey);
  if (publicKey.length !== 2_592) throw new Error("The stored wallet public key is invalid.");
  return publicKey;
}

export async function signWithLocalWallet(
  material: LocalWalletKeyMaterial,
  message: Uint8Array,
): Promise<Uint8Array> {
  try {
    return await signDilithium5(message, material.secretKey);
  } finally {
    clearKeyPair(material);
    material.seed.fill(0);
  }
}

export function clearLocalWalletKeyMaterial(material: LocalWalletKeyMaterial): void {
  clearKeyPair(material);
  material.seed.fill(0);
}

export function keyMaterialToStoredWallet(
  identity: LocalWalletIdentity,
  encryptedSeed: EncryptedWalletSeed,
): StoredWallet {
  return {
    version: WALLET_CRYPTO_VERSION,
    address: identity.address,
    publicKey: bytesToHex(identity.publicKey),
    encryptedSeed,
    createdAt: identity.createdAt,
    updatedAt: Date.now(),
  };
}

export function splitRecoveryPhrase(phrase: string): string[] {
  return normalizeRecoveryPhrase(phrase).split(" ");
}
