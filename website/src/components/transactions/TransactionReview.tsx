"use client";

import { AlertTriangle, ArrowLeft, Send } from "lucide-react";
import type { RefObject } from "react";
import type { AccountSnapshot, FeeEstimate } from "@/lib/api";
import { formatAddress, formatUltraAmount } from "@/lib/transaction-format";

interface TransactionReviewProps {
  account: AccountSnapshot;
  recipient: string;
  amount: bigint;
  estimate: FeeEstimate;
  busy: boolean;
  onConfirm: () => void;
  onEdit: () => void;
  headingRef: RefObject<HTMLHeadingElement | null>;
}

export default function TransactionReview({ account, recipient, amount, estimate, busy, onConfirm, onEdit, headingRef }: TransactionReviewProps) {
  return (
    <section aria-labelledby="transaction-review-title" className="cut-corner neon-inset p-6 sm:p-8" aria-busy={busy}>
      <div className="flex items-start justify-between gap-6"><div><p className="font-mono text-[10px] uppercase tracking-[0.2em] text-cyan-glow">Transfer // confirmation required</p><h2 ref={headingRef} id="transaction-review-title" tabIndex={-1} className="mt-3 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum">Review transfer</h2></div><Send className="h-6 w-6 text-cyan-glow" aria-hidden="true" /></div>
      <p className="mt-4 text-sm leading-7 text-platinum/55">Check the complete recipient and amount before the wallet creates a local signature.</p>

      <dl className="mt-7 border-y border-cyan-glow/15 py-4 font-mono text-xs leading-6">
        <div className="grid gap-1 border-b border-cyan-glow/10 py-3 sm:grid-cols-[150px_1fr]"><dt className="text-platinum/45">From this wallet</dt><dd className="break-all text-platinum/80">{formatAddress(account.address)}<span className="sr-only"> {account.address}</span></dd></div>
        <div className="grid gap-1 border-b border-cyan-glow/10 py-3 sm:grid-cols-[150px_1fr]"><dt className="text-platinum/45">To</dt><dd className="break-all text-platinum/85">{recipient}</dd></div>
        <div className="grid gap-1 border-b border-cyan-glow/10 py-3 sm:grid-cols-[150px_1fr]"><dt className="text-platinum/45">Amount</dt><dd className="text-platinum/80">{formatUltraAmount(amount)} ULTRA</dd></div>
        <div className="grid gap-1 border-b border-cyan-glow/10 py-3 sm:grid-cols-[150px_1fr]"><dt className="text-platinum/45">Network fee</dt><dd className="text-platinum/80">{formatUltraAmount(BigInt(estimate.fee))} ULTRA</dd></div>
        <div className="grid gap-1 py-3 font-bold sm:grid-cols-[150px_1fr]"><dt className="text-platinum/70">Total deducted</dt><dd className="text-platinum">{formatUltraAmount(BigInt(estimate.total))} ULTRA</dd></div>
        <div className="grid gap-1 py-1 sm:grid-cols-[150px_1fr]"><dt className="text-platinum/45">Balance after</dt><dd className="text-platinum/80">{formatUltraAmount(BigInt(account.balance) - BigInt(estimate.total))} ULTRA</dd></div>
      </dl>

      <div className="mt-6 flex items-start gap-3 border border-amber-300/30 bg-amber-300/10 p-4 text-xs leading-6 text-amber-100"><AlertTriangle className="mt-1 h-4 w-4 shrink-0" aria-hidden="true" /><p>Transfers cannot be reversed. Check the address and amount before sending.</p></div>
      {busy && <div className="mt-6" role="status" aria-live="polite"><div className="h-px w-full overflow-hidden bg-cyan-glow/20"><div className="h-full w-1/2 animate-pulse bg-cyan-glow" /></div><p className="mt-3 font-mono text-xs text-cyan-glow">Sending transfer… Keep this page open until the network responds.</p></div>}

      <div className="mt-7 flex flex-col items-start gap-4 sm:flex-row sm:items-center"><button type="button" onClick={onConfirm} disabled={busy} className="inline-flex min-h-11 w-full items-center justify-center gap-3 bg-cyan-glow px-6 py-4 font-mono text-xs font-black uppercase tracking-[0.14em] text-ink-black hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-[#0A0A1A] disabled:cursor-wait disabled:opacity-50 sm:w-auto">{busy ? "Sending…" : "Confirm and send"}</button><button type="button" onClick={onEdit} disabled={busy} className="inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/55 underline decoration-platinum/25 underline-offset-4 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-[#0A0A1A] disabled:opacity-40"><ArrowLeft className="h-3.5 w-3.5" aria-hidden="true" /> Edit transfer</button></div>
    </section>
  );
}
