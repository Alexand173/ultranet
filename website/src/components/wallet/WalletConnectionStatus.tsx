"use client";

import { Check, CircleAlert, LoaderCircle, LockKeyhole, WalletCards } from "lucide-react";
import { useWalletSession } from "@/components/wallet/WalletSessionProvider";
import type { WalletStatusVariant } from "@/components/wallet/wallet.types";

interface WalletConnectionStatusProps {
  variant?: WalletStatusVariant;
  compact?: boolean;
  className?: string;
}

export default function WalletConnectionStatus({
  variant = "dark",
  compact = false,
  className = "",
}: WalletConnectionStatusProps) {
  const { sessionState, externalWalletStatus } = useWalletSession();

  const status = sessionState === "checking"
    ? { label: "Checking wallet", detail: "Reading local wallet state", tone: "checking" as const }
    : sessionState === "unlocked"
      ? { label: "Ready to sign locally", detail: "Local wallet unlocked in this browser", tone: "ready" as const }
      : sessionState === "locked"
        ? { label: "Wallet locked", detail: "Open Send Ultra to unlock before signing", tone: "locked" as const }
        : externalWalletStatus === "detected"
          ? { label: "UltraWallet detected", detail: "The provider may still require unlock", tone: "detected" as const }
          : externalWalletStatus === "checking"
            ? { label: "Checking UltraWallet", detail: "Looking for a browser provider", tone: "checking" as const }
            : { label: "UltraWallet not detected", detail: "Open or install a wallet before submitting", tone: "missing" as const };

  const Icon = status.tone === "ready"
    ? Check
    : status.tone === "locked"
      ? LockKeyhole
      : status.tone === "missing"
        ? CircleAlert
        : status.tone === "checking"
          ? LoaderCircle
          : WalletCards;

  const dark = variant === "dark";
  const toneClass = status.tone === "ready" || status.tone === "detected"
    ? dark ? "text-cyan-glow" : "text-learn-violet"
    : status.tone === "missing" || status.tone === "locked"
      ? dark ? "text-amber-200" : "text-amber-700"
      : dark ? "text-platinum/55" : "text-learn-muted";

  return (
    <div
      className={`inline-flex min-w-0 items-center gap-2 font-mono uppercase tracking-[0.12em] ${compact ? "text-[9px]" : "border px-3 py-2 text-[10px]"} ${dark ? "border-platinum/15 bg-ink-black/40" : "border-learn-ink/10 bg-white/75"} ${toneClass} ${className}`}
      role="status"
      aria-live="polite"
      title={status.detail}
    >
      <Icon className={`h-3.5 w-3.5 shrink-0 ${status.tone === "checking" ? "animate-spin" : ""}`} aria-hidden="true" />
      <span className="truncate">{status.label}</span>
    </div>
  );
}
