import { authenticatedFetch } from "@/lib/auth";
import { API_BASE_URL } from "@/lib/links";

export type GovernanceCapability = "validator_review" | "sovereign_approve";

export type ApprovalStage =
  | "created"
  | "signing"
  | "awaiting_second_owner"
  | "finalizing"
  | "approved"
  | "activated"
  | "expired"
  | "rejected";

export type ApprovalErrorCode =
  | "UNAUTHORIZED"
  | "FORBIDDEN"
  | "CSRF_REQUIRED"
  | "PROPOSAL_NOT_PENDING"
  | "HASH_MISMATCH"
  | "NONCE_BUSY"
  | "NONCE_STALE"
  | "TIMESTAMP_EXPIRED"
  | "NULLIFIER_REPLAY"
  | "DUPLICATE_OWNER"
  | "INVALID_SIGNATURE"
  | "SIGNER_UNAVAILABLE"
  | "INTENT_NOT_FOUND"
  | "INTENT_EXPIRED"
  | "ALREADY_APPROVED"
  | "FINALIZATION_REJECTED"
  | "STORAGE_ERROR"
  | "CONTRACT_ERROR";

export interface GovernanceCapabilitySet {
  capabilities: GovernanceCapability[];
}

export interface PendingValidatorProposal {
  proposalHash: string;
  publicKey: string;
  metadata: string;
  proposer: string;
  submittedAt: number;
  intentId: string | null;
  signedOwnerCount: number;
  currentOwnerSigned: boolean;
  threshold: 2;
  totalOwners: 3;
}

export interface ValidatorReviewResponse {
  proposals: PendingValidatorProposal[];
  capabilities: GovernanceCapabilitySet;
}

export interface ApprovalIntentStatus {
  intentId: string;
  proposalHash: string;
  stage: ApprovalStage;
  signedOwnerCount: number;
  threshold: 2;
  expiresAt: number;
  activated: boolean;
  message: string;
  code?: ApprovalErrorCode;
}

export class GovernanceRequestError extends Error {
  constructor(
    message: string,
    public readonly status: number,
    public readonly code?: ApprovalErrorCode,
  ) {
    super(message);
    this.name = "GovernanceRequestError";
  }
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function readString(value: unknown, fallback = ""): string {
  return typeof value === "string" ? value : fallback;
}

function readBoolean(value: unknown, fallback = false): boolean {
  return typeof value === "boolean" ? value : fallback;
}

function readInteger(value: unknown): number | null {
  if (typeof value === "number" && Number.isSafeInteger(value) && value >= 0) return value;
  if (typeof value !== "string" || !/^\d+$/.test(value)) return null;
  const parsed = Number(value);
  return Number.isSafeInteger(parsed) && parsed >= 0 ? parsed : null;
}

function readHash(value: unknown, fieldName: string): string {
  const hash = readString(value).trim().replace(/^0x/i, "").toLowerCase();
  if (!/^[0-9a-f]{64}$/.test(hash)) throw new GovernanceRequestError(`${fieldName} must be 64 hexadecimal characters.`, 502, "CONTRACT_ERROR");
  return hash;
}

function readPublicKey(value: unknown): string {
  const publicKey = readString(value).trim().replace(/^0x/i, "").toLowerCase();
  if (!/^[0-9a-f]{5184}$/.test(publicKey)) throw new GovernanceRequestError("The node returned an invalid validator public key.", 502, "CONTRACT_ERROR");
  return publicKey;
}

function readStage(value: unknown): ApprovalStage {
  if (
    value === "created" ||
    value === "signing" ||
    value === "awaiting_second_owner" ||
    value === "finalizing" ||
    value === "approved" ||
    value === "activated" ||
    value === "expired" ||
    value === "rejected"
  ) return value;
  throw new GovernanceRequestError("The node returned an unknown approval stage.", 502, "CONTRACT_ERROR");
}

function readCapabilities(value: unknown): GovernanceCapabilitySet {
  if (!isRecord(value) || !Array.isArray(value.capabilities)) {
    throw new GovernanceRequestError("The node returned an incomplete capability response.", 502, "CONTRACT_ERROR");
  }
  const capabilities = value.capabilities.filter(
    (capability): capability is GovernanceCapability => capability === "validator_review" || capability === "sovereign_approve",
  );
  return { capabilities };
}

function normalizeProposal(value: unknown): PendingValidatorProposal {
  if (!isRecord(value)) throw new GovernanceRequestError("The node returned an invalid proposal record.", 502, "CONTRACT_ERROR");
  const submittedAt = readInteger(value.timestamp);
  const signedOwnerCount = readInteger(value.signed_owner_count);
  const threshold = readInteger(value.threshold);
  const totalOwners = readInteger(value.total_owners);
  if (submittedAt === null || signedOwnerCount === null || threshold !== 2 || totalOwners !== 3 || signedOwnerCount > totalOwners) {
    throw new GovernanceRequestError("The node returned an incomplete proposal approval record.", 502, "CONTRACT_ERROR");
  }
  const intentId = value.intent_id === null || value.intent_id === undefined ? null : readString(value.intent_id).trim();
  if (intentId === "") throw new GovernanceRequestError("The node returned an invalid approval intent identifier.", 502, "CONTRACT_ERROR");
  return {
    proposalHash: readHash(value.proposal_hash, "proposal_hash"),
    publicKey: readPublicKey(value.public_key),
    metadata: readString(value.metadata, "Unnamed validator"),
    proposer: readHash(value.proposer, "proposer"),
    submittedAt,
    intentId,
    signedOwnerCount,
    currentOwnerSigned: readBoolean(value.current_owner_signed),
    threshold: 2,
    totalOwners: 3,
  };
}

function readErrorCode(value: unknown): ApprovalErrorCode | undefined {
  return typeof value === "string" && /^[A-Z_]+$/.test(value) ? value as ApprovalErrorCode : undefined;
}

function apiMessage(payload: unknown, fallback: string): string {
  return isRecord(payload) && typeof payload.message === "string" && payload.message.trim() ? payload.message : fallback;
}

async function readResponse<T>(response: Response, fallback: string): Promise<T> {
  const payload = await response.json().catch(() => null) as unknown;
  if (!response.ok) {
    throw new GovernanceRequestError(
      apiMessage(payload, fallback),
      response.status,
      isRecord(payload) ? readErrorCode(payload.code) : undefined,
    );
  }
  return payload as T;
}

export async function fetchValidatorReview(signal?: AbortSignal): Promise<ValidatorReviewResponse> {
  const response = await fetch(`${API_BASE_URL}/api/governance/validator-review`, {
    signal,
    credentials: "include",
    cache: "no-store",
    headers: { Accept: "application/json" },
  });
  const payload = await readResponse<Record<string, unknown>>(response, "Unable to load pending validator approvals.");
  if (!isRecord(payload) || payload.success !== true || !Array.isArray(payload.proposals)) {
    throw new GovernanceRequestError("The node returned an incomplete validator review response.", 502, "CONTRACT_ERROR");
  }
  return {
    proposals: payload.proposals.map(normalizeProposal),
    capabilities: readCapabilities(payload.capabilities),
  };
}

function normalizeIntentStatus(value: unknown, fallbackMessage: string): ApprovalIntentStatus {
  if (!isRecord(value)) throw new GovernanceRequestError("The node returned an incomplete approval status.", 502, "CONTRACT_ERROR");
  const signedOwnerCount = readInteger(value.signed_owner_count);
  const threshold = readInteger(value.threshold);
  const expiresAt = readInteger(value.expires_at);
  if (signedOwnerCount === null || signedOwnerCount > 3 || threshold !== 2 || expiresAt === null) {
    throw new GovernanceRequestError("The node returned an invalid approval status.", 502, "CONTRACT_ERROR");
  }
  const intentId = readString(value.intent_id).trim();
  if (!intentId) throw new GovernanceRequestError("The node returned an invalid approval intent identifier.", 502, "CONTRACT_ERROR");
  const code = readErrorCode(value.code);
  return {
    intentId,
    proposalHash: readHash(value.proposal_hash, "proposal_hash"),
    stage: readStage(value.stage),
    signedOwnerCount,
    threshold: 2,
    expiresAt,
    activated: readBoolean(value.activated),
    message: readString(value.message, fallbackMessage),
    ...(code ? { code } : {}),
  };
}

async function readIntentResponse(response: Response, fallback: string): Promise<ApprovalIntentStatus> {
  const payload = await readResponse<Record<string, unknown>>(response, fallback);
  if (!isRecord(payload) || payload.success !== true || !payload.data) {
    throw new GovernanceRequestError(apiMessage(payload, fallback), response.status, isRecord(payload) ? readErrorCode(payload.code) : undefined);
  }
  return normalizeIntentStatus(payload.data, fallback);
}

export async function createApprovalIntent(
  proposalHash: string,
  confirmedProposalHash: string,
): Promise<ApprovalIntentStatus> {
  let response: Response;
  try {
    response = await authenticatedFetch("/api/governance/approval-intents", {
      method: "POST",
      body: JSON.stringify({ proposal_hash: proposalHash, confirmed_proposal_hash: confirmedProposalHash }),
    });
  } catch {
    throw new GovernanceRequestError("Unable to reach the UltraNet approval service.", 0);
  }
  return readIntentResponse(response, "Unable to create the approval intent.");
}

export async function approveApprovalIntent(intentId: string): Promise<ApprovalIntentStatus> {
  let response: Response;
  try {
    response = await authenticatedFetch(`/api/governance/approval-intents/${encodeURIComponent(intentId)}/approve`, {
      method: "POST",
      body: JSON.stringify({}),
    });
  } catch {
    throw new GovernanceRequestError("Unable to reach the isolated Sovereign signer.", 0);
  }
  return readIntentResponse(response, "Unable to request Sovereign approval.");
}

export async function fetchApprovalIntentStatus(intentId: string): Promise<ApprovalIntentStatus> {
  let response: Response;
  try {
    response = await authenticatedFetch(`/api/governance/approval-intents/${encodeURIComponent(intentId)}`);
  } catch {
    throw new GovernanceRequestError("Unable to reach the UltraNet approval service.", 0);
  }
  return readIntentResponse(response, "Unable to read the approval status.");
}

export function formatProposalHash(hash: string): string {
  return hash;
}

export function formatHashFingerprint(hash: string): string {
  return hash.length <= 24 ? hash : `${hash.slice(0, 12)}…${hash.slice(-12)}`;
}

export function formatPublicKeyFingerprint(publicKey: string): string {
  return publicKey.length <= 20 ? publicKey : `${publicKey.slice(0, 10)}…${publicKey.slice(-10)}`;
}

export function formatSubmittedAt(timestamp: number): string {
  return new Intl.DateTimeFormat(undefined, { dateStyle: "medium", timeStyle: "short" }).format(new Date(timestamp * 1000));
}

export function formatApprovalProgress(signedOwnerCount: number, threshold = 2): string {
  return `${Math.min(signedOwnerCount, threshold)}/${threshold}`;
}
