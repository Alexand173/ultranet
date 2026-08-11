import { API_BASE_URL } from "@/lib/links";

export type LiveMetricStatus = "loading" | "live" | "unavailable";

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

function parseNumber(value: unknown): number | null {
  if (typeof value === "number" && Number.isFinite(value)) return value;
  if (typeof value !== "string" || value.trim() === "") return null;

  const parsed = Number(value);
  return Number.isFinite(parsed) ? parsed : null;
}

function parseString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
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

async function fetchJson<T>(path: string, signal?: AbortSignal): Promise<T> {
  const response = await fetch(`${API_BASE_URL}${path}`, {
    signal,
    cache: "no-store",
    headers: { Accept: "application/json" },
  });

  if (!response.ok) {
    throw new Error(`UltraNet API returned ${response.status}`);
  }

  return response.json() as Promise<T>;
}

export async function fetchNetworkStats(signal?: AbortSignal): Promise<LiveNetworkStats> {
  const payload = await fetchJson<StatsPayload>("/api/stats", signal);
  return normalizeNetworkStats(payload);
}

export async function fetchLatestTransactions(signal?: AbortSignal): Promise<LatestTransaction[]> {
  const payload = await fetchJson<TransactionsPayload>("/api/transactions/latest", signal);
  return normalizeTransactions(payload);
}
