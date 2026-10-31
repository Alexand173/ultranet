import { SEND_ULTRA_PATH } from "@/lib/links";

export type WalletSetupMode = "create" | "restore";
export type WalletLockedInitialAction = "locked" | "restore" | "replace-confirm";
export type SupportedPathId = "wallet" | "appchains" | "validator";
export type SupportedPathIcon = "wallet" | "layers" | "validator";

export interface SupportedPath {
  id: SupportedPathId;
  title: string;
  icon: SupportedPathIcon;
  accessLabel: string;
  facts: readonly string[];
  href: string;
  actionLabel: string;
  secondaryActions?: readonly {
    label: string;
    href: string;
  }[];
}

export interface ConceptGroup {
  id: string;
  title: string;
  items: readonly string[];
}

export const supportedPaths: readonly SupportedPath[] = [
  {
    id: "wallet",
    title: "Wallet",
    icon: "wallet",
    accessLabel: "Local wallet",
    facts: [
      "Review the transfer before signing locally.",
      "Receive by copying your address; there is no QR flow.",
    ],
    href: SEND_ULTRA_PATH,
    actionLabel: "Open wallet",
    secondaryActions: [
      { label: "Create wallet", href: getWalletSetupHref("create") },
      { label: "Restore wallet", href: getWalletSetupHref("restore") },
    ],
  },
  {
    id: "appchains",
    title: "AppChains",
    icon: "layers",
    accessLabel: "Operator sign-in for the website console",
    facts: [
      "Register an AppChain record and request a state-commitment anchor.",
      "Public read API endpoints exist.",
      "Registration is free.",
      "An anchor fee is recorded against a funded treasury.",
    ],
    href: "/login?next=%2Fappchains",
    actionLabel: "Sign in to AppChains",
  },
  {
    id: "validator",
    title: "Validator applications",
    icon: "validator",
    accessLabel: "Wallet signature + owner approval",
    facts: [
      "Activation requires approval from two distinct Sovereign owners.",
      "$ULTRA holder voting is not available.",
    ],
    href: "/validator",
    actionLabel: "View validator onboarding",
  },
] as const;

export const conceptGroups: readonly ConceptGroup[] = [
  {
    id: "payments-addresses",
    title: "Payments & addresses",
    items: ["UltraNet Pay", "Name Service"],
  },
  {
    id: "move-acquire-assets",
    title: "Move & acquire assets",
    items: [
      "DEX",
      "Bridge",
      "Fiat on-ramp",
      "Exchange / listing access",
      "BTC / other-crypto purchases",
    ],
  },
  {
    id: "participate-own",
    title: "Participate & own",
    items: ["Staking", "Holder-led DAO", "AppChain ownership trading"],
  },
  {
    id: "applications",
    title: "Applications",
    items: ["Games", "NFT marketplace"],
  },
] as const;

export function parseWalletSetupMode(
  value: string | string[] | undefined,
): WalletSetupMode | undefined {
  if (value === "create" || value === "restore") return value;
  return undefined;
}

export function getWalletSetupHref(mode: WalletSetupMode): string {
  return `${SEND_ULTRA_PATH}?setup=${mode}`;
}

export function getWalletLockedInitialAction(
  mode: WalletSetupMode | undefined,
): WalletLockedInitialAction {
  if (mode === "restore") return "restore";
  if (mode === "create") return "replace-confirm";
  return "locked";
}
