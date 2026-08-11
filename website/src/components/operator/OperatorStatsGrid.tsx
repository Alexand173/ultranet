"use client";

import { Activity, Blocks, Clock3, Database, Gauge, Layers, ListChecks, RefreshCw, ShieldCheck } from "lucide-react";
import type { LiveNetworkStats } from "@/lib/api";

interface OperatorStatsGridProps {
  stats: LiveNetworkStats;
  isRefreshing: boolean;
  onRefresh: () => void;
}

function formatInteger(value: number | null): string {
  return value === null ? "Unavailable" : new Intl.NumberFormat("en-US").format(value);
}

function formatMetric(value: number | null, suffix = ""): string {
  return value === null
    ? "Unavailable"
    : `${new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(value)}${suffix}`;
}

function formatUpdatedAt(value: string | null): string {
  if (!value) return "—";
  return new Intl.DateTimeFormat("en-US", { timeStyle: "medium" }).format(new Date(value));
}

export default function OperatorStatsGrid({ stats, isRefreshing, onRefresh }: OperatorStatsGridProps) {
  const cards = [
    { label: "Committed blocks", value: formatInteger(stats.totalBlocks), detail: "Current node observation", icon: Blocks },
    { label: "Active validators", value: formatInteger(stats.validatorCount), detail: "Validator records", icon: ShieldCheck },
    { label: "Validator weight", value: formatInteger(stats.totalWeight), detail: "Reported total weight", icon: Activity },
    { label: "Mempool", value: formatInteger(stats.mempoolSize), detail: "Pending transactions", icon: Database },
    { label: "Total transactions", value: formatInteger(stats.totalTransactions), detail: "Committed node total", icon: ListChecks },
    { label: "Current TPS", value: formatMetric(stats.tps), detail: "Only shown when measured", icon: Gauge },
    { label: "Latest proving", value: formatMetric(stats.provingTimeMs, " ms"), detail: "Only shown when measured", icon: Clock3 },
    { label: "Active AppChains", value: formatInteger(stats.activeAppchains), detail: "Reported L3 chains", icon: Layers },
  ];

  const statusLabel = stats.status === "live" ? "CONNECTED_TO_NODE" : stats.status === "loading" ? "CONNECTING" : "NODE_DATA_UNAVAILABLE";
  const statusClass = stats.status === "live" ? "text-emerald-300" : stats.status === "loading" ? "text-amber-300" : "text-red-200";

  return (
    <section aria-labelledby="operator-telemetry-heading" className="space-y-5">
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="font-mono text-[10px] uppercase tracking-[0.24em] text-cyan-glow/70">Live_Node_Telemetry</p>
          <h2 id="operator-telemetry-heading" className="mt-2 font-space-grotesk text-2xl font-bold uppercase tracking-tight text-platinum sm:text-3xl">
            Read the network as it is
          </h2>
        </div>
        <div className="flex flex-wrap items-center gap-3 font-mono text-[10px] uppercase tracking-[0.16em]">
          <span className={`flex items-center gap-2 ${statusClass}`} aria-live="polite">
            <span className="h-1.5 w-1.5 rounded-full bg-current" aria-hidden="true" />
            {statusLabel}
          </span>
          <span className="text-platinum/30">UPDATED {formatUpdatedAt(stats.updatedAt)}</span>
          <button
            type="button"
            onClick={onRefresh}
            disabled={isRefreshing}
            aria-label="Refresh telemetry"
            title="Refresh telemetry"
            className="inline-flex min-h-10 items-center gap-2 border border-platinum/15 px-3 text-platinum/60 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-50"
          >
            <RefreshCw className={`h-3.5 w-3.5 ${isRefreshing ? "animate-spin" : ""}`} aria-hidden="true" />
            Refresh
          </button>
        </div>
      </div>

      <div className="grid gap-px border border-platinum/10 bg-platinum/10 sm:grid-cols-2 xl:grid-cols-4">
        {cards.map(({ label, value, detail, icon: Icon }) => (
          <div key={label} className="min-w-0 bg-ink-black/80 p-5 transition-colors hover:bg-cyan-glow/[0.03]">
            <div className="flex items-start justify-between gap-3">
              <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/40">{label}</span>
              <Icon className="h-4 w-4 shrink-0 text-cyan-glow/60" aria-hidden="true" />
            </div>
            <p className={`mt-5 break-words font-space-grotesk text-2xl font-bold tracking-tight ${value === "Unavailable" ? "text-platinum/40" : "text-platinum"}`}>
              {value}
            </p>
            <p className="mt-1 font-mono text-[10px] leading-relaxed text-platinum/30">{detail}</p>
          </div>
        ))}
      </div>
      <p className="font-mono text-[10px] leading-relaxed text-platinum/35" aria-live="polite">
        Unavailable means the connected node does not expose that measurement; no value is fabricated by this console.
      </p>
    </section>
  );
}
