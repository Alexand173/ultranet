import type { AccountSnapshot, FeeEstimate, TransactionRecord } from "@/lib/api";
import type { StoredWallet } from "@/lib/wallet-crypto";

export type WalletLifecycle = "checking" | "no-wallet" | "setup" | "locked" | "unlocking" | "unlocked" | "error";
export type TransferState = "idle" | "estimating-fee" | "ready" | "reviewing" | "signing" | "submitting" | "pending" | "success" | "failed" | "unknown";
export type CopyStatus = "idle" | "copied" | "error";

export interface TransferDraft {
  recipient: string;
  amountInput: string;
  amountBaseUnits: bigint | null;
  feeEstimate: FeeEstimate | null;
  estimateError: string;
}

export interface TransactionPageModel {
  lifecycle: WalletLifecycle;
  transferState: TransferState;
  storedWallet: StoredWallet | null;
  account: AccountSnapshot | null;
  history: TransactionRecord[];
  draft: TransferDraft;
  result: TransactionRecord | null;
  error: string;
}
