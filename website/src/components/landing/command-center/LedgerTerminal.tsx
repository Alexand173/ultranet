"use client";

import { useEffect, useState } from "react";
import { cn } from "@/lib/utils";
import { fetchLatestTransactions, type LatestTransaction } from "@/lib/api";
import TerminalFrame from "./TerminalFrame";

export default function LedgerTerminal({ className }: { className?: string }) {
  const [txs, setTxs] = useState<LatestTransaction[]>([]);
  const [status, setStatus] = useState<"loading" | "live" | "unavailable">("loading");

  useEffect(() => {
    let mounted = true;
    let controller: AbortController | null = null;

    const load = async () => {
      controller?.abort();
      controller = new AbortController();
      try {
        const nextTransactions = await fetchLatestTransactions(controller.signal);
        if (mounted) {
          setTxs(nextTransactions);
          setStatus("live");
        }
      } catch {
        if (mounted) {
          setTxs([]);
          setStatus("unavailable");
        }
      }
    };

    void load();
    const interval = window.setInterval(() => void load(), 5000);
    return () => {
      mounted = false;
      controller?.abort();
      window.clearInterval(interval);
    };
  }, []);

  return (
    <TerminalFrame
      title="Ledger // Live_Block_Stream"
      liveLabel={status === "live" ? "MEMPOOL_LIVE" : status === "loading" ? "CONNECTING" : "NO_DATA"}
      cornerAccent="tr"
      className={className}
    >
      <div className="relative h-full overflow-hidden">
        <div className="absolute top-0 left-0 right-0 z-10 h-4 bg-gradient-to-b from-ink-black to-transparent pointer-events-none" />
        <div className="absolute bottom-0 left-0 right-0 z-10 h-4 bg-gradient-to-t from-ink-black to-transparent pointer-events-none" />

        {txs.length > 0 ? (
          <div className="marquee-vertical flex flex-col">
            {[...txs, ...txs].map((tx, i) => (
              <div key={`${tx.id}-${i}`} className={cn("grid grid-cols-[1fr_110px_90px] gap-4 px-6 py-5 text-lg font-mono border-b border-platinum/[0.03]", i % 2 === 0 ? "bg-platinum/[0.01]" : "")}>
                <span className="text-cyan-glow/70 truncate" title={tx.hash}>{tx.hash.slice(0, 12)}…</span>
                <span className="text-platinum/70 text-right">{tx.amount}</span>
                <span className="text-platinum/20 text-right">{tx.shard === null ? "SH_—" : `SH_${tx.shard}`}</span>
              </div>
            ))}
          </div>
        ) : (
          <div className="flex h-full min-h-44 items-center justify-center px-8 text-center font-mono text-sm uppercase tracking-widest text-platinum/35">
            {status === "loading" ? "Connecting to the live ledger…" : "No recent transactions returned by the node"}
          </div>
        )}
      </div>
    </TerminalFrame>
  );
}
