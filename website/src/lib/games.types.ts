/**
 * The shapes the Games service returns, as this site reads them. They are the same facts the Android app reads from
 * the same endpoints (`GET /api/games/store` and friends): one catalog, one set of ids, one set of statuses.
 */

export type GameRewardsKind = "available" | "none";

export interface StoreGame {
  /** The stable identity of a game. Never the display name. */
  gameId: string;
  /** The AppChain the game lives on. */
  chainId: number;
  name: string;
  description: string;
  /** The catalog status: `active` or `disabled`. */
  status: string;
  /** `record_only` (results are only recorded) or `verified_adapter` (a platform-approved verifier judges them). */
  validationScheme: string;
  /** SHA3-256 of the approved verifier module: the verifier's identity. Null while record only. */
  adapterHash: string | null;
  /** The chain's current L1 owner as the service read it from the ledger now; null if it could not be read. */
  owner: string | null;
  playable: boolean;
  playableReason: string | null;
  rewards: GameRewardsKind;
  rewardUnits: number | null;
  rewardsReason: string | null;
}

/** What a result has reached. Only `reward_paid` means the network confirmed a payment. */
export type ResultStage = "verified" | "reward_reserved" | "reward_paid" | "reward_released";

export interface PlayReport {
  gameId: string;
  resultId: string;
  outcome: string;
  stage: ResultStage;
  rewardUnits: number | null;
  creditId: string | null;
  payoutState: string | null;
  notRewarded: string | null;
}

export interface RewardRow {
  gameId: string;
  resultId: string;
  rewardUnits: number;
  stage: ResultStage;
  payoutState: string | null;
  transactionHash: string | null;
  at: number;
}

export interface PoolReport {
  funded: number;
  reserved: number;
  paid: number;
  withdrawn: number;
  feesReserved: number;
  feesPaid: number;
  available: number;
}

export interface DepositRow {
  txHash: string;
  funder: string;
  amountUnits: number;
  claimedAt: number;
}

export interface PolicyReport {
  version: number;
  rewardUnits: number;
  eligibleOutcomes: string;
  perPlayerPerDay: number;
  perGamePerDay: number;
  status: string;
  decisionNote: string | null;
}

export interface PoolPayoutRow {
  kind: string;
  amountUnits: number;
  toAddress: string;
  state: string;
  transactionHash: string | null;
}

/** The proof fields every owner-signed request carries. */
export interface AdminProof {
  nonce: string;
  timestamp: number;
  publicKey: string;
  signature: string;
}

export interface GamesSession {
  token: string;
  address: string;
  /** Seconds since the Unix epoch. */
  expiresAt: number;
}
