export type AppChainsLoadState = "loading" | "ready" | "empty" | "unavailable";
/**
 * What the node says about *server-side* anchoring. Server anchoring is retired, so the value is now
 * `not-implemented` everywhere; `test-only` and `production` remain so an older node's answer is read
 * rather than misreported, and so a value the dashboard does not understand fails closed.
 */
export type AppChainAnchorAvailability = "not-implemented" | "test-only" | "production";
export type AppChainOwnerKind = "address" | "alias" | "unknown";
/**
 * Which store on the node knows this chain. `ledger` means its owner signed a version-5 L1
 * transaction: it has no treasury of its own and the node holds no key to anchor it.
 */
export type AppChainSource = "ledger" | "registry" | "ledger+registry";

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
  source: AppChainSource;
}

export interface AppChainOverview {
  chains: AppChainSummary[];
  totalAnchorCount: number | null;
  totalAnchorSpendBaseUnits: bigint | null;
  anchorAvailability: AppChainAnchorAvailability;
  proofScheme: string;
  updatedAt: number | null;
}

/* No anchor-result or anchor-action types: this dashboard cannot anchor, so it has no such state. */
