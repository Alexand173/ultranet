"use client";

import { useEffect, useState } from "react";
import { fetchMempool, type MempoolSnapshot, type TransactionRecord } from "@/lib/api";
import { formatAddress, formatUltraAmount } from "@/lib/transaction-format";
import TerminalFrame from "./TerminalFrame";

const REFRESH_MS = 5_000;

type FeedStatus = "loading" | "live" | "unavailable";

interface PendingMempoolTerminalProps {
  className?: string;
}

/** Age of a pending transaction, in the shortest readable unit. */
function ageLabel(timestamp: number, nowMs: number): string {
  if (!Number.isSafeInteger(timestamp) || timestamp <= 0) return "—";
  const seconds = Math.max(0, Math.round(nowMs / 1000) - timestamp);
  if (seconds < 60) return `${seconds}s`;
  if (seconds < 3_600) return `${Math.floor(seconds / 60)}m`;
  return `${Math.floor(seconds / 3_600)}h`;
}

/** Total value still waiting to be sealed into a block. */
function heldValue(transactions: TransactionRecord[]): string {
  const total = transactions.reduce(
    (sum, transaction) => sum + BigInt(Math.max(0, Math.trunc(transaction.amount))),
    0n,
  );
  return formatUltraAmount(total);
}

function StatusStrip({
  status,
  count,
  transactions,
}: {
  status: FeedStatus;
  count: number;
  transactions: TransactionRecord[];
}) {
  if (status === "unavailable") {
    return (
      <div className="flex items-center gap-3 px-6 py-3 border-b border-platinum/10">
        <span className="font-mono text-xs uppercase tracking-widest text-platinum/40">
          Feed unavailable
        </span>
      </div>
    );
  }

  return (
    <div className="flex items-center justify-between gap-4 px-6 py-3 border-b border-platinum/10">
      <span
        aria-live="polite"
        className="font-mono text-xs uppercase tracking-widest text-cyan-glow/70"
      >
        {status === "loading" ? "Reading mempool" : `${count} queued`}
      </span>
      {transactions.length > 0 && (
        <span className="font-mono text-xs uppercase tracking-widest text-platinum/35">
          Held {heldValue(transactions)}
        </span>
      )}
    </div>
  );
}

function EmptyState({ status }: { status: FeedStatus }) {
  const message =
    status === "loading"
      ? "Reading the mempool…"
      : status === "unavailable"
        ? "The node did not return the mempool."
        : "Mempool is empty — every transaction is in a block.";

  return (
    <div className="flex h-full min-h-32 items-center justify-center px-8 text-center font-mono text-sm uppercase tracking-widest text-platinum/35">
      {message}
    </div>
  );
}

export default function PendingMempoolTerminal({ className }: PendingMempoolTerminalProps) {
  const [snapshot, setSnapshot] = useState<MempoolSnapshot>({ count: 0, transactions: [] });
  const [status, setStatus] = useState<FeedStatus>("loading");
  const [nowMs, setNowMs] = useState(() => Date.now());

  useEffect(() => {
    let mounted = true;
    let controller: AbortController | null = null;

    const load = async () => {
      controller?.abort();
      controller = new AbortController();
      try {
        const nextSnapshot = await fetchMempool(controller.signal);
        if (mounted) {
          setSnapshot(nextSnapshot);
          setStatus("live");
          setNowMs(Date.now());
        }
      } catch {
        if (mounted) setStatus("unavailable");
      }
    };

    void load();
    const interval = window.setInterval(() => void load(), REFRESH_MS);
    return () => {
      mounted = false;
      controller?.abort();
      window.clearInterval(interval);
    };
  }, []);

  return (
    <TerminalFrame
      title="Mempool // Pending_Transactions"
      liveLabel={status === "live" ? "PENDING_TX_FEED" : status === "loading" ? "CONNECTING" : undefined}
      cornerAccent="br"
      className={className}
    >
      <div className="flex h-full flex-col">
        <StatusStrip
          status={status}
          count={snapshot.count}
          transactions={snapshot.transactions}
        />

        <div className="relative min-h-0 flex-1">
          <div className="pointer-events-none absolute inset-x-0 top-0 z-10 h-4 bg-gradient-to-b from-ink-black to-transparent" />
          <div className="pointer-events-none absolute inset-x-0 bottom-0 z-10 h-4 bg-gradient-to-t from-ink-black to-transparent" />

          {snapshot.transactions.length > 0 ? (
            <ul className="h-full overflow-y-auto py-1" role="list">
              {snapshot.transactions.map((transaction) => (
                <li
                  key={transaction.hash}
                  className="grid grid-cols-[minmax(0,1fr)_116px_56px] items-baseline gap-3 px-6 py-3 font-mono text-sm border-b border-platinum/[0.04] last:border-b-0 hover:bg-cyan-glow/[0.04]"
                >
                  <span className="min-w-0">
                    <span
                      className="block truncate text-cyan-glow/70"
                      title={`${transaction.hash} (${transaction.status})`}
                    >
                      {formatAddress(transaction.hash, 10, 6)}
                    </span>
                    <span
                      className="block truncate text-[11px] text-platinum/35"
                      title={`${transaction.sender} → ${transaction.recipient}`}
                    >
                      {formatAddress(transaction.sender, 6, 4)} →{" "}
                      {formatAddress(transaction.recipient, 6, 4)}
                    </span>
                  </span>
                  <span className="text-right tabular-nums text-platinum/80">
                    {formatUltraAmount(transaction.amount)}
                  </span>
                  <span className="text-right tabular-nums text-platinum/35">
                    {ageLabel(transaction.timestamp, nowMs)}
                  </span>
                </li>
              ))}
            </ul>
          ) : (
            <EmptyState status={status} />
          )}
        </div>
      </div>
    </TerminalFrame>
  );
}
