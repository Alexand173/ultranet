import { describe, expect, it } from "vitest";
import { bytesToHex } from "@/lib/wallet-crypto";
import { buildAdminDigest, buildChallengeDigest, createLocalGamesProvider, policySummary } from "@/lib/games-signing";
import { ULTRA_WALLET_SIGN_GAME_ADMIN, ULTRA_WALLET_SIGN_GAMES_CHALLENGE } from "@/lib/ultra-wallet";

// The same vectors are pinned in src/games/tests.rs (the server) and in the Android tests.
const address = "7d2cd1cc30c1088e0382b040c8b3832acf9e45feae88624d06c42cd3f9d9388e";
const challengeId = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
const nonce = "ab".repeat(16);
const game = "00112233445566778899aabbccddeeff";
const hex = (bytes: Uint8Array) => bytesToHex(bytes);

describe("digests match the server's vectors byte for byte", () => {
  it("sign-in challenge", () => {
    expect(hex(buildChallengeDigest({ purpose: "play", address, challengeId, issuedAt: 1_800_000_000, expiresAt: 1_800_000_300 }))).toBe(
      "2d1074fd8db57d0497d6bf5f07a8963db1806425b147fe5a350233271e60e020",
    );
  });

  it("owner actions", () => {
    expect(hex(buildAdminDigest({ action: "pool-withdraw", chainId: 7, gameId: game, name: "400", description: "", nonce, timestamp: 1_800_000_000 }))).toBe(
      "8006596d5a34eaf610a746f8ef3bf172dddd2c374294ef27562a68ea85d62930",
    );
    expect(hex(buildAdminDigest({ action: "register", chainId: 7, gameId: "", name: "Echo Game", description: "a short description", nonce, timestamp: 1_800_000_000 }))).toBe(
      "f78faa7dee8619104d2ed305802a17a7ba5aaffdb87df06c257ed1ed9abd29cd",
    );
  });

  it("every field is inside the digest, so a signature cannot be moved", () => {
    const base = hex(buildAdminDigest({ action: "pool-withdraw", chainId: 7, gameId: game, name: "400", description: "", nonce, timestamp: 1 }));
    const variants = [
      { action: "pool-fund" }, { chainId: 8 }, { gameId: game.replace("0", "1") }, { name: "401" }, { description: "x" },
      { nonce: "cd".repeat(16) }, { timestamp: 2 },
    ];
    for (const change of variants) {
      expect(hex(buildAdminDigest({ action: "pool-withdraw", chainId: 7, gameId: game, name: "400", description: "", nonce, timestamp: 1, ...change })), JSON.stringify(change)).not.toBe(base);
    }
    const challenge = hex(buildChallengeDigest({ purpose: "play", address, challengeId, issuedAt: 1, expiresAt: 2 }));
    expect(hex(buildChallengeDigest({ purpose: "play", address: "8" + address.slice(1), challengeId, issuedAt: 1, expiresAt: 2 }))).not.toBe(challenge);
    expect(hex(buildChallengeDigest({ purpose: "play", address, challengeId, issuedAt: 1, expiresAt: 3 }))).not.toBe(challenge);
  });

  it("refuses to build a digest for another purpose or a malformed field", () => {
    expect(() => buildChallengeDigest({ purpose: "withdraw", address, challengeId, issuedAt: 1, expiresAt: 2 })).toThrow();
    expect(() => buildChallengeDigest({ purpose: "play", address: address.toUpperCase(), challengeId, issuedAt: 1, expiresAt: 2 })).toThrow();
    expect(() => buildChallengeDigest({ purpose: "play", address, challengeId: challengeId.slice(2), issuedAt: 1, expiresAt: 2 })).toThrow();
    expect(() => buildChallengeDigest({ purpose: "play", address, challengeId, issuedAt: 5, expiresAt: 2 })).toThrow();
    expect(() => buildAdminDigest({ action: "register", chainId: 0, gameId: "", name: "x", description: "", nonce, timestamp: 1 })).toThrow();
    expect(() => buildAdminDigest({ action: "register", chainId: 7, gameId: "", name: "x", description: "", nonce: "short", timestamp: 1 })).toThrow();
  });

  it("builds the policy text the service rebuilds", () => {
    expect(policySummary(100, "win", 3, 100, 0)).toBe("100|win|3|100|0");
  });
});

describe("the local Games provider", () => {
  it("is locked without key material and signs nothing", async () => {
    const provider = createLocalGamesProvider(() => null);
    await expect(
      provider.request({
        method: ULTRA_WALLET_SIGN_GAME_ADMIN,
        params: { action: "register", chainId: 7, gameId: "", name: "x", description: "" },
      }),
    ).rejects.toThrow(/locked/);
  });

  it("refuses a challenge for another wallet or one whose digest is not the wallet's own", async () => {
    const material = { address, publicKey: new Uint8Array(2592), secretKey: new Uint8Array(4896), seed: new Uint8Array(32), createdAt: 0 };
    const provider = createLocalGamesProvider(() => material);
    const fields = { challengeId, purpose: "play", issuedAt: 1_800_000_000, expiresAt: 1_800_000_300 };
    await expect(
      provider.request({ method: ULTRA_WALLET_SIGN_GAMES_CHALLENGE, params: { ...fields, address: "8" + address.slice(1), offeredDigest: "00".repeat(32) } }),
    ).rejects.toThrow(/different wallet/);
    await expect(
      provider.request({ method: ULTRA_WALLET_SIGN_GAMES_CHALLENGE, params: { ...fields, address, offeredDigest: "00".repeat(32) } }),
    ).rejects.toThrow(/does not match/);
  });
});
