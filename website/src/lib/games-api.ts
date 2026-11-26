import { API_BASE_URL } from "@/lib/links";
import type {
  AdminProof,
  DepositRow,
  GamesSession,
  PlayReport,
  PolicyReport,
  PoolPayoutRow,
  PoolReport,
  ResultStage,
  RewardRow,
  StoreGame,
} from "@/lib/games.types";

/**
 * The client for the Games service. It is the same service and the same endpoints the Android app uses; this site has
 * no registry, reward database or ownership list of its own. Every response is checked for the fields this site relies
 * on: an answer that does not have them is an error, never something to guess around.
 */

export const GAMES_BASE = `${API_BASE_URL}/api/games`;

export class GamesApiError extends Error {
  constructor(
    message: string,
    public readonly code: string,
    public readonly status: number,
  ) {
    super(message);
    this.name = "GamesApiError";
  }
}

type Json = Record<string, unknown>;

function isRecord(value: unknown): value is Json {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function unexpected(what: string): never {
  throw new GamesApiError(`The Games service sent an answer this site does not understand (${what}).`, "unexpected_response", 502);
}

function str(raw: Json, key: string): string {
  const value = raw[key];
  if (typeof value !== "string") return unexpected(key);
  return value;
}

function optStr(raw: Json, key: string): string | null {
  const value = raw[key];
  if (value === null || value === undefined) return null;
  if (typeof value !== "string") return unexpected(key);
  return value;
}

function uint(raw: Json, key: string): number {
  const value = raw[key];
  if (typeof value !== "number" || !Number.isSafeInteger(value) || value < 0) return unexpected(key);
  return value;
}

function optUint(raw: Json, key: string): number | null {
  const value = raw[key];
  if (value === null || value === undefined) return null;
  return uint(raw, key);
}

function bool(raw: Json, key: string): boolean {
  const value = raw[key];
  if (typeof value !== "boolean") return unexpected(key);
  return value;
}

function list(raw: Json, key: string): Json[] {
  const value = raw[key];
  if (!Array.isArray(value)) return unexpected(key);
  return value.map((item) => (isRecord(item) ? item : unexpected(key)));
}

const STAGES: readonly string[] = ["verified", "reward_reserved", "reward_paid", "reward_released"];

function stage(raw: Json, key: string): ResultStage {
  const value = str(raw, key);
  // An unknown stage is refused rather than shown: it must never be mistaken for a paid one.
  if (!STAGES.includes(value)) return unexpected(key);
  return value as ResultStage;
}

export function parseStoreGame(raw: unknown): StoreGame {
  if (!isRecord(raw)) return unexpected("game");
  const rewards = str(raw, "rewards");
  if (rewards !== "available" && rewards !== "none") return unexpected("rewards");
  return {
    gameId: str(raw, "game_id"),
    chainId: uint(raw, "chain_id"),
    name: str(raw, "name"),
    description: typeof raw.description === "string" ? raw.description : "",
    status: str(raw, "status"),
    validationScheme: str(raw, "validation_scheme"),
    adapterHash: optStr(raw, "adapter_hash"),
    owner: optStr(raw, "owner"),
    playable: bool(raw, "playable"),
    playableReason: optStr(raw, "playable_reason"),
    rewards,
    rewardUnits: optUint(raw, "reward_units"),
    rewardsReason: optStr(raw, "rewards_reason"),
  };
}

export function parsePlayReport(raw: unknown): PlayReport {
  if (!isRecord(raw)) return unexpected("result");
  return {
    gameId: str(raw, "game_id"),
    resultId: str(raw, "result_id"),
    outcome: typeof raw.outcome === "string" ? raw.outcome : "invalid",
    stage: stage(raw, "stage"),
    rewardUnits: optUint(raw, "reward_units"),
    creditId: optStr(raw, "credit_id"),
    payoutState: optStr(raw, "payout_state"),
    notRewarded: optStr(raw, "not_rewarded"),
  };
}

export function parseRewardRow(raw: unknown): RewardRow {
  if (!isRecord(raw)) return unexpected("reward");
  return {
    gameId: str(raw, "game_id"),
    resultId: str(raw, "result_id"),
    rewardUnits: uint(raw, "reward_units"),
    stage: stage(raw, "stage"),
    payoutState: optStr(raw, "payout_state"),
    transactionHash: optStr(raw, "transaction_hash"),
    at: uint(raw, "at"),
  };
}

export function parsePool(raw: unknown): PoolReport {
  if (!isRecord(raw)) return unexpected("pool");
  return {
    funded: uint(raw, "funded"),
    reserved: uint(raw, "reserved"),
    paid: uint(raw, "paid"),
    withdrawn: uint(raw, "withdrawn"),
    feesReserved: uint(raw, "fees_reserved"),
    feesPaid: uint(raw, "fees_paid"),
    available: uint(raw, "available"),
  };
}

export function parseDeposit(raw: unknown): DepositRow {
  if (!isRecord(raw)) return unexpected("deposit");
  return { txHash: str(raw, "tx_hash"), funder: str(raw, "funder"), amountUnits: uint(raw, "amount_units"), claimedAt: uint(raw, "claimed_at") };
}

export function parsePolicy(raw: unknown): PolicyReport {
  if (!isRecord(raw)) return unexpected("policy");
  return {
    version: uint(raw, "version"),
    rewardUnits: uint(raw, "reward_units"),
    eligibleOutcomes: str(raw, "eligible_outcomes"),
    perPlayerPerDay: uint(raw, "per_player_per_day"),
    perGamePerDay: uint(raw, "per_game_per_day"),
    status: str(raw, "status"),
    decisionNote: optStr(raw, "decision_note"),
  };
}

export function parsePoolPayout(raw: unknown): PoolPayoutRow {
  if (!isRecord(raw)) return unexpected("payout");
  return {
    kind: str(raw, "kind"),
    amountUnits: uint(raw, "amount_units"),
    toAddress: str(raw, "to_address"),
    state: str(raw, "state"),
    transactionHash: optStr(raw, "tx_hash"),
  };
}

export function parseSession(raw: unknown): GamesSession {
  if (!isRecord(raw)) return unexpected("session");
  return { token: str(raw, "token"), address: str(raw, "address"), expiresAt: uint(raw, "expires_at") };
}

export interface GamesChallenge {
  challengeId: string;
  address: string;
  purpose: string;
  issuedAt: number;
  expiresAt: number;
  digest: string;
}

export function parseChallenge(raw: unknown): GamesChallenge {
  if (!isRecord(raw)) return unexpected("challenge");
  return {
    challengeId: str(raw, "challenge_id"),
    address: str(raw, "address"),
    purpose: str(raw, "purpose"),
    issuedAt: uint(raw, "issued_at"),
    expiresAt: uint(raw, "expires_at"),
    digest: str(raw, "digest"),
  };
}

interface RequestOptions {
  method?: "GET" | "POST";
  body?: unknown;
  session?: string;
  signal?: AbortSignal;
}

async function request(path: string, options: RequestOptions = {}): Promise<Json> {
  const headers: Record<string, string> = { Accept: "application/json" };
  if (options.body !== undefined) headers["Content-Type"] = "application/json";
  if (options.session) headers["X-Wallet-Session"] = options.session;
  let response: Response;
  try {
    response = await fetch(`${GAMES_BASE}${path}`, {
      method: options.method ?? "GET",
      headers,
      body: options.body === undefined ? undefined : JSON.stringify(options.body),
      cache: "no-store",
      credentials: "omit",
      signal: options.signal,
    });
  } catch (error) {
    if (error instanceof DOMException && error.name === "AbortError") throw error;
    throw new GamesApiError("The Games service could not be reached. Check your connection and try again.", "unreachable", 0);
  }
  let payload: unknown = null;
  try {
    payload = await response.json();
  } catch {
    payload = null;
  }
  if (!isRecord(payload)) {
    throw new GamesApiError(`The Games service answered with something unexpected (HTTP ${response.status}).`, "unexpected_response", response.status);
  }
  if (payload.success !== true) {
    const message = typeof payload.message === "string" ? payload.message : "The Games service refused the request.";
    const code = typeof payload.code === "string" ? payload.code : "refused";
    throw new GamesApiError(message, code, response.status);
  }
  return payload;
}

function proofBody(proof: AdminProof): Json {
  return { nonce: proof.nonce, timestamp: proof.timestamp, public_key: proof.publicKey, signature: proof.signature };
}

// --- public reads: no wallet needed ----------------------------------------------------------------

export async function fetchStore(options: { query?: string; chainId?: number; signal?: AbortSignal } = {}): Promise<StoreGame[]> {
  const params = new URLSearchParams();
  if (options.query?.trim()) params.set("q", options.query.trim());
  if (options.chainId !== undefined) params.set("chain_id", String(options.chainId));
  const suffix = params.size > 0 ? `?${params.toString()}` : "";
  const payload = await request(`/store${suffix}`, { signal: options.signal });
  return list(payload, "games").map(parseStoreGame);
}

/** One game by its id. The id is the identity: the lookup is by id, and the answer must carry that same id. */
export async function fetchGame(gameId: string, signal?: AbortSignal): Promise<StoreGame | null> {
  const games = await fetchStore({ query: gameId, signal });
  return games.find((game) => game.gameId === gameId) ?? null;
}

export async function fetchPool(gameId: string, signal?: AbortSignal): Promise<PoolReport> {
  return parsePool(await request(`/pools/${gameId}`, { signal }));
}

export async function fetchDeposits(gameId: string, signal?: AbortSignal): Promise<DepositRow[]> {
  return list(await request(`/pools/${gameId}/deposits`, { signal }), "deposits").map(parseDeposit);
}

export async function fetchPolicies(gameId: string, signal?: AbortSignal): Promise<PolicyReport[]> {
  return list(await request(`/pools/${gameId}/policies`, { signal }), "policies").map(parsePolicy);
}

export async function fetchPoolPayouts(gameId: string, signal?: AbortSignal): Promise<PoolPayoutRow[]> {
  return list(await request(`/pools/${gameId}/payouts`, { signal }), "payouts").map(parsePoolPayout);
}

export async function fetchPlayerResults(address: string, signal?: AbortSignal): Promise<RewardRow[]> {
  return list(await request(`/players/${address}/results`, { signal }), "results").map(parseRewardRow);
}

export async function fetchResult(gameId: string, resultId: string, signal?: AbortSignal): Promise<PlayReport> {
  return parsePlayReport(await request(`/results/${gameId}/${encodeURIComponent(resultId)}`, { signal }));
}

// --- wallet-authenticated play ---------------------------------------------------------------------

export async function requestChallenge(address: string): Promise<GamesChallenge> {
  return parseChallenge(await request("/auth/challenge", { method: "POST", body: { address, purpose: "play" } }));
}

export async function verifyChallenge(challengeId: string, publicKeyHex: string, signatureHex: string): Promise<GamesSession> {
  return parseSession(
    await request("/auth/verify", { method: "POST", body: { challenge_id: challengeId, public_key: publicKeyHex, signature: signatureHex } }),
  );
}

export async function logoutSession(token: string): Promise<void> {
  await request("/auth/logout", { method: "POST", body: {}, session: token });
}

export async function play(session: string, address: string, gameId: string, resultId: string, payloadHex: string): Promise<PlayReport> {
  return parsePlayReport(
    await request(`/play/${gameId}`, { method: "POST", session, body: { address, result_id: resultId, payload: payloadHex } }),
  );
}

// --- owner actions: each carries the owner's signature, checked by the service against the L1 owner ----

export async function registerGame(chainId: number, name: string, description: string, proof: AdminProof): Promise<StoreGame> {
  const payload = await request("/catalog", { method: "POST", body: { chain_id: chainId, name, description, ...proofBody(proof) } });
  // The answer is the catalog's own record, flat. It is not a store entry (no owner or availability), so read the
  // entry back from the store by the id the service just assigned.
  const found = await fetchGame(str(payload, "game_id"));
  if (!found) return unexpected("registered game missing from the store");
  return found;
}

export async function claimDeposit(gameId: string, txHash: string, proof: AdminProof): Promise<PoolReport> {
  return parsePool(await request(`/pools/${gameId}/fund`, { method: "POST", body: { tx_hash: txHash, ...proofBody(proof) } }));
}

export async function proposePolicy(
  gameId: string,
  terms: { rewardUnits: number; outcomes: string; perPlayerPerDay: number; perGamePerDay: number },
  proof: AdminProof,
): Promise<PolicyReport> {
  return parsePolicy(
    await request(`/pools/${gameId}/policy`, {
      method: "POST",
      body: {
        reward_units: terms.rewardUnits,
        eligible_outcomes: terms.outcomes,
        per_player_per_day: terms.perPlayerPerDay,
        per_game_per_day: terms.perGamePerDay,
        ...proofBody(proof),
      },
    }),
  );
}

export async function withdrawFromPool(gameId: string, amountUnits: number, proof: AdminProof): Promise<PoolReport> {
  return parsePool(await request(`/pools/${gameId}/withdraw`, { method: "POST", body: { amount_units: amountUnits, ...proofBody(proof) } }));
}

export function isGameId(value: string): boolean {
  return /^[0-9a-f]{32}$/.test(value);
}
