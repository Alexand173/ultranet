"use client";

import { AlertTriangle, CheckCircle2, ShieldAlert, X } from "lucide-react";
import type { MineBlockResult } from "@/lib/operator";

export type MineState = "idle" | "armed" | "submitting" | "success" | "rejected" | "unavailable";

interface MineControlProps {
  state: MineState;
  result: MineBlockResult | null;
  error: string;
  onArm: () => void;
  onCancel: () => void;
  onConfirm: () => void;
}

export default function MineControl({ state, result, error, onArm, onCancel, onConfirm }: MineControlProps) {
  const isSubmitting = state === "submitting";
  const hasError = state === "rejected" || state === "unavailable";

  return (
    <section aria-labelledby="manual-mine-heading" className="neon-inset p-5 sm:p-6">
      <div className="flex items-start gap-3">
        <div className="flex h-10 w-10 shrink-0 items-center justify-center border border-amber-300/30 bg-amber-300/10 text-amber-200">
          <ShieldAlert className="h-5 w-5" aria-hidden="true" />
        </div>
        <div>
          <p className="font-mono text-[10px] uppercase tracking-[0.24em] text-amber-200/75">State_Changing_Operation</p>
          <h2 id="manual-mine-heading" className="mt-2 font-space-grotesk text-2xl font-bold uppercase tracking-tight text-platinum">
            Manual Mine
          </h2>
        </div>
      </div>

      <p className="mt-5 text-sm leading-relaxed text-platinum/55">
        Mining changes chain state and is available only inside this authenticated operator surface. The node may reject the request when no valid transactions are available.
      </p>

      {state === "armed" && (
        <div className="mt-5 border border-amber-300/30 bg-amber-300/10 p-4" role="group" aria-labelledby="mine-confirmation-heading">
          <p id="mine-confirmation-heading" className="font-mono text-xs uppercase tracking-[0.14em] text-amber-100">
            Confirm_Chain_State_Change
          </p>
          <p className="mt-2 text-sm leading-relaxed text-amber-100/70">
            Submit one authenticated mining request to the connected node?
          </p>
          <div className="mt-4 grid gap-3 sm:grid-cols-2">
            <button
              type="button"
              onClick={onConfirm}
              className="min-h-11 bg-cyan-glow px-4 font-mono text-xs font-black uppercase tracking-[0.16em] text-ink-black transition-colors hover:bg-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"
            >
              CONFIRM_MINE
            </button>
            <button
              type="button"
              onClick={onCancel}
              className="inline-flex min-h-11 items-center justify-center gap-2 border border-platinum/15 px-4 font-mono text-xs font-bold uppercase tracking-[0.16em] text-platinum/60 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"
            >
              <X className="h-4 w-4" aria-hidden="true" />
              CANCEL
            </button>
          </div>
        </div>
      )}

      {state === "success" && result && (
        <div className="mt-5 border border-emerald-300/30 bg-emerald-300/10 p-4" role="status" aria-live="polite">
          <div className="flex items-start gap-3 text-emerald-200">
            <CheckCircle2 className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
            <div>
              <p className="font-mono text-xs font-bold uppercase tracking-[0.14em]">Block_Committed</p>
              <p className="mt-2 text-sm text-emerald-100/75">
                {result.index === null ? result.message : `Block #${result.index} committed. ${result.message}`}
              </p>
            </div>
          </div>
        </div>
      )}

      {hasError && error && (
        <div className="mt-5 border border-red-300/35 bg-red-300/10 p-4" role="alert" aria-live="assertive">
          <div className="flex items-start gap-3 text-red-200">
            <AlertTriangle className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />
            <div>
              <p className="font-mono text-xs font-bold uppercase tracking-[0.14em]">MINING_REQUEST_REJECTED</p>
              <p className="mt-2 text-sm text-red-100/75">{error}</p>
            </div>
          </div>
        </div>
      )}

      {state !== "armed" && (
        <button
          type="button"
          onClick={onArm}
          disabled={isSubmitting}
          aria-busy={isSubmitting}
          className="mt-5 flex min-h-12 w-full items-center justify-center gap-2 bg-cyan-glow px-4 font-mono text-xs font-black uppercase tracking-[0.18em] text-ink-black transition-colors hover:bg-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-60"
        >
          {isSubmitting ? "MINING_REQUEST_PENDING" : state === "success" ? "ARM_MANUAL_MINE_AGAIN" : "ARM_MANUAL_MINE"}
        </button>
      )}
    </section>
  );
}
