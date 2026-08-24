"use client";

import { AlertTriangle, CheckCircle2, ExternalLink, RefreshCw } from "lucide-react";
import type { RefObject, ReactNode } from "react";
import type { TransactionRecord } from "@/lib/api";
import { EXPLORER_URL } from "@/lib/links";
import { formatAddress, formatUltraAmount } from "@/lib/transaction-format";
import CopyAddressButton from "@/components/transactions/CopyAddressButton";
import type { TransferState } from "@/components/transactions/transaction.types";

interface TransactionResultProps {
  state: Extract<TransferState, "pending" | "success" | "failed" | "unknown">;
  result: TransactionRecord | null;
  error: string;
  onSendAnother: () => void;
  onTryAgain: () => void;
  onEdit: () => void;
  onCheckStatus: () => void;
  headingRef: RefObject<HTMLHeadingElement | null>;
}

export default function TransactionResult({
  state,
  result,
  error,
  onSendAnother,
  onTryAgain,
  onEdit,
  onCheckStatus,
  headingRef,
}: TransactionResultProps) {
  if (state === "pending" && result) {
    return (
      <ResultFrame
        headingRef={headingRef}
        icon={<RefreshCw className="h-7 w-7 animate-pulse text-amber-200" aria-hidden="true" />}
        title="Transfer submitted"
        tone="amber"
      >
        <p>{formatUltraAmount(BigInt(result.amount))} ULTRA was accepted by the node and is confirming.</p>
        <ResultDetails result={result} />
        <button
          type="button"
          onClick={onCheckStatus}
          className="mt-7 inline-flex min-h-11 items-center gap-2 border border-platinum/20 px-5 py-3 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-platinum/75 hover:border-cyan-glow/60 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
        >
          <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" /> Check transaction
        </button>
      </ResultFrame>
    );
  }

  if (state === "success" && result) {
    return (
      <ResultFrame
        headingRef={headingRef}
        icon={<CheckCircle2 className="h-7 w-7 text-emerald-300" aria-hidden="true" />}
        title="Transfer sent"
        tone="emerald"
      >
        <p>
          {formatUltraAmount(BigInt(result.amount))} ULTRA was sent to{" "}
          <code className="text-platinum/80">{formatAddress(result.recipient)}</code>.
        </p>
        <ResultDetails result={result} />
        <div className="mt-7 flex flex-col items-start gap-4">
          <button
            type="button"
            onClick={onSendAnother}
            className="inline-flex min-h-11 items-center gap-2 bg-cyan-glow px-5 py-3 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-ink-black hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
          >
            Send another transfer
          </button>
          <a
            href={`${EXPLORER_URL}?search=${encodeURIComponent(result.hash)}`}
            target="_blank"
            rel="noreferrer"
            className="inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/55 underline decoration-platinum/25 underline-offset-4 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
          >
            View transaction in Explorer <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
          </a>
        </div>
      </ResultFrame>
    );
  }

  if (state === "unknown") {
    return (
      <ResultFrame
        headingRef={headingRef}
        icon={<AlertTriangle className="h-7 w-7 text-amber-200" aria-hidden="true" />}
        title="Transfer status is unknown"
        tone="amber"
      >
        <p>{error || "The network response was interrupted. Do not send it again yet."}</p>
        {result && <ResultDetails result={result} />}
        <div className="mt-7 flex flex-col items-start gap-4">
          <button
            type="button"
            onClick={onCheckStatus}
            className="inline-flex min-h-11 items-center gap-2 bg-cyan-glow px-5 py-3 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-ink-black hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
          >
            <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" /> Check transaction
          </button>
          <button
            type="button"
            onClick={onEdit}
            className="min-h-11 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/55 underline decoration-platinum/25 underline-offset-4 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
          >
            Edit transfer
          </button>
        </div>
      </ResultFrame>
    );
  }

  return (
    <ResultFrame
      headingRef={headingRef}
      icon={<AlertTriangle className="h-7 w-7 text-red-300" aria-hidden="true" />}
      title="Transfer was not sent"
      tone="red"
    >
      <p>{error || "The node rejected this transfer."}</p>
      <p className="mt-3 text-platinum/55">Your balance was not changed.</p>
      <div className="mt-7 flex flex-col items-start gap-4">
        <button
          type="button"
          onClick={onTryAgain}
          className="inline-flex min-h-11 items-center gap-2 bg-cyan-glow px-5 py-3 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-ink-black hover:bg-platinum focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
        >
          Try again
        </button>
        <button
          type="button"
          onClick={onEdit}
          className="min-h-11 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/55 underline decoration-platinum/25 underline-offset-4 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
        >
          Edit transfer
        </button>
      </div>
    </ResultFrame>
  );
}

function ResultFrame({
  icon,
  title,
  tone,
  children,
  headingRef,
}: {
  icon: ReactNode;
  title: string;
  tone: "emerald" | "amber" | "red";
  children: ReactNode;
  headingRef: RefObject<HTMLHeadingElement | null>;
}) {
  const titleClass = tone === "emerald" ? "text-emerald-300" : tone === "red" ? "text-red-200" : "text-amber-100";
  return (
    <section aria-labelledby="transaction-result-title" className="border-t border-platinum/15 pt-8" role="status" aria-live="polite">
      <div className="flex items-center gap-4">
        {icon}
        <h2 ref={headingRef} id="transaction-result-title" tabIndex={-1} className={`font-space-grotesk text-3xl font-bold uppercase tracking-tight ${titleClass}`}>
          {title}
        </h2>
      </div>
      <div className="mt-5 text-sm leading-7 text-platinum/70">{children}</div>
    </section>
  );
}

function ResultDetails({ result }: { result: TransactionRecord }) {
  return (
    <dl className="mt-6 border-y border-platinum/10 py-4 font-mono text-xs leading-6">
      <div className="flex flex-col gap-1 py-2 sm:flex-row sm:justify-between sm:gap-6"><dt className="text-platinum/45">Transaction ID</dt><dd className="break-all text-platinum/80">{result.hash}</dd></div>
      <div className="flex flex-col gap-1 py-2 sm:flex-row sm:justify-between sm:gap-6"><dt className="text-platinum/45">Status</dt><dd className="text-platinum/80">{result.status}</dd></div>
      <div className="mt-2 flex flex-wrap items-center gap-3"><CopyAddressButton value={result.hash} label="Copy transaction ID" /></div>
    </dl>
  );
}
