import { authenticatedFetch } from "@/lib/auth";
import { API_BASE_URL } from "@/lib/links";
import { isUltraAddress } from "@/lib/transaction-format";
import type {
  AnchorAppChainResult,
  AppChainAnchorAvailability,
  AppChainOverview,
  AppChainOwnerKind,
  AppChainSummary,
  CreateAppChainInput,
  CreateAppChainResult,
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

interface RawCreatePayload {
  success?: unknown;
  message?: unknown;
  chain_id?: unknown;
  chain?: unknown;
}

interface RawAnchorPayload {
  success?: unknown;
  message?: unknown;
  chain_id?: unknown;
  anchor_number?: unknown;
  state_root?: unknown;
  timestamp?: unknown;
  anchor_count?: unknown;
  charged_base_units?: unknown;
  balance?: unknown;
  account_address?: unknown;
  proof_scheme?: unknown;
  is_test?: unknown;
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

export async function createAppChain(input: CreateAppChainInput): Promise<CreateAppChainResult> {
  let response: Response;
  try {
    response = await authenticatedFetch("/api/appchain/create", {
      method: "POST",
      body: JSON.stringify({ name: input.name.trim(), owner: input.owner.trim() }),
      headers: { "Content-Type": "application/json" },
    });
  } catch {
    throw new AppChainRequestError("Unable to reach the UltraNet node.", 0);
  }

  const payload = await readResponse<RawCreatePayload>(response, "Unable to create the AppChain.");
  const chainId = readInteger(payload.chain_id);
  if (payload.success === false || chainId === null) {
    throw new AppChainRequestError(readString(payload.message, "The node rejected AppChain creation."), response.status);
  }

  return {
    chainId,
    message: readString(payload.message, "AppChain created successfully."),
    chain: normalizeChain(payload.chain),
  };
}

async function submitAnchor(path: string, rejectionMessage: string): Promise<AnchorAppChainResult> {
  let response: Response;
  try {
    response = await authenticatedFetch(path, { method: "POST" });
  } catch {
    throw new AppChainRequestError("Unable to reach the UltraNet node.", 0);
  }

  const payload = await readResponse<RawAnchorPayload>(response, "Unable to anchor the AppChain.");
  if (payload.success === false || readInteger(payload.chain_id) === null) {
    throw new AppChainRequestError(readString(payload.message, rejectionMessage), response.status);
  }

  return {
    chainId: readInteger(payload.chain_id) as number,
    anchorNumber: readInteger(payload.anchor_number),
    stateRoot: typeof payload.state_root === "string" ? payload.state_root : null,
    timestamp: readInteger(payload.timestamp),
    anchorCount: readInteger(payload.anchor_count),
    chargedBaseUnits: readBaseUnits(payload.charged_base_units),
    balanceBaseUnits: readBaseUnits(payload.balance),
    accountAddress: typeof payload.account_address === "string" ? payload.account_address : null,
    proofScheme: readString(payload.proof_scheme, "Unknown server proof scheme"),
    isTest: payload.is_test === true,
    message: readString(payload.message, "AppChain anchor completed."),
  };
}

export function anchorAppChain(chainId: number): Promise<AnchorAppChainResult> {
  return submitAnchor(`/api/appchain/${encodeURIComponent(chainId)}/anchor`, "The node rejected the AppChain anchor.");
}

export function anchorAppChainForTest(chainId: number): Promise<AnchorAppChainResult> {
  return submitAnchor(`/api/appchain/${encodeURIComponent(chainId)}/anchor/test`, "The node rejected the test anchor.");
}

export function normalizeAppChainOverview(payload: RawOverviewPayload): AppChainOverview {
  return normalizeOverview(payload);
}

export function getAppChainOwnerKind(owner: string): AppChainOwnerKind {
  return readOwnerKind(owner);
}
