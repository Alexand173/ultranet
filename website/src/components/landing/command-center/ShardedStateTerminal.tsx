"use client";

import { useEffect, useState } from "react";
import { fetchNetworkStats, type LiveNetworkStats } from "@/lib/api";
import TerminalFrame from "./TerminalFrame";

interface GaugeConfig {
  key: keyof Pick<LiveNetworkStats, "tps" | "provingTimeMs" | "validatorCount" | "totalWeight">;
  label: string;
  suffix: string;
}

const GAUGES: GaugeConfig[] = [
  { key: "tps", label: "TPS", suffix: "" },
  { key: "provingTimeMs", label: "PROVING", suffix: "ms" },
  { key: "validatorCount", label: "VALIDATORS", suffix: "" },
  { key: "totalWeight", label: "WEIGHT", suffix: "" },
];

function formatValue(value: number | null, suffix: string): string {
  if (value === null) return "—";
  return `${new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(value)}${suffix}`;
}

function RadialGauge({ label, value, suffix }: { label: string; value: number | null; suffix: string }) {
  // The ring communicates data availability only; it is not a fabricated magnitude.
  const pct = value === null ? 0 : 100;
  return (
    <div className="flex flex-col items-center justify-center gap-3 py-4">
      <div className="relative flex h-28 w-28 items-center justify-center rounded-full" style={{ background: `conic-gradient(#0FFFFF ${pct}%, rgba(242,243,244,0.06) ${pct}%)` }}>
        <div className="absolute inset-[6px] flex items-center justify-center rounded-full bg-ink-black">
          <span className={`text-center text-sm font-mono font-bold ${value === null ? "text-platinum/35" : "text-platinum"}`}>
            {formatValue(value, suffix)}
          </span>
        </div>
      </div>
      <span className="text-xs font-mono uppercase tracking-widest text-platinum/40">{label}</span>
    </div>
  );
}

export default function ShardedStateTerminal({ className }: { className?: string }) {
  const [stats, setStats] = useState<LiveNetworkStats>({
    totalBlocks: null,
    validatorCount: null,
    totalWeight: null,
    tps: null,
    provingTimeMs: null,
    activeAppchains: null,
    status: "loading",
    updatedAt: null,
  });

  useEffect(() => {
    let mounted = true;
    let controller: AbortController | null = null;

    const load = async () => {
      controller?.abort();
      controller = new AbortController();
      try {
        const nextStats = await fetchNetworkStats(controller.signal);
        if (mounted) setStats(nextStats);
      } catch {
        if (mounted) setStats((current) => ({ ...current, status: "unavailable", updatedAt: null }));
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

  return (
    <TerminalFrame
      title="Gametocyte // Sharded_State"
      liveLabel={stats.status === "live" ? "NODE_STATS_LIVE" : stats.status === "loading" ? "CONNECTING" : "NO_DATA"}
      cornerAccent="bl"
      className={className}
    >
      <div className="flex h-full flex-col">
        <div className="grid flex-1 grid-cols-2 gap-x-1">
          {GAUGES.map((gauge) => <RadialGauge key={gauge.key} label={gauge.label} value={stats[gauge.key]} suffix={gauge.suffix} />)}
        </div>
        <p className="border-t border-platinum/5 px-6 py-3 text-xs font-mono uppercase tracking-widest text-platinum/25">
          Unavailable means the current public API does not expose that measurement.
        </p>
      </div>
    </TerminalFrame>
  );
}
