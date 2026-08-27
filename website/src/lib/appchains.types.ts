export type AppChainsLoadState = "loading" | "ready" | "empty" | "unavailable";
export type AppChainActionState = "idle" | "confirming" | "submitting" | "success" | "rejected" | "unavailable";
export type AppChainAnchorAvailability = "not-implemented" | "test-only" | "production";
export type AppChainOwnerKind = "address" | "alias" | "unknown";

export interface AppChainSummary {
  id: number;
  name: string;
  owner: string;
  ownerKind: AppChainOwnerKind;
  accountAddress: string | null;
  genesisRoot: string;
  anchorFeeBaseUnits: bigint | null;
  balanceBaseUnits: bigint | null;
  anchorSpendBaseUnits: bigint | null;
  anchorCount: number | null;
  latestAnchorAt: number | null;
  latestStateRoot: string | null;
  anchorAvailability: AppChainAnchorAvailability;
  proofScheme: string;
}

export interface AppChainOverview {
  chains: AppChainSummary[];
  totalAnchorCount: number | null;
  totalAnchorSpendBaseUnits: bigint | null;
  anchorAvailability: AppChainAnchorAvailability;
  proofScheme: string;
  updatedAt: number | null;
}

export interface CreateAppChainInput {
  name: string;
  owner: string;
}

export interface CreateAppChainResult {
  chainId: number;
  message: string;
  chain: AppChainSummary | null;
}

export interface AnchorAppChainResult {
  chainId: number;
  anchorNumber: number | null;
  stateRoot: string | null;
  timestamp: number | null;
  anchorCount: number | null;
  chargedBaseUnits: bigint | null;
  balanceBaseUnits: bigint | null;
  accountAddress: string | null;
  proofScheme: string;
  isTest: boolean;
  message: string;
}

export interface AppChainAction {
  state: AppChainActionState;
  error: string;
  result: AnchorAppChainResult | null;
}
