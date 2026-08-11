"use client";

import { Activity, Clock3 } from "lucide-react";
import type { LatestTransaction, LiveMetricStatus } from "@/lib/api";

interface OperatorActivityProps {
  transactions: LatestTransaction[];
  status: LiveMetricStatus;
}

export default function OperatorActivity({ transactions, status }: OperatorActivityProps) {
  return (
    <section aria-labelledby="operator-activity-heading" className="border border-platinum/10 bg-ink-black/70">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-platinum/10 px-5 py-4 sm:px-6">
        <div className="flex items-center gap-3">
          <Activity className="h-4 w-4 text-cyan-glow" aria-hidden="true" />
          <h2 id="operator-activity-heading" className="font-mono text-xs font-bold uppercase tracking-[0.18em] text-platinum/70">
            Recent_Network_Activity
          </h2>
        </div>
        <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/30">
          {status === "loading" ? "CONNECTING" : status === "live" ? "NODE_RETURN" : "UNAVAILABLE"}
        </span>
      </div>

      {transactions.length > 0 ? (
        <div className="divide-y divide-platinum/5" aria-live="polite">
          {transactions.slice(0, 6).map((transaction) => (
            <div key={transaction.id} className="grid gap-2 px-5 py-4 sm:grid-cols-[minmax(0,1fr)_auto_auto] sm:items-center sm:gap-6 sm:px-6">
              <div className="min-w-0">
                <p className="truncate font-mono text-xs text-cyan-glow/75" title={transaction.hash}>{transaction.hash}</p>
                <p className="mt-1 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/30">TX_{transaction.id}</p>
              </div>
              <span className="font-mono text-xs text-platinum/70">{transaction.amount} $ULTRA</span>
              <span className="flex items-center gap-1 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/30">
                <Clock3 className="h-3 w-3" aria-hidden="true" />
                {transaction.shard === null ? "SH_—" : `SH_${transaction.shard}`}
              </span>
            </div>
          ))}
        </div>
      ) : (
        <div className="px-6 py-12 text-center font-mono text-[10px] uppercase tracking-[0.18em] text-platinum/35" aria-live="polite">
          {status === "loading" ? "CONNECTING_TO_LIVE_LEDGER" : status === "live" ? "NO_RECENT_TRANSACTIONS_RETURNED_BY_NODE" : "NETWORK_ACTIVITY_UNAVAILABLE"}
        </div>
      )}
    </section>
  );
}
