"use client";

import Link from "next/link";
import { ArrowRight, Check, CircleAlert, LockKeyhole, WalletCards } from "lucide-react";
import { useWalletSession } from "@/components/wallet/WalletSessionProvider";
import type { WalletStatusVariant } from "@/components/wallet/wallet.types";
import { getSendUltraHref, VALIDATOR_PROPOSAL_PATH } from "@/lib/links";

interface ProposalWalletPrerequisiteProps {
  variant?: WalletStatusVariant;
  className?: string;
}

export default function ProposalWalletPrerequisite({ variant = "dark", className = "" }: ProposalWalletPrerequisiteProps) {
  const { sessionState, externalWalletStatus } = useWalletSession();
  const dark = variant === "dark";
  const localReady = sessionState === "unlocked";
  const localLocked = sessionState === "locked";
  const externalDetected = !localReady && externalWalletStatus === "detected";
  const Icon = localReady ? Check : localLocked ? LockKeyhole : externalDetected ? WalletCards : CircleAlert;
  const statusText = localReady
    ? "Your personal wallet is ready to sign locally."
    : localLocked
      ? "Your local wallet exists but is locked. Unlock it before submitting."
      : externalDetected
        ? "An UltraWallet provider was detected; it may still ask you to unlock or approve."
        : "A personal signing wallet is required before this proposal can be submitted.";
  const actionText = localReady ? "Wallet ready" : localLocked ? "Unlock wallet" : "Create or unlock wallet";

  return (
    <aside
      aria-labelledby="proposal-wallet-prerequisite-title"
      className={`border p-4 sm:p-5 ${dark ? "border-cyan-glow/25 bg-cyan-glow/[0.04]" : "border-learn-violet/20 bg-learn-soft/70"} ${className}`}
    >
      <div className="flex items-start gap-3">
        <Icon className={`mt-0.5 h-5 w-5 shrink-0 ${dark ? "text-cyan-glow" : "text-learn-violet"}`} aria-hidden="true" />
        <div className="min-w-0">
          <p className={`font-mono text-[10px] uppercase tracking-[0.18em] ${dark ? "text-cyan-glow" : "text-learn-violet"}`}>Before you submit</p>
          <h2 id="proposal-wallet-prerequisite-title" className={`mt-2 font-space-grotesk text-xl font-bold uppercase tracking-tight ${dark ? "text-platinum" : "text-learn-ink"}`}>
            Your personal wallet must sign this proposal
          </h2>
          <p className={`mt-3 text-sm leading-6 ${dark ? "text-platinum/65" : "text-learn-muted"}`}>{statusText}</p>
          <p className={`mt-2 text-xs leading-5 ${dark ? "text-platinum/45" : "text-learn-muted"}`}>
            The node public key identifies the computer. Your wallet signature proves that you authorized it. The website never stores your wallet password or sends it to the node.
          </p>
          {!localReady && (
            <Link
              href={getSendUltraHref(VALIDATOR_PROPOSAL_PATH)}
              className={`mt-4 inline-flex min-h-11 items-center gap-2 font-mono text-[10px] font-bold uppercase tracking-[0.14em] focus:outline-hidden focus:ring-2 focus:ring-offset-2 ${dark ? "text-cyan-glow hover:text-white focus:ring-cyan-glow focus:ring-offset-ink-black" : "text-learn-violet hover:text-learn-ink focus:ring-learn-violet focus:ring-offset-learn-canvas"}`}
            >
              {actionText} <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
            </Link>
          )}
          {localReady && <p className={`mt-4 font-mono text-[10px] uppercase tracking-[0.12em] ${dark ? "text-cyan-glow/80" : "text-learn-violet"}`}>Ready for local signing</p>}
        </div>
      </div>
    </aside>
  );
}
