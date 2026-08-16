"use client";

import { cn } from "@/lib/utils";

const TICKER_ITEMS = [
  "STATUS: ACTIVE_L1_NODE",
  "TFHE_PROTOCOL_V.7.1 // Q_RESISTANT",
  "SHARD_INTEGRITY: VERIFIED",
  "DILITHIUM-5_SIG: ENABLED",
  "GOSSIP_LAYER: SYNCED",
  "MYSTICETI_DAG: COMMIT_OK",
];

interface StatusTickerProps {
  position: "top" | "bottom";
  blockHeight: string;
  provingTime: string;
}

export default function StatusTicker({ position, blockHeight, provingTime }: StatusTickerProps) {
  const items = [...TICKER_ITEMS, ...TICKER_ITEMS];

  return (
    <div
      className={cn(
        "w-full min-w-0 shrink-0 h-8 flex items-center overflow-hidden bg-ink-black/90 font-mono text-[9px] uppercase tracking-widest",
        position === "top" ? "border-b border-platinum/10" : "border-t border-platinum/10"
      )}
    >
      <div className="shrink-0 flex items-center gap-2 px-4 border-r border-platinum/10 h-full text-cyan-glow/80">
        <span className="w-1.5 h-1.5 bg-cyan-glow rounded-full animate-pulse shadow-[0_0_8px_#0FFFFF]" />
        {position === "top" ? "SOVEREIGN_ENGINE" : "NETWORK_OPERATIONAL"}
      </div>

      <div className="relative min-w-0 flex-1 overflow-hidden h-full flex items-center">
        <div className="ticker-track gap-12 whitespace-nowrap text-platinum/40">
          {items.map((item, i) => (
            <span key={i} className="flex items-center gap-2">
              <span className="text-cyan-glow/30">{"//"}</span>
              {item}
            </span>
          ))}
        </div>
      </div>

      <div className="shrink-0 flex items-center gap-4 px-4 border-l border-platinum/10 h-full text-platinum/40">
        <span>HEIGHT: <span className="text-platinum/70">{blockHeight}</span></span>
        <span className="hidden sm:inline">PROVE: <span className="text-platinum/70">{provingTime}</span></span>
      </div>
    </div>
  );
}
