"use client";

import { LockKeyhole, RefreshCw } from "lucide-react";
import type { AccountSnapshot } from "@/lib/api";
import { formatAddress, formatTimestamp, formatUltraAmount } from "@/lib/transaction-format";
import CopyAddressButton from "@/components/transactions/CopyAddressButton";

type NetworkState = "loading" | "connected" | "delayed" | "unavailable";

interface WalletLedgerProps {
  account: AccountSnapshot | null;
  networkState: NetworkState;
  onRefresh: () => void;
  onLock: () => void;
}

export default function WalletLedger({ account, networkState, onRefresh, onLock }: WalletLedgerProps) {
  const updateLabel = networkState === "loading" ? "Reading balance…" : networkState === "delayed" ? "Balance update delayed" : networkState === "unavailable" ? "Balance unavailable" : account ? `Updated ${formatTimestamp(account.updatedAt)}` : "Reading balance…";

  return (
    <section aria-labelledby="wallet-ledger-title" className="relative z-10 mx-auto max-w-7xl border-y border-platinum/15 px-6 py-7 sm:px-10 lg:px-12">
      <h1 id="wallet-ledger-title" className="sr-only">Wallet balance and address</h1>
      <div className="grid gap-8 lg:grid-cols-[1fr_auto_1fr] lg:items-end lg:gap-10">
        <div>
          <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-platinum/45">Available balance</p>
          <p className="mt-3 font-space-grotesk text-4xl font-bold tabular-nums tracking-tight text-platinum sm:text-5xl">
            {account ? formatUltraAmount(BigInt(account.balance)) : "—"} <span className="font-mono text-base text-cyan-glow sm:text-lg">$ULTRA</span>
          </p>
          <p className={`mt-3 flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] ${networkState === "delayed" || networkState === "unavailable" ? "text-amber-200" : "text-platinum/45"}`}>
            <span className={`h-1.5 w-1.5 ${networkState === "connected" ? "bg-cyan-glow" : "bg-amber-200"}`} aria-hidden="true" />
            {updateLabel}
          </p>
        </div>

        <div className="hidden h-16 w-px bg-platinum/10 lg:block" aria-hidden="true" />

        <div className="min-w-0 lg:justify-self-end lg:text-right">
          <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-platinum/45">Wallet address</p>
          {account ? <p className="mt-3 break-all font-mono text-sm text-platinum/85" title={account.address}><span aria-hidden="true">{formatAddress(account.address)}</span><span className="sr-only">{account.address}</span></p> : <p className="mt-3 font-mono text-sm text-platinum/35">No account loaded</p>}
          {account && <div className="mt-3 lg:flex lg:justify-end"><CopyAddressButton value={account.address} /></div>}
          <p className="mt-3 text-xs text-platinum/40">Use this address to receive ULTRA.</p>
        </div>
      </div>

      <div className="mt-6 flex flex-wrap items-center justify-between gap-4 border-t border-platinum/10 pt-5">
        {networkState === "delayed" || networkState === "unavailable" ? <p className="text-xs leading-6 text-amber-100">Showing the last balance received. Try again when the node is available.</p> : <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/35">Public wallet signing only // private keys stay local</p>}
        <div className="flex flex-wrap gap-3">
          <button type="button" onClick={onRefresh} className="inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/55 underline decoration-platinum/25 underline-offset-4 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"><RefreshCw className="h-3.5 w-3.5" aria-hidden="true" /> Try again</button>
          <button type="button" onClick={onLock} className="inline-flex min-h-11 items-center gap-2 border border-platinum/15 px-4 py-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/55 hover:border-cyan-glow/60 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"><LockKeyhole className="h-3.5 w-3.5" aria-hidden="true" /> Lock wallet</button>
        </div>
      </div>
    </section>
  );
}
