"use client";

import { CircleDollarSign, Layers3, RefreshCw, ShieldCheck } from "lucide-react";
import { formatUltraAmount } from "@/lib/transaction-format";
import type { AppChainOverview as AppChainOverviewData, AppChainAction } from "@/lib/appchains.types";
import AppChainTable from "@/components/appchains/AppChainTable";

interface AppChainOverviewProps {
  overview: AppChainOverviewData | null;
  isLoading: boolean;
  isRefreshing: boolean;
  error: string;
  onRefresh: () => void;
  onRegister: () => void;
  actions: Record<number, AppChainAction>;
  onArm: (chainId: number) => void;
  onCancel: (chainId: number) => void;
  onConfirm: (chainId: number) => void;
}

function formatCount(value: number | null): string {
  return value === null ? "Unavailable" : new Intl.NumberFormat("en-US").format(value);
}

export default function AppChainOverview({
  overview,
  isLoading,
  isRefreshing,
  error,
  onRefresh,
  onRegister,
  actions,
  onArm,
  onCancel,
  onConfirm,
}: AppChainOverviewProps) {
  const hasChains = Boolean(overview && overview.chains.length > 0);
  const cards = [
    { label: "Registered AppChains", value: overview ? formatCount(overview.chains.length) : "—", detail: "Durable registry records", icon: Layers3 },
    { label: "Total anchors", value: overview ? formatCount(overview.totalAnchorCount) : "—", detail: "Server-verified state anchors", icon: ShieldCheck },
    { label: "Total anchor spend", value: overview ? `${formatUltraAmount(overview.totalAnchorSpendBaseUnits)} $ULTRA` : "—", detail: "Debited from L1 treasuries", icon: CircleDollarSign },
  ];
  const emptyDescription = overview?.anchorAvailability === "production"
    ? "Create the first L3 registry record to begin server-verified anchoring."
    : overview?.anchorAvailability === "test-only"
      ? "Create the first L3 registry record to begin testing server-generated anchors."
      : "Create the first L3 registry record; anchoring becomes available when the connected node exposes it.";

  return (
    <section aria-labelledby="appchain-overview-heading" className="space-y-5">
      <div className="flex flex-col gap-5 border-b border-platinum/10 pb-6 lg:flex-row lg:items-end lg:justify-between">
        <div className="max-w-2xl">
          <p className="font-mono text-[10px] uppercase tracking-[0.26em] text-cyan-glow/70">L3_AppChain_Registry</p>
          <h2 id="appchain-overview-heading" className="mt-2 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum sm:text-4xl">AppChains</h2>
          <p className="mt-4 text-sm leading-7 text-platinum/50">Read and operate the AppChain registry. Anchors are generated and verified by the node, then charged against each AppChain&apos;s dedicated L1 treasury.</p>
          <p className="mt-3 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/30">PROOF_SCHEME // {overview?.proofScheme ?? "Awaiting node contract"}</p>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          <span className={`inline-flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.16em] ${overview?.anchorAvailability === "production" ? "text-emerald-300/80" : overview?.anchorAvailability === "test-only" ? "text-amber-200/75" : "text-platinum/35"}`}><span className={`h-1.5 w-1.5 rounded-full ${overview?.anchorAvailability === "production" ? "bg-emerald-300" : overview?.anchorAvailability === "test-only" ? "bg-amber-200" : "bg-platinum/30"}`} aria-hidden="true" />{overview?.anchorAvailability === "production" ? "SERVER_VERIFIED_ANCHORING" : overview?.anchorAvailability === "test-only" ? "TEST_ONLY_ANCHORING" : "ANCHORING_DISABLED"}</span>
          <button type="button" onClick={onRefresh} disabled={isRefreshing} aria-label="Refresh AppChain registry" title="Refresh AppChain registry" className="inline-flex min-h-10 items-center gap-2 border border-platinum/15 px-3 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-platinum/60 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-50"><RefreshCw className={`h-3.5 w-3.5 ${isRefreshing ? "animate-spin" : ""}`} aria-hidden="true" />REFRESH</button>
          <button type="button" onClick={onRegister} className="inline-flex min-h-10 items-center justify-center gap-2 bg-cyan-glow px-4 font-mono text-[10px] font-black uppercase tracking-[0.14em] text-ink-black transition-colors hover:bg-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow">REGISTER NEW APPCHAIN</button>
        </div>
      </div>

      {error && <div className="border border-red-300/30 bg-red-300/10 px-4 py-3 font-mono text-xs leading-5 text-red-200" role="alert" aria-live="assertive">{error}</div>}

      <div className="grid gap-px border border-platinum/10 bg-platinum/10 md:grid-cols-3">
        {cards.map(({ label, value, detail, icon: Icon }) => (
          <div key={label} className="min-w-0 bg-ink-black/80 p-5 transition-colors hover:bg-cyan-glow/[0.03]">
            <div className="flex items-start justify-between gap-3"><span className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/40">{label}</span><Icon className="h-4 w-4 shrink-0 text-cyan-glow/60" aria-hidden="true" /></div>
            <p className={`mt-5 break-words font-space-grotesk text-2xl font-bold tracking-tight ${overview ? "text-platinum" : "text-platinum/35"}`}>{value}</p>
            <p className="mt-1 font-mono text-[10px] leading-relaxed text-platinum/30">{detail}</p>
          </div>
        ))}
      </div>

      {isLoading && !overview && (
        <div className="border border-platinum/10 bg-ink-black/70 p-8" aria-busy="true" aria-live="polite"><div className="space-y-4 animate-pulse"><div className="h-3 w-32 bg-platinum/10" /><div className="h-12 w-full bg-platinum/5" /><div className="h-12 w-5/6 bg-platinum/5" /></div><p className="mt-6 font-mono text-[10px] uppercase tracking-[0.18em] text-cyan-glow/60">READING_APPCHAIN_REGISTRY</p></div>
      )}

      {!isLoading && !error && !hasChains && (
        <div className="border border-dashed border-cyan-glow/20 bg-cyan-glow/[0.025] px-6 py-14 text-center" role="status"><Layers3 className="mx-auto h-7 w-7 text-cyan-glow/60" aria-hidden="true" /><p className="mt-4 font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">No AppChains are registered yet.</p><p className="mx-auto mt-3 max-w-md text-sm leading-6 text-platinum/45">{emptyDescription}</p><button type="button" onClick={onRegister} className="mt-6 inline-flex min-h-11 items-center justify-center bg-cyan-glow px-5 font-mono text-xs font-black uppercase tracking-[0.14em] text-ink-black transition-colors hover:bg-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow">REGISTER FIRST APPCHAIN</button></div>
      )}

      {overview && overview.chains.length > 0 && <AppChainTable chains={overview.chains} actions={actions} onArm={onArm} onCancel={onCancel} onConfirm={onConfirm} />}

      <p className="font-mono text-[10px] leading-relaxed text-platinum/30" aria-live="polite">Unavailable means the connected node did not expose a metric. The dashboard does not infer balances or anchor totals from incomplete history.</p>
    </section>
  );
}
