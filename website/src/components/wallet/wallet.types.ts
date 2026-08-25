import type { LocalWalletKeyMaterial, StoredWallet } from "@/lib/wallet-crypto";
import type { UltraWalletProvider } from "@/lib/ultra-wallet";

export type WalletSessionState = "checking" | "none" | "locked" | "unlocked";
export type ExternalWalletStatus = "checking" | "detected" | "not-detected";
export type WalletStatusVariant = "dark" | "light";

export interface WalletSessionContextValue {
  sessionState: WalletSessionState;
  externalWalletStatus: ExternalWalletStatus;
  walletAddress: string | null;
  storedWallet: StoredWallet | null;
  storageError: string | null;
  localProvider: UltraWalletProvider | null;
  unlock(material: LocalWalletKeyMaterial, wallet?: StoredWallet): void;
  lock(): void;
  beginSigning(): void;
  endSigning(): void;
}
