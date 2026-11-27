import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  GamesApiError,
  claimDeposit,
  fetchGame,
  fetchPlayerResults,
  fetchStore,
  isGameId,
  logoutSession,
  parseChallenge,
  parseDeposit,
  parsePlayReport,
  parsePolicy,
  parsePool,
  parsePoolPayout,
  parseRewardRow,
  parseSession,
  parseStoreGame,
  play,
  registerGame,
  requestChallenge,
  verifyChallenge,
  withdrawFromPool,
} from "@/lib/games-api";

// The answers the Games service really gives: captured by a Rust test that drives the real routes and checks each
// answer's shape against this file. The Android app reads an identical copy.
const contract = JSON.parse(
  readFileSync(fileURLToPath(new URL("./games-api-contract.json", import.meta.url)), "utf8"),
) as Record<string, Record<string, unknown>>;

const firstOf = (name: string, key: string) => (contract[name][key] as unknown[])[0];

afterEach(() => vi.unstubAllGlobals());

function stubFetch(handler: (url: string, init: RequestInit) => unknown, status = 200) {
  const calls: Array<{ url: string; init: RequestInit }> = [];
  vi.stubGlobal(
    "fetch",
    vi.fn(async (url: string, init: RequestInit) => {
      calls.push({ url, init });
      return new Response(JSON.stringify(handler(url, init)), { status });
    }),
  );
  return calls;
}

describe("the real Games answers are readable", () => {
  it("every contract answer is a success answer and the contract is complete", () => {
    const names = Object.keys(contract);
    expect(names.length).toBeGreaterThanOrEqual(14);
    for (const name of names) expect(contract[name].success, name).toBe(true);
  });

  it("reads a store entry with its identity, owner, verifier and availability", () => {
    const game = parseStoreGame(firstOf("store", "games"));
    expect(isGameId(game.gameId)).toBe(true);
    expect(game.chainId).toBeGreaterThan(0);
    expect(game.owner).toMatch(/^[0-9a-f]{64}$/);
    expect(["available", "none"]).toContain(game.rewards);
    expect(typeof game.validationScheme).toBe("string");
  });

  it("reads a play and its read-back as the same reserved, queued, unpaid result", () => {
    const played = parsePlayReport(contract.play);
    const read = parsePlayReport(contract.result);
    expect(played.stage).toBe("reward_reserved");
    expect(read.stage).toBe(played.stage);
    expect(read.creditId).toBe(played.creditId);
    expect(played.payoutState).toBe("queued");
    expect(played.rewardUnits).toBe(100);
  });

  it("reads rewards, pools, deposits, policies, payouts, challenges and sessions", () => {
    expect(parseRewardRow(firstOf("player_results", "results")).rewardUnits).toBe(100);
    const pool = parsePool(contract.pool);
    expect(pool.funded - pool.reserved - pool.paid - pool.withdrawn - pool.feesReserved - pool.feesPaid).toBe(pool.available);
    expect(parseDeposit(firstOf("deposits", "deposits")).amountUnits).toBeGreaterThan(0);
    expect(parsePolicy(contract.policy_propose).status).toBe("proposed");
    expect(parsePolicy(firstOf("policies", "policies")).rewardUnits).toBeGreaterThan(0);
    expect(parsePoolPayout(firstOf("payouts", "payouts")).kind).toBe("reward");
    expect(Object.keys(firstOf("payouts", "payouts") as object)).not.toContain("envelope");
    expect(parseChallenge(contract.challenge).digest).toMatch(/^[0-9a-f]{64}$/);
    expect(parseSession(contract.session).token.length).toBeGreaterThan(0);
    expect(parsePool(contract.fund).funded).toBeGreaterThan(0);
    expect(parsePool(contract.withdraw).withdrawn).toBeGreaterThan(0);
  });
});

describe("strict reading: a wrong answer is an error, never a guess", () => {
  it("refuses an unknown result stage so it can never be mistaken for a paid one", () => {
    expect(() => parsePlayReport({ ...contract.play, stage: "reward_confirmed_maybe" })).toThrow(GamesApiError);
    expect(() => parseRewardRow({ ...(firstOf("player_results", "results") as object), stage: "paid" })).toThrow(GamesApiError);
  });

  it("refuses missing or mistyped required fields", () => {
    const game = firstOf("store", "games") as Record<string, unknown>;
    expect(() => parseStoreGame({ ...game, game_id: 7 })).toThrow(GamesApiError);
    expect(() => parseStoreGame({ ...game, chain_id: -1 })).toThrow(GamesApiError);
    expect(() => parseStoreGame({ ...game, rewards: "maybe" })).toThrow(GamesApiError);
    expect(() => parseStoreGame({ ...game, playable: "yes" })).toThrow(GamesApiError);
    expect(() => parsePool({ ...contract.pool, funded: "1000" })).toThrow(GamesApiError);
    expect(() => parseStoreGame(null)).toThrow(GamesApiError);
  });
});

describe("requests", () => {
  it("reads the catalog from the shared service, with no credentials and no caching", async () => {
    const calls = stubFetch(() => contract.store);
    const games = await fetchStore({ query: " echo ", chainId: 7 });
    expect(games.length).toBeGreaterThan(0);
    expect(calls[0].url).toContain("/api/games/store?");
    expect(calls[0].url).toContain("q=echo");
    expect(calls[0].url).toContain("chain_id=7");
    expect(calls[0].init.credentials).toBe("omit");
    expect(calls[0].init.cache).toBe("no-store");
  });

  it("finds a game by its id and never by a lookalike", async () => {
    const wanted = (firstOf("store", "games") as { game_id: string }).game_id;
    stubFetch(() => contract.store);
    expect((await fetchGame(wanted))?.gameId).toBe(wanted);
    expect(await fetchGame("f".repeat(32))).toBeNull();
  });

  it("turns a refusal into an error carrying the service's own words and code", async () => {
    stubFetch(() => ({ success: false, code: "auth_required", message: "sign in with your wallet first" }), 401);
    await expect(play("t", "a".repeat(64), "g".repeat(32), "r1", "01")).rejects.toMatchObject({ code: "auth_required", status: 401 });
  });

  it("says plainly when the service cannot be reached or answers with something else", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => { throw new TypeError("network"); }));
    await expect(fetchStore()).rejects.toMatchObject({ code: "unreachable" });
    vi.stubGlobal("fetch", vi.fn(async () => new Response("<html>bad gateway</html>", { status: 502 })));
    await expect(fetchStore()).rejects.toMatchObject({ code: "unexpected_response", status: 502 });
  });

  it("sends the session in its own header and the owner proof as five plain fields", async () => {
    const calls = stubFetch((url) => (url.includes("/play/") ? contract.play : url.includes("/fund") ? contract.fund : contract.pool));
    await play("tok", "a".repeat(64), "g".repeat(32), "r1", "01");
    const headers = calls[0].init.headers as Record<string, string>;
    expect(headers["X-Wallet-Session"]).toBe("tok");
    expect(JSON.parse(calls[0].init.body as string)).toEqual({ address: "a".repeat(64), result_id: "r1", payload: "01" });
    const proof = { nonce: "ab".repeat(16), timestamp: 1, publicKey: "aa", signature: "bb" };
    await claimDeposit("g".repeat(32), "c".repeat(64), proof);
    expect(Object.keys(JSON.parse(calls[1].init.body as string)).sort()).toEqual(["nonce", "public_key", "signature", "timestamp", "tx_hash"]);
    await withdrawFromPool("g".repeat(32), 400, proof);
    expect(JSON.parse(calls[2].init.body as string).amount_units).toBe(400);
  });

  it("authenticates with a challenge and a signature and ends a session on request", async () => {
    const calls = stubFetch((url) => (url.includes("/challenge") ? contract.challenge : url.includes("/verify") ? contract.session : { success: true }));
    expect((await requestChallenge("a".repeat(64))).purpose).toBe("play");
    expect((await verifyChallenge("c".repeat(64), "aa", "bb")).token.length).toBeGreaterThan(0);
    await logoutSession("tok");
    expect(JSON.parse(calls[0].init.body as string)).toEqual({ address: "a".repeat(64), purpose: "play" });
    expect(Object.keys(JSON.parse(calls[1].init.body as string)).sort()).toEqual(["challenge_id", "public_key", "signature"]);
  });

  it("registers a game and reads it back from the store by the id the service assigned", async () => {
    const assigned = (contract.catalog_register as { game_id: string }).game_id;
    const calls = stubFetch((url) => (url.endsWith("/catalog") ? contract.catalog_register : { success: true, games: [{ ...(firstOf("store", "games") as object), game_id: assigned }] }));
    const game = await registerGame(7, "Second", "d", { nonce: "ab".repeat(16), timestamp: 1, publicKey: "aa", signature: "bb" });
    expect(game.gameId).toBe(assigned);
    expect(calls).toHaveLength(2);
  });

  it("reads a wallet's rewards", async () => {
    stubFetch(() => contract.player_results);
    const rows = await fetchPlayerResults("a".repeat(64));
    expect(rows[0].stage).toBe("reward_reserved");
  });
});
