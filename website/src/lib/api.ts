import { API_BASE_URL } from "@/lib/links";
import type { SignedTransaction } from "@/lib/ultra-wallet";

export type LiveMetricStatus = "loading" | "live" | "unavailable";
export type TransactionStatus = "pending" | "confirmed" | "failed";

export interface LiveNetworkStats {
  totalBlocks: number | null;
  validatorCount: number | null;
  totalWeight: number | null;
  tps: number | null;
  provingTimeMs: number | null;
  activeAppchains: number | null;
  mempoolSize: number | null;
  totalTransactions: number | null;
  status: LiveMetricStatus;
  updatedAt: string | null;
}

export interface LatestTransaction {
  id: string;
  hash: string;
  amount: string;
  shard: number | null;
}

export interface AccountSnapshot {
  address: string;
  /** Compatibility alias: integer protocol base units. */
  balance: number;
  /** Canonical integer account balance in base units. */
  balanceBaseUnits: number;
  nonce: number;
  decimals: number;
  updatedAt: number;
}

export interface FeeEstimate {
  recipient: string;
  amount: number;
  fee: number;
  gasLimit: number;
  gasPrice: number;
  total: number;
}

export interface TransactionRecord {
  id: string;
  hash: string;
  sender: string;
  recipient: string;
  amount: number;
  fee: number;
  nonce: number;
  timestamp: number;
  status: TransactionStatus;
}

interface StatsPayload {
  success?: boolean;
  data?: Record<string, unknown>;
  message?: string;
}

interface TransactionsPayload {
  success?: boolean;
  transactions?: Array<Record<string, unknown>>;
  message?: string;
}

interface DataPayload<T> {
  success?: boolean;
  data?: T;
  message?: string;
}

interface AccountPayload {
  address?: unknown;
  balance?: unknown;
  balance_base_units?: unknown;
  nonce?: unknown;
  decimals?: unknown;
  updated_at?: unknown;
}

interface FeePayload {
  recipient?: unknown;
  amount?: unknown;
  fee?: unknown;
  gas_limit?: unknown;
  gas_price?: unknown;
  total?: unknown;
}

interface AddressHistoryPayload {
  success?: boolean;
  transactions?: Array<Record<string, unknown>>;
  message?: string;
}

function parseNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) return value;
  if (typeof value !== "string" || value.trim() === "") return null;

  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : null;
}

function parseString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function parseStatus(value: unknown, fallback: TransactionStatus = "confirmed"): TransactionStatus {
  return value === "pending" || value === "confirmed" || value === "failed" ? value : fallback;
}

function apiMessage(payload: { message?: unknown }, fallback: string): string {
  return typeof payload.message === "string" && payload.message.trim() ? payload.message : fallback;
}

function normalizeTransactionRecord(
  transaction: Record<string, unknown>,
  fallbackStatus: TransactionStatus = "confirmed",
): TransactionRecord | null {
  const hash = parseString(transaction.hash);
  const sender = parseString(transaction.sender);
  const recipient = parseString(transaction.recipient);
  const amount = parseNumber(transaction.amount);
  const fee = parseNumber(transaction.fee);
  const nonce = parseNumber(transaction.nonce);
  const timestamp = parseNumber(transaction.timestamp);

  if (!hash || !sender || !recipient || amount === null || fee === null || nonce === null || timestamp === null) {
    return null;
  }

  return {
    id: parseString(transaction.id, hash.slice(0, 16)),
    hash,
    sender,
    recipient,
    amount,
    fee,
    nonce,
    timestamp,
    status: parseStatus(transaction.status, fallbackStatus),
  };
}

export function emptyNetworkStats(status: LiveMetricStatus = "loading"): LiveNetworkStats {
  return {
    totalBlocks: null,
    validatorCount: null,
    totalWeight: null,
    tps: null,
    provingTimeMs: null,
    activeAppchains: null,
    mempoolSize: null,
    totalTransactions: null,
    status,
    updatedAt: null,
  };
}

export function normalizeNetworkStats(payload: StatsPayload): LiveNetworkStats {
  const data = payload.data ?? {};

  return {
    totalBlocks: parseNumber(data.total_blocks),
    validatorCount: parseNumber(data.validator_count),
    totalWeight: parseNumber(data.total_weight),
    // The current node only reports these when a real measurement exists.
    tps: parseNumber(data.tps),
    provingTimeMs: parseNumber(data.last_proving_time_ms),
    activeAppchains: parseNumber(data.active_appchains),
    mempoolSize: parseNumber(data.mempool_size),
    totalTransactions: parseNumber(data.total_transactions),
    status: payload.success === false ? "unavailable" : "live",
    updatedAt: new Date().toISOString(),
  };
}

export function normalizeTransactions(payload: TransactionsPayload): LatestTransaction[] {
  if (!payload.success || !Array.isArray(payload.transactions)) return [];

  return payload.transactions.flatMap((transaction) => {
    const hash = parseString(transaction.hash);
    if (!hash) return [];

    return [{
      id: parseString(transaction.id, hash.slice(0, 12)),
      hash,
      amount: parseString(transaction.amount, "—"),
      shard: parseNumber(transaction.shard),
    }];
  });
}

export function normalizeAccount(payload: DataPayload<AccountPayload>): AccountSnapshot {
  const account = payload.data;
  if (payload.success === false || !account) throw new Error(apiMessage(payload, "Unable to read the account."));

  const address = parseString(account.address);
  const compatibilityBalance = parseNumber(account.balance);
  const explicitBalance = parseNumber(account.balance_base_units);
  if (compatibilityBalance !== null && explicitBalance !== null && compatibilityBalance !== explicitBalance) {
    throw new Error("The node returned conflicting account balance fields.");
  }
  const balanceBaseUnits = explicitBalance ?? compatibilityBalance;
  const nonce = parseNumber(account.nonce);
  const decimals = parseNumber(account.decimals);
  const updatedAt = parseNumber(account.updated_at);
  if (!address || balanceBaseUnits === null || nonce === null || decimals === null || updatedAt === null) {
    throw new Error("The node returned an incomplete account response.");
  }

  return { address, balance: balanceBaseUnits, balanceBaseUnits, nonce, decimals, updatedAt };
}

export function normalizeFeeEstimate(payload: DataPayload<FeePayload>): FeeEstimate {
  const fee = payload.data;
  if (payload.success === false || !fee) throw new Error(apiMessage(payload, "The network fee is currently unavailable."));

  const recipient = parseString(fee.recipient);
  const amount = parseNumber(fee.amount);
  const networkFee = parseNumber(fee.fee);
  const gasLimit = parseNumber(fee.gas_limit);
  const gasPrice = parseNumber(fee.gas_price);
  const total = parseNumber(fee.total);
  if (!recipient || amount === null || networkFee === null || gasLimit === null || gasPrice === null || total === null) {
    throw new Error("The node returned an incomplete fee estimate.");
  }

  return { recipient, amount, fee: networkFee, gasLimit, gasPrice, total };
}

export function normalizeTransaction(payload: DataPayload<Record<string, unknown>>): TransactionRecord {
  if (payload.success === false || !payload.data) throw new Error(apiMessage(payload, "The node did not return a transaction."));
  const transaction = normalizeTransactionRecord(payload.data, "pending");
  if (!transaction) throw new Error("The node returned an incomplete transaction response.");
  return transaction;
}

export function normalizeAddressTransactions(payload: AddressHistoryPayload): TransactionRecord[] {
  if (!payload.success || !Array.isArray(payload.transactions)) {
    throw new Error(apiMessage(payload, "Unable to read transaction history."));
  }
  return payload.transactions.flatMap((transaction) => {
    const normalized = normalizeTransactionRecord(transaction);
    return normalized ? [normalized] : [];
  });
}

async function fetchJson<T>(path: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(`${API_BASE_URL}${path}`, {
    ...init,
    signal: init.signal,
    credentials: "omit",
    cache: "no-store",
    headers: {
      Accept: "application/json",
      ...(init.body ? { "Content-Type": "application/json" } : {}),
      ...init.headers,
    },
  });

  const payload = (await response.json().catch(() => null)) as T | { message?: unknown } | null;
  if (!response.ok) {
    const message = payload && typeof payload === "object" && "message" in payload
      ? apiMessage(payload as { message?: unknown }, `UltraNet API returned ${response.status}`)
      : `UltraNet API returned ${response.status}`;
    throw new Error(message);
  }
  return payload as T;
}

export async function fetchNetworkStats(signal?: AbortSignal): Promise<LiveNetworkStats> {
  const payload = await fetchJson<StatsPayload>("/api/stats", { signal });
  return normalizeNetworkStats(payload);
}

export async function fetchLatestTransactions(signal?: AbortSignal): Promise<LatestTransaction[]> {
  const payload = await fetchJson<TransactionsPayload>("/api/transactions/latest", { signal });
  return normalizeTransactions(payload);
}

export async function fetchAccount(address: string, signal?: AbortSignal): Promise<AccountSnapshot> {
  const payload = await fetchJson<DataPayload<AccountPayload>>(`/api/account/${encodeURIComponent(address)}`, { signal });
  return normalizeAccount(payload);
}

export async function estimateTransactionFee(
  recipient: string,
  amount: number,
  signal?: AbortSignal,
): Promise<FeeEstimate> {
  const query = new URLSearchParams({ recipient, amount: String(amount) });
  const payload = await fetchJson<DataPayload<FeePayload>>(`/api/transaction/estimate?${query.toString()}`, { signal });
  return normalizeFeeEstimate(payload);
}

export async function submitSignedTransaction(
  transaction: SignedTransaction,
  signal?: AbortSignal,
): Promise<TransactionRecord> {
  const payload = await fetchJson<DataPayload<Record<string, unknown>>>("/api/transaction", {
    method: "POST",
    body: JSON.stringify(transaction),
    signal,
  });
  return normalizeTransaction(payload);
}

export async function fetchTransactionStatus(hash: string, signal?: AbortSignal): Promise<TransactionRecord> {
  const payload = await fetchJson<DataPayload<Record<string, unknown>>>(`/api/transaction/${encodeURIComponent(hash)}`, { signal });
  return normalizeTransaction(payload);
}

export async function fetchAddressTransactions(
  address: string,
  limit = 20,
  signal?: AbortSignal,
): Promise<TransactionRecord[]> {
  const query = new URLSearchParams({ limit: String(limit) });
  const payload = await fetchJson<AddressHistoryPayload>(
    `/api/address/${encodeURIComponent(address)}/transactions?${query.toString()}`,
    { signal },
  );
  return normalizeAddressTransactions(payload);
}
