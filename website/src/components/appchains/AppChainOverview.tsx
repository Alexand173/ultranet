"use client";

import { CircleDollarSign, Layers3, RefreshCw, ShieldCheck } from "lucide-react";
import { formatUltraAmount } from "@/lib/transaction-format";
import type { AppChainOverview as AppChainOverviewData } from "@/lib/appchains.types";
import AppChainTable from "@/components/appchains/AppChainTable";

interface AppChainOverviewProps {
  overview: AppChainOverviewData | null;
  isLoading: boolean;
  isRefreshing: boolean;
  error: string;
  onRefresh: () => void;
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
}: AppChainOverviewProps) {
  const hasChains = Boolean(overview && overview.chains.length > 0);
  const cards = [
    { label: "Registered AppChains", value: overview ? formatCount(overview.chains.length) : "—", detail: "Durable registry records", icon: Layers3 },
    { label: "Total anchors", value: overview ? formatCount(overview.totalAnchorCount) : "—", detail: "Owner-signed state anchors", icon: ShieldCheck },
    { label: "Total anchor spend", value: overview ? `${formatUltraAmount(overview.totalAnchorSpendBaseUnits)} $ULTRA` : "—", detail: "Paid by owners as L1 fees", icon: CircleDollarSign },
  ];
  const emptyDescription =
    "A chain appears here once its owner signs an AppChainRegister transaction with the owner key. The node does not create chains, and this surface never infers one from partial history.";

  return (
    <section aria-labelledby="appchain-overview-heading" className="space-y-5">
      <div className="flex flex-col gap-5 border-b border-platinum/10 pb-6 lg:flex-row lg:items-end lg:justify-between">
        <div className="max-w-2xl">
          <p className="font-mono text-[10px] uppercase tracking-[0.26em] text-cyan-glow/70">L3_AppChain_Registry</p>
          <h2 id="appchain-overview-heading" className="mt-2 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum sm:text-4xl">AppChains</h2>
          <p className="mt-4 text-sm leading-7 text-platinum/50">Read the AppChain registry. Every anchor is the chain owner&apos;s own version-5 transaction, signed in the UltraNet mobile wallet; the node holds no AppChain key, so this dashboard cannot anchor on your behalf.</p>
          <p className="mt-3 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum-muted">PROOF_SCHEME // {overview?.proofScheme ?? "Awaiting node contract"}</p>
        </div>
        <div className="flex flex-wrap items-center gap-3">
          {/* The badge states the model rather than echoing the node's availability field, which is now
              constant: there is no server-side anchor path to advertise or hide. */}
          <span className="inline-flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.16em] text-cyan-glow/75" title="Anchors are owner-signed version-5 transactions. The node holds no AppChain key."><span className="h-1.5 w-1.5 rounded-full bg-cyan-glow" aria-hidden="true" />OWNER_SIGNED_ANCHORING</span>
          <button type="button" onClick={onRefresh} disabled={isRefreshing} aria-label="Refresh AppChain registry" title="Refresh AppChain registry" className="inline-flex min-h-10 items-center gap-2 border border-platinum/15 px-3 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-platinum/60 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-50"><RefreshCw className={`h-3.5 w-3.5 ${isRefreshing ? "animate-spin" : ""}`} aria-hidden="true" />REFRESH</button>
          <span title="An AppChain is registered by its owner signing an AppChainRegister transaction. The node does not create chains." className="inline-flex min-h-10 items-center justify-center gap-2 border border-cyan-glow/25 bg-cyan-glow/[0.04] px-4 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-cyan-glow/75">OWNER_SIGNED_REGISTRATION</span>
        </div>
      </div>

      {error && <div className="border border-red-300/30 bg-red-300/10 px-4 py-3 font-mono text-xs leading-5 text-red-200" role="alert" aria-live="assertive">{error}</div>}

      <div className="grid gap-px border border-platinum/10 bg-platinum/10 md:grid-cols-3">
        {cards.map(({ label, value, detail, icon: Icon }) => (
          <div key={label} className="min-w-0 bg-ink-black/80 p-5 transition-colors hover:bg-cyan-glow/[0.03]">
            <div className="flex items-start justify-between gap-3"><span className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum-muted">{label}</span><Icon className="h-4 w-4 shrink-0 text-cyan-glow/60" aria-hidden="true" /></div>
            <p className={`mt-5 break-words font-space-grotesk text-2xl font-bold tracking-tight ${overview ? "text-platinum" : "text-platinum-muted"}`}>{value}</p>
            <p className="mt-1 font-mono text-[10px] leading-relaxed text-platinum-muted">{detail}</p>
          </div>
        ))}
      </div>

      {isLoading && !overview && (
        <div className="border border-platinum/10 bg-ink-black/70 p-8" aria-busy="true" aria-live="polite"><div className="space-y-4 animate-pulse"><div className="h-3 w-32 bg-platinum/10" /><div className="h-12 w-full bg-platinum/5" /><div className="h-12 w-5/6 bg-platinum/5" /></div><p className="mt-6 font-mono text-[10px] uppercase tracking-[0.18em] text-cyan-glow/60">READING_APPCHAIN_REGISTRY</p></div>
      )}

      {!isLoading && !error && !hasChains && (
        <div className="border border-dashed border-cyan-glow/20 bg-cyan-glow/[0.025] px-6 py-14 text-center" role="status"><Layers3 className="mx-auto h-7 w-7 text-cyan-glow/60" aria-hidden="true" /><p className="mt-4 font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">No AppChains are registered yet.</p><p className="mx-auto mt-3 max-w-md text-sm leading-6 text-platinum-muted">{emptyDescription}</p><code className="mx-auto mt-6 block max-w-xl overflow-x-auto border border-platinum/15 bg-ink-black/70 px-4 py-3 text-left font-mono text-[11px] leading-6 text-cyan-glow/80">appchain-cli register --key-file &lt;owner-key&gt; --name &lt;name&gt; --node &lt;api-url&gt;</code><p className="mt-3 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum-muted">Fee is published by the node at /api/appchain/ledger</p></div>
      )}

      {overview && overview.chains.length > 0 && <AppChainTable chains={overview.chains} />}

      <p className="font-mono text-[10px] leading-relaxed text-platinum-muted" aria-live="polite">Unavailable means the connected node did not expose a metric. The dashboard does not infer balances or anchor totals from incomplete history.</p>
    </section>
  );
}
