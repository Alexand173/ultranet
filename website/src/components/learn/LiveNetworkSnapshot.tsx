"use client";

import { useEffect, useState } from "react";
import { Activity, Blocks, Clock3, ShieldCheck } from "lucide-react";
import { emptyNetworkStats, fetchNetworkStats, type LiveNetworkStats } from "@/lib/api";

function formatInteger(value: number | null): string {
  return value === null ? "Unavailable" : new Intl.NumberFormat("en-US").format(value);
}

function formatWeight(value: number | null): string {
  if (value === null) return "Unavailable";
  return new Intl.NumberFormat("en-US", { notation: "compact", maximumFractionDigits: 2 }).format(value);
}

function formatMilliseconds(value: number | null): string {
  return value === null ? "Unavailable" : `${new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(value)} ms`;
}

function MetricCard({ label, value, detail, icon: Icon }: { label: string; value: string; detail: string; icon: typeof Activity }) {
  return (
    <div className="rounded-2xl border border-learn-ink/10 bg-white p-5 shadow-[0_8px_24px_rgba(16,17,20,0.04)]">
      <div className="flex items-center justify-between gap-3 text-learn-muted">
        <span className="text-xs font-semibold uppercase tracking-[0.16em]">{label}</span>
        <Icon className="h-4 w-4 text-learn-violet" aria-hidden="true" />
      </div>
      <p className="mt-4 break-words font-space-grotesk text-2xl font-bold tracking-tight text-learn-ink">{value}</p>
      <p className="mt-1 text-sm text-learn-muted">{detail}</p>
    </div>
  );
}

export default function LiveNetworkSnapshot({ compact = false, refreshMs = 15000 }: { compact?: boolean; refreshMs?: number }) {
  const [stats, setStats] = useState<LiveNetworkStats>(() => emptyNetworkStats());

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
    const interval = window.setInterval(() => void load(), refreshMs);
    return () => {
      mounted = false;
      controller?.abort();
      window.clearInterval(interval);
    };
  }, [refreshMs]);

  const isLive = stats.status === "live";
  const cards = [
    { label: "Committed blocks", value: formatInteger(stats.totalBlocks), detail: "Current node observation", icon: Blocks },
    { label: "Active validators", value: formatInteger(stats.validatorCount), detail: "Validator records on this node", icon: ShieldCheck },
    { label: "Validator weight", value: formatWeight(stats.totalWeight), detail: "Reported total weight", icon: Activity },
    { label: "Latest proving", value: formatMilliseconds(stats.provingTimeMs), detail: "Only shown when measured", icon: Clock3 },
  ];

  return (
    <section aria-labelledby="live-network-heading" className={compact ? "space-y-4" : "space-y-6"}>
      <div className="flex flex-wrap items-end justify-between gap-4">
        <div>
          <p className="text-xs font-semibold uppercase tracking-[0.2em] text-learn-violet">Live node snapshot</p>
          <h2 id="live-network-heading" className="mt-2 font-space-grotesk text-2xl font-bold tracking-tight text-learn-ink sm:text-3xl">
            Read the network as it is
          </h2>
        </div>
        <div className="flex items-center gap-2 text-sm text-learn-muted" aria-live="polite">
          <span className={`h-2.5 w-2.5 rounded-full ${isLive ? "bg-emerald-500" : stats.status === "loading" ? "bg-amber-400 animate-pulse" : "bg-slate-400"}`} aria-hidden="true" />
          {isLive ? "Connected to node" : stats.status === "loading" ? "Connecting…" : "Node data unavailable"}
        </div>
      </div>

      <div className={`grid gap-4 ${compact ? "sm:grid-cols-2" : "sm:grid-cols-2 xl:grid-cols-4"}`}>
        {cards.map((card) => <MetricCard key={card.label} {...card} />)}
      </div>
      <p className="text-xs leading-relaxed text-learn-muted">
        Values are read from the configured UltraNet API. Unavailable is intentional: this page never invents TPS, balances, transactions, or validator counts.
      </p>
    </section>
  );
}
