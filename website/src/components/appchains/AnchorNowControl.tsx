"use client";

import Link from "next/link";
import { AlertTriangle, Anchor, ArrowUpRight, CheckCircle2, LoaderCircle, ShieldAlert, X } from "lucide-react";
import { formatUltraAmount } from "@/lib/transaction-format";
import { getSendUltraHref } from "@/lib/links";
import type { AppChainAction } from "@/lib/appchains.types";

interface AnchorNowControlProps {
  chainId: number;
  availability: "not-implemented" | "test-only" | "production";
  anchorFeeBaseUnits: bigint | null;
  balanceBaseUnits: bigint | null;
  controlId: string;
  action: AppChainAction;
  onArm: () => void;
  onCancel: () => void;
  onConfirm: () => void;
}

export default function AnchorNowControl({
  chainId,
  availability,
  anchorFeeBaseUnits,
  balanceBaseUnits,
  controlId,
  action,
  onArm,
  onCancel,
  onConfirm,
}: AnchorNowControlProps) {
  const isSubmitting = action.state === "submitting";
  const hasError = action.state === "rejected" || action.state === "unavailable";
  const isProduction = availability === "production";
  const isAvailable = isProduction || availability === "test-only";
  const actionLabel = isProduction ? "ANCHOR NOW" : "ANCHOR NOW (TESTING)";
  const confirmationLabel = isProduction ? "CONFIRM_ANCHOR" : "CONFIRM_TEST_ANCHOR";
  const confirmationCopy = isProduction
    ? "Snapshot the current server state, verify its proof, and charge the treasury fee?"
    : "Generate a server-side development fixture and charge the test balance?";
  const hasTreasuryData = balanceBaseUnits !== null && anchorFeeBaseUnits !== null;
  const hasFunds = hasTreasuryData && balanceBaseUnits >= anchorFeeBaseUnits;

  if (!isAvailable) {
    return (
      <div className="min-w-44" title={`Server anchoring for AppChain #${chainId} is disabled by the connected node.`}>
        <span className="inline-flex min-h-10 w-full items-center justify-center gap-2 border border-platinum/10 px-3 text-center font-mono text-[10px] font-bold uppercase tracking-[0.12em] text-platinum/30">
          <Anchor className="h-3.5 w-3.5 shrink-0" aria-hidden="true" />
          ANCHORING_DISABLED
        </span>
      </div>
    );
  }

  return (
    <div className="min-w-44">
      {action.state === "confirming" && (
        <div className="border border-amber-200/25 bg-amber-200/[0.06] p-3" role="group" aria-labelledby={`anchor-confirm-${controlId}`}>
          <p id={`anchor-confirm-${controlId}`} className="font-mono text-[10px] font-bold uppercase tracking-[0.12em] text-amber-100">
            {confirmationLabel}
          </p>
          <p className="mt-2 text-xs leading-5 text-amber-100/70">{confirmationCopy}</p>
          <p className="mt-2 font-mono text-[10px] leading-5 text-amber-100/50">Fee: {anchorFeeBaseUnits === null ? "Unavailable" : `${formatUltraAmount(anchorFeeBaseUnits)} $ULTRA`}. Charged only after server verification.</p>
          <div className="mt-3 grid gap-2 sm:grid-cols-2">
            <button
              type="button"
              onClick={onConfirm}
              className="inline-flex min-h-10 items-center justify-center gap-2 bg-cyan-glow px-3 font-mono text-[10px] font-black uppercase tracking-[0.1em] text-ink-black transition-colors hover:bg-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"
            >
              <Anchor className="h-3.5 w-3.5" aria-hidden="true" />
              CONFIRM
            </button>
            <button
              type="button"
              onClick={onCancel}
              className="inline-flex min-h-10 items-center justify-center gap-2 border border-platinum/15 px-3 font-mono text-[10px] font-bold uppercase tracking-[0.1em] text-platinum/60 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"
            >
              <X className="h-3.5 w-3.5" aria-hidden="true" />
              CANCEL
            </button>
          </div>
        </div>
      )}

      {action.state === "success" && action.result && (
        <div className="border border-emerald-300/25 bg-emerald-300/[0.06] p-3" role="status" aria-live="polite">
          <div className="flex items-start gap-2 text-emerald-200">
            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
            <div className="min-w-0">
              <p className="font-mono text-[10px] font-bold uppercase tracking-[0.12em]">{isProduction ? "ANCHOR_COMPLETE" : "TEST_ANCHOR_COMPLETE"}</p>
              <p className="mt-1 text-xs leading-5 text-emerald-100/70">
                Anchor {action.result.anchorNumber ?? "—"} recorded. {action.result.chargedBaseUnits === null ? "Fee unavailable." : isProduction ? "Treasury fee charged." : "Test fee charged."}
              </p>
            </div>
          </div>
        </div>
      )}

      {hasError && action.error && (
        <div className="border border-red-300/30 bg-red-300/10 p-3" role="alert" aria-live="assertive">
          <div className="flex items-start gap-2 text-red-200">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
            <div className="min-w-0">
              <p className="font-mono text-[10px] font-bold uppercase tracking-[0.12em]">ANCHOR_REQUEST_REJECTED</p>
              <p className="mt-1 break-words text-xs leading-5 text-red-100/75">{action.error}</p>
            </div>
          </div>
        </div>
      )}

      {action.state !== "confirming" && (
        <>
          {!hasFunds && hasTreasuryData && <p className="mt-2 border border-amber-200/20 bg-amber-200/[0.04] px-3 py-2 font-mono text-[10px] leading-5 text-amber-100/70">Fund treasury first. Available: {formatUltraAmount(balanceBaseUnits)} $ULTRA; required: {formatUltraAmount(anchorFeeBaseUnits)} $ULTRA.</p>}
          {!hasTreasuryData && <p className="mt-2 border border-platinum/15 bg-platinum/[0.03] px-3 py-2 font-mono text-[10px] leading-5 text-platinum/50">Treasury balance or anchor fee unavailable. Refresh the registry before anchoring.</p>}
          {hasFunds ? (
            <button
              type="button"
              onClick={onArm}
              disabled={isSubmitting}
              aria-busy={isSubmitting}
              className="mt-2 inline-flex min-h-10 w-full items-center justify-center gap-2 bg-cyan-glow px-3 font-mono text-[10px] font-black uppercase tracking-[0.12em] text-ink-black transition-colors hover:bg-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-not-allowed disabled:opacity-45"
            >
              {isSubmitting ? <LoaderCircle className="h-3.5 w-3.5 animate-spin" aria-hidden="true" /> : <ShieldAlert className="h-3.5 w-3.5" aria-hidden="true" />}
              {isSubmitting ? "ANCHORING…" : action.state === "success" ? "ANCHOR AGAIN" : actionLabel}
            </button>
          ) : hasTreasuryData ? (
            <Link
              href={getSendUltraHref("/appchains")}
              className="mt-2 inline-flex min-h-10 w-full items-center justify-center gap-2 border border-amber-200/35 bg-amber-200/[0.08] px-3 text-center font-mono text-[10px] font-black uppercase tracking-[0.12em] text-amber-100 transition-colors hover:border-amber-100 hover:bg-amber-100 hover:text-ink-black focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-amber-200"
              title="Open Send Ultra to fund this AppChain treasury"
            >
              <ArrowUpRight className="h-3.5 w-3.5" aria-hidden="true" />
              FUND TREASURY FIRST
            </Link>
          ) : (
            <button
              type="button"
              disabled
              aria-disabled="true"
              title="Treasury accounting is unavailable from the connected node."
              className="mt-2 inline-flex min-h-10 w-full items-center justify-center gap-2 border border-platinum/10 px-3 text-center font-mono text-[10px] font-black uppercase tracking-[0.12em] text-platinum/30 disabled:cursor-not-allowed"
            >
              <ShieldAlert className="h-3.5 w-3.5" aria-hidden="true" />
              ANCHORING_UNAVAILABLE
            </button>
          )}
        </>
      )}
    </div>
  );
}
