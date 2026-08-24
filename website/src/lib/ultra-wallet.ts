import type { AuthLoginPayload } from "@/lib/auth-payload";

export const ULTRA_WALLET_SIGN_VALIDATOR_PROPOSAL =
  "ultranet_signValidatorProposal" as const;
export const ULTRA_WALLET_SIGN_AUTHENTICATION_CHALLENGE =
  "ultranet_signAuthenticationChallenge" as const;
export const ULTRA_WALLET_SIGN_TRANSACTION = "ultranet_signTransaction" as const;
export const ULTRA_WALLET_SIGNING_ENVELOPE_VERSION = 2 as const;
export const ULTRA_WALLET_TRANSACTION_VERSION = 1 as const;
export const ULTRA_WALLET_AUTH_VERSION = 1 as const;
export const ULTRA_WALLET_PUBLIC_KEY_BYTES = 2_592 as const;
export const ULTRA_WALLET_SIGNATURE_BYTES = 4_627 as const;

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

export interface SignTransactionParams {
  recipient: string;
  amount: number;
  fee: number;
  nonce: number;
  timestamp: number;
  nullifier: JsonByteArray;
  gasLimit: number;
  gasPrice: number;
  chainId: 0;
  version: typeof ULTRA_WALLET_TRANSACTION_VERSION;
}

export interface UltraWalletSignTransactionRequest {
  method: typeof ULTRA_WALLET_SIGN_TRANSACTION;
  params: SignTransactionParams;
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

export interface SignedTransaction {
  sender: string;
  sender_public_key: JsonByteArray;
  recipient: string;
  amount: number;
  fee: number;
  nonce: number;
  timestamp: number;
  nullifier: JsonByteArray;
  gas_limit: number;
  gas_price: number;
  signature: JsonByteArray;
  chain_id: 0;
  version: typeof ULTRA_WALLET_TRANSACTION_VERSION;
}

export interface UltraWalletProvider {
  request(
    request:
      | UltraWalletSignValidatorProposalRequest
      | UltraWalletSignAuthenticationChallengeRequest
      | UltraWalletSignTransactionRequest,
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

function isSafeUint(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function isUltraAddress(value: unknown): value is string {
  return typeof value === "string" && /^[0-9a-f]{64}$/.test(value);
}

export function isSignedValidatorProposal(
  value: unknown,
): value is SignedValidatorProposal {
  if (!value || typeof value !== "object") return false;

  const proposal = value as Partial<SignedValidatorProposal>;
  return (
    isUltraAddress(proposal.sender) &&
    isJsonByteArray(proposal.sender_public_key) &&
    proposal.sender_public_key.length === ULTRA_WALLET_PUBLIC_KEY_BYTES &&
    isJsonByteArray(proposal.proposal_public_key) &&
    isSafeUint(proposal.nonce) &&
    isSafeUint(proposal.timestamp) &&
    isJsonByteArray(proposal.nullifier) &&
    proposal.nullifier.length === 32 &&
    isJsonByteArray(proposal.signature) &&
    proposal.signature.length === ULTRA_WALLET_SIGNATURE_BYTES &&
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

export function isSignedTransaction(value: unknown): value is SignedTransaction {
  if (!value || typeof value !== "object") return false;

  const transaction = value as Partial<SignedTransaction>;
  return (
    isUltraAddress(transaction.sender) &&
    isJsonByteArray(transaction.sender_public_key) &&
    transaction.sender_public_key.length === ULTRA_WALLET_PUBLIC_KEY_BYTES &&
    isUltraAddress(transaction.recipient) &&
    isSafeUint(transaction.amount) &&
    isSafeUint(transaction.fee) &&
    isSafeUint(transaction.nonce) &&
    isSafeUint(transaction.timestamp) &&
    isJsonByteArray(transaction.nullifier) &&
    transaction.nullifier.length === 32 &&
    isSafeUint(transaction.gas_limit) &&
    isSafeUint(transaction.gas_price) &&
    isJsonByteArray(transaction.signature) &&
    transaction.signature.length === ULTRA_WALLET_SIGNATURE_BYTES &&
    transaction.chain_id === 0 &&
    transaction.version === ULTRA_WALLET_TRANSACTION_VERSION
  );
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

export function buildTransactionRequest(
  signedTransaction: SignedTransaction,
): SignedTransaction {
  return {
    ...signedTransaction,
    sender: signedTransaction.sender.trim(),
    recipient: signedTransaction.recipient.trim(),
    sender_public_key: [...signedTransaction.sender_public_key],
    nullifier: [...signedTransaction.nullifier],
    signature: [...signedTransaction.signature],
  };
}

export function getUltraWalletProvider(): UltraWalletProvider | undefined {
  if (typeof window === "undefined") return undefined;
  return window.ultraWallet;
}
