import { API_BASE_URL } from "@/lib/links";
import { isUltraAddress } from "@/lib/transaction-format";
import type {
  AppChainAnchorAvailability,
  AppChainOverview,
  AppChainOwnerKind,
  AppChainSource,
  AppChainSummary,
} from "@/lib/appchains.types";

interface RawAppChainView {
  id?: unknown;
  name?: unknown;
  owner?: unknown;
  account_address?: unknown;
  genesis_root?: unknown;
  anchor_fee?: unknown;
  balance?: unknown;
  anchor_spend?: unknown;
  anchor_count?: unknown;
  latest_anchor_at?: unknown;
  latest_state_root?: unknown;
  anchor_availability?: unknown;
  proof_scheme?: unknown;
  source?: unknown;
  registered_tx?: unknown;
  registered_block?: unknown;
}

interface RawOverviewPayload {
  success?: unknown;
  chains?: unknown;
  totals?: {
    anchor_count?: unknown;
    anchor_spend?: unknown;
  };
  anchor_availability?: unknown;
  proof_scheme?: unknown;
  updated_at?: unknown;
  message?: unknown;
}

export class AppChainRequestError extends Error {
  constructor(
    message: string,
    public readonly status: number,
  ) {
    super(message);
    this.name = "AppChainRequestError";
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function readInteger(value: unknown): number | null {
  if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) return value;
  if (typeof value !== "string" || value.trim() === "") return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : null;
}

function readBaseUnits(value: unknown): bigint | null {
  if (typeof value !== "string" || !/^\d+$/.test(value)) return null;
  const normalized = value;
  try {
    return BigInt(normalized);
  } catch {
    return null;
  }
}

function readOwnerKind(owner: string): AppChainOwnerKind {
  if (isUltraAddress(owner)) return "address";
  if (owner.trim()) return "alias";
  return "unknown";
}

function readAnchorAvailability(value: unknown): AppChainAnchorAvailability {
  return value === "test-only" || value === "production" || value === "not-implemented"
    ? value
    : "not-implemented";
}

// A node that predates the ledger view only ever reported registry chains, so that is the honest
// default when the field is absent.
function readSource(value: unknown): AppChainSource {
  return value === "ledger" || value === "ledger+registry" || value === "registry"
    ? value
    : "registry";
}

function normalizeChain(value: unknown): AppChainSummary | null {
  if (!isRecord(value)) return null;
  const chain = value as RawAppChainView;
  const id = readInteger(chain.id);
  const name = readString(chain.name).trim();
  const owner = readString(chain.owner).trim();
  if (id === null || !name || !owner) return null;

  return {
    id,
    name,
    owner,
    ownerKind: readOwnerKind(owner),
    accountAddress: isUltraAddress(readString(chain.account_address).trim())
      ? readString(chain.account_address).trim()
      : null,
    genesisRoot: readString(chain.genesis_root),
    anchorFeeBaseUnits: readBaseUnits(chain.anchor_fee),
    balanceBaseUnits: readBaseUnits(chain.balance),
    anchorSpendBaseUnits: readBaseUnits(chain.anchor_spend),
    anchorCount: readInteger(chain.anchor_count),
    latestAnchorAt: readInteger(chain.latest_anchor_at),
    latestStateRoot: typeof chain.latest_state_root === "string" ? chain.latest_state_root : null,
    anchorAvailability: readAnchorAvailability(chain.anchor_availability),
    proofScheme: readString(chain.proof_scheme, "Unknown server proof scheme"),
    source: readSource(chain.source),
  };
}

function normalizeOverview(payload: RawOverviewPayload): AppChainOverview {
  if (payload.success === false) {
    throw new Error(readString(payload.message, "Unable to read AppChain overview."));
  }
  const chains = Array.isArray(payload.chains)
    ? payload.chains.flatMap((chain) => {
        const normalized = normalizeChain(chain);
        return normalized ? [normalized] : [];
      })
    : [];
  chains.sort((left, right) => left.id - right.id);

  return {
    chains,
    totalAnchorCount: readInteger(payload.totals?.anchor_count),
    totalAnchorSpendBaseUnits: readBaseUnits(payload.totals?.anchor_spend),
    anchorAvailability: readAnchorAvailability(payload.anchor_availability),
    proofScheme: readString(payload.proof_scheme, "Unknown server proof scheme"),
    updatedAt: readInteger(payload.updated_at),
  };
}

function readApiMessage(value: unknown, fallback: string): string {
  return isRecord(value) && typeof value.message === "string" && value.message.trim()
    ? value.message
    : fallback;
}

async function readResponse<T>(response: Response, fallback: string): Promise<T> {
  const payload = (await response.json().catch(() => null)) as T | null;
  if (!response.ok) {
    throw new AppChainRequestError(readApiMessage(payload, fallback), response.status);
  }
  if (!payload || typeof payload !== "object") {
    throw new AppChainRequestError(fallback, response.status);
  }
  return payload;
}

export async function fetchAppChainOverview(signal?: AbortSignal): Promise<AppChainOverview> {
  const response = await fetch(`${API_BASE_URL}/api/appchain/overview`, {
    signal,
    credentials: "omit",
    cache: "no-store",
    headers: { Accept: "application/json" },
  });
  return normalizeOverview(await readResponse<RawOverviewPayload>(response, "Unable to read AppChain overview."));
}

/*
 * There is deliberately no `anchorAppChain` here.
 *
 * Anchoring is an owner-signed version-5 transaction submitted to POST /api/appchain/tx. The
 * dashboard holds no owner key, and the old server-side path
 * (`POST /api/appchain/{id}/anchor`) is retired: the node answers it with 410 Gone and tells the
 * caller to sign instead. Posting it from a button would either fail or, worse, be reported as an
 * anchor that never happened.
 *
 * Reading the registry is all this module does. Anchoring belongs to the UltraNet mobile wallet,
 * which holds the owner's key; `OwnerAnchorNotice` says so in the UI.
 */

export function normalizeAppChainOverview(payload: RawOverviewPayload): AppChainOverview {
  return normalizeOverview(payload);
}

export function getAppChainOwnerKind(owner: string): AppChainOwnerKind {
  return readOwnerKind(owner);
}
