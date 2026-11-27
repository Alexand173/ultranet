import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterAll, beforeAll, describe, expect, it, vi } from "vitest";
import { verifyDilithium5 } from "@/lib/dilithium-wasm";
import { buildAdminDigest, buildChallengeDigest, createLocalGamesProvider } from "@/lib/games-signing";
import { ULTRA_WALLET_SIGN_GAME_ADMIN, ULTRA_WALLET_SIGN_GAMES_CHALLENGE, type SignedGamesDigest } from "@/lib/ultra-wallet";
import { bytesToHex, createLocalWalletFromPhrase, hexToBytes } from "@/lib/wallet-crypto";

// A real Dilithium-5 key, the real WebAssembly signer, and the real verifier: the signature the Games pages produce is
// one the verifier accepts over exactly the digest the service recomputes, and over nothing else.
const wasm = readFileSync(fileURLToPath(new URL("../../public/crypto/ultra-dilithium5.wasm", import.meta.url)));
const phrase = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";

beforeAll(() => {
  vi.stubGlobal("fetch", vi.fn(async () => new Response(wasm, { status: 200 })));
});
afterAll(() => vi.unstubAllGlobals());

describe("a real wallet signs the Games requests", () => {
  it("signs the sign-in digest it computed itself, and the verifier accepts it for that digest only", async () => {
    const material = await createLocalWalletFromPhrase(phrase);
    const provider = createLocalGamesProvider(() => material);
    const challenge = { challengeId: "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff", purpose: "play", issuedAt: 1_800_000_000, expiresAt: 1_800_000_300 };
    const digest = buildChallengeDigest({ ...challenge, address: material.address });
    const signed = (await provider.request({
      method: ULTRA_WALLET_SIGN_GAMES_CHALLENGE,
      params: { ...challenge, address: material.address, offeredDigest: bytesToHex(digest) },
    })) as SignedGamesDigest;
    const publicKey = hexToBytes(signed.publicKeyHex);
    expect(publicKey).toEqual(material.publicKey);
    expect(await verifyDilithium5(hexToBytes(signed.signatureHex), digest, publicKey)).toBe(true);
    const other = buildChallengeDigest({ ...challenge, address: material.address, expiresAt: 1_800_000_301 });
    expect(await verifyDilithium5(hexToBytes(signed.signatureHex), other, publicKey)).toBe(false);
  }, 60_000);

  it("signs an owner action with a fresh nonce each time, over the digest the service rebuilds", async () => {
    const material = await createLocalWalletFromPhrase(phrase);
    const provider = createLocalGamesProvider(() => material);
    const params = { action: "pool-withdraw", chainId: 7, gameId: "00112233445566778899aabbccddeeff", name: "400", description: "" };
    const first = (await provider.request({ method: ULTRA_WALLET_SIGN_GAME_ADMIN, params })) as SignedGamesDigest;
    const second = (await provider.request({ method: ULTRA_WALLET_SIGN_GAME_ADMIN, params })) as SignedGamesDigest;
    expect(first.nonce).toMatch(/^[0-9a-f]{32}$/);
    expect(first.nonce).not.toBe(second.nonce);
    const digest = buildAdminDigest({ ...params, nonce: first.nonce!, timestamp: first.timestamp! });
    expect(await verifyDilithium5(hexToBytes(first.signatureHex), digest, material.publicKey)).toBe(true);
    // The same signature over a different amount is not valid: an owner cannot be made to sign a larger withdrawal.
    const larger = buildAdminDigest({ ...params, name: "4000", nonce: first.nonce!, timestamp: first.timestamp! });
    expect(await verifyDilithium5(hexToBytes(first.signatureHex), larger, material.publicKey)).toBe(false);
  }, 60_000);
});
