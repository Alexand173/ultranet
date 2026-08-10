"use client";

import { useEffect, useState } from "react";
import { ShieldCheck } from "lucide-react";
import { cn } from "@/lib/utils";
import { emptyNetworkStats, fetchNetworkStats } from "@/lib/api";
import TerminalFrame from "./TerminalFrame";

function formatCount(value: number | null): string {
  return value === null ? "Unavailable" : new Intl.NumberFormat("en-US").format(value);
}

export default function NodeDiagnosticsTerminal({ className }: { className?: string }) {
  const [validatorCount, setValidatorCount] = useState<number | null>(null);
  const [status, setStatus] = useState<"loading" | "live" | "unavailable">("loading");

  useEffect(() => {
    let mounted = true;
    let controller: AbortController | null = null;

    const load = async () => {
      controller?.abort();
      controller = new AbortController();
      try {
        const stats = await fetchNetworkStats(controller.signal);
        if (mounted) {
          setValidatorCount(stats.validatorCount);
          setStatus(stats.status);
        }
      } catch {
        if (mounted) {
          setValidatorCount(emptyNetworkStats("unavailable").validatorCount);
          setStatus("unavailable");
        }
      }
    };

    void load();
    const interval = window.setInterval(() => void load(), 10000);
    return () => {
      mounted = false;
      controller?.abort();
      window.clearInterval(interval);
    };
  }, []);

  const label = validatorCount === null ? "UNAVAILABLE_NODES" : `${validatorCount}_NODES`;

  return (
    <TerminalFrame
      title="Wolfbane_Protocol // Node_Diagnostics"
      liveLabel={label}
      cornerAccent="tl"
      className={className}
      contentClassName="hex-grid"
    >
      <div className="flex h-full flex-col">
        <div className="grid grid-cols-[1fr_130px] px-6 py-4 border-b border-platinum/5 text-sm font-mono uppercase tracking-widest text-platinum/30">
          <span>Node_Metric</span>
          <span className="text-right">Value</span>
        </div>
        <div className="flex-1 overflow-hidden">
          <div className={cn("grid grid-cols-[1fr_130px] px-6 py-5 text-lg font-mono border-b border-platinum/[0.03]", status === "live" ? "bg-cyan-glow/[0.03]" : "bg-platinum/[0.01]")}>
            <span className="text-platinum/60 truncate flex items-center gap-3">
              <ShieldCheck className="h-5 w-5 shrink-0 text-cyan-glow/40" aria-hidden="true" />
              Active_Validators
            </span>
            <span className="text-right text-cyan-glow">{formatCount(validatorCount)}</span>
          </div>
          <div className="px-6 py-5 text-sm leading-relaxed text-platinum/40">
            Per-validator ping, stake, and identity details are not exposed by the current public stats endpoint. This panel will not fabricate rows or latency values.
          </div>
        </div>
        <div className="px-6 py-3 border-t border-platinum/5 text-xs font-mono text-platinum/20 uppercase tracking-widest">
          {status === "live" ? "NODE_STATS: LIVE" : status === "loading" ? "NODE_STATS: CONNECTING" : "NODE_STATS: UNAVAILABLE"}
        </div>
      </div>
    </TerminalFrame>
  );
}
