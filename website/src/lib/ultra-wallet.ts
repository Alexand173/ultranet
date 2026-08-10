import type { AuthLoginPayload } from "@/lib/auth-payload";

export const ULTRA_WALLET_SIGN_VALIDATOR_PROPOSAL =
  "ultranet_signValidatorProposal" as const;
export const ULTRA_WALLET_SIGN_AUTHENTICATION_CHALLENGE =
  "ultranet_signAuthenticationChallenge" as const;
export const ULTRA_WALLET_SIGNING_ENVELOPE_VERSION = 2 as const;
export const ULTRA_WALLET_AUTH_VERSION = 1 as const;

export type JsonByteArray = number[];

export interface SignValidatorProposalParams {
  metadata: string;
  proposalPublicKey: string;
  version: typeof ULTRA_WALLET_SIGNING_ENVELOPE_VERSION;
}

export interface UltraWalletSignValidatorProposalRequest {
  method: typeof ULTRA_WALLET_SIGN_VALIDATOR_PROPOSAL;
  params: SignValidatorProposalParams;
}

export interface SignAuthenticationChallengeParams {
  challengeId: string;
  challenge: string;
  nodeIdentifier: string;
  expiresAt: number;
  version: typeof ULTRA_WALLET_AUTH_VERSION;
}

export interface UltraWalletSignAuthenticationChallengeRequest {
  method: typeof ULTRA_WALLET_SIGN_AUTHENTICATION_CHALLENGE;
  params: SignAuthenticationChallengeParams;
}

export interface SignedAuthenticationChallenge {
  public_key: JsonByteArray;
  signature: JsonByteArray;
}

export interface AuthChallenge {
  challengeId: string;
  challenge: string;
  nodeIdentifier: string;
  expiresAt: number;
  version: typeof ULTRA_WALLET_AUTH_VERSION;
}

export interface AuthSessionPayload {
  node_identifier: string;
  expires_at: number;
}

export interface SignedValidatorProposal {
  sender: string;
  sender_public_key: JsonByteArray;
  proposal_public_key: JsonByteArray;
  nonce: number;
  timestamp: number;
  nullifier: JsonByteArray;
  signature: JsonByteArray;
  version: typeof ULTRA_WALLET_SIGNING_ENVELOPE_VERSION;
}

export interface ValidatorProposalRequest extends SignedValidatorProposal {
  metadata: string;
}

export interface UltraWalletProvider {
  request(
    request:
      | UltraWalletSignValidatorProposalRequest
      | UltraWalletSignAuthenticationChallengeRequest,
  ): Promise<unknown>;
}

export interface UltraWalletError {
  code?: string | number;
  message: string;
  data?: unknown;
}

function isJsonByteArray(value: unknown): value is JsonByteArray {
  return (
    Array.isArray(value) &&
    value.length > 0 &&
    value.every(
      (byte) =>
        typeof byte === "number" &&
        Number.isInteger(byte) &&
        byte >= 0 &&
        byte <= 255,
    )
  );
}

export function isSignedValidatorProposal(
  value: unknown,
): value is SignedValidatorProposal {
  if (!value || typeof value !== "object") return false;

  const proposal = value as Partial<SignedValidatorProposal>;
  return (
    typeof proposal.sender === "string" &&
    proposal.sender.trim().length > 0 &&
    isJsonByteArray(proposal.sender_public_key) &&
    isJsonByteArray(proposal.proposal_public_key) &&
    typeof proposal.nonce === "number" &&
    Number.isSafeInteger(proposal.nonce) &&
    proposal.nonce >= 0 &&
    typeof proposal.timestamp === "number" &&
    Number.isSafeInteger(proposal.timestamp) &&
    proposal.timestamp >= 0 &&
    isJsonByteArray(proposal.nullifier) &&
    proposal.nullifier.length === 32 &&
    isJsonByteArray(proposal.signature) &&
    proposal.version === ULTRA_WALLET_SIGNING_ENVELOPE_VERSION
  );
}

export function isSignedAuthenticationChallenge(
  value: unknown,
): value is SignedAuthenticationChallenge {
  if (!value || typeof value !== "object") return false;
  const signed = value as Partial<SignedAuthenticationChallenge>;
  return isJsonByteArray(signed.public_key) && isJsonByteArray(signed.signature);
}

export function buildAuthenticationLoginRequest(
  challenge: AuthChallenge,
  signed: SignedAuthenticationChallenge,
): AuthLoginPayload {
  return {
    challenge_id: challenge.challengeId,
    challenge: challenge.challenge,
    node_identifier: challenge.nodeIdentifier,
    expires_at: challenge.expiresAt,
    version: challenge.version,
    public_key: signed.public_key,
    signature: signed.signature,
  };
}

export function buildValidatorProposalRequest(
  signedProposal: SignedValidatorProposal,
  metadata: string,
): ValidatorProposalRequest {
  return {
    ...signedProposal,
    metadata: metadata.trim(),
  };
}

export function getUltraWalletProvider(): UltraWalletProvider | undefined {
  if (typeof window === "undefined") return undefined;
  return window.ultraWallet;
}
