"use client";

import { Box, CircleDollarSign, Hash, Layers3 } from "lucide-react";
import { formatAddress, formatTimestamp, formatUltraAmount } from "@/lib/transaction-format";
import type { AppChainAction, AppChainSummary } from "@/lib/appchains.types";
import AnchorNowControl from "@/components/appchains/AnchorNowControl";
import TreasuryAddress from "@/components/appchains/TreasuryAddress";

interface AppChainCardProps {
  chain: AppChainSummary;
  action: AppChainAction;
  onArm: () => void;
  onCancel: () => void;
  onConfirm: () => void;
}

function valueOrUnavailable(value: string | number | null): string {
  return value === null || value === "" ? "Unavailable" : String(value);
}

export default function AppChainCard({ chain, action, onArm, onCancel, onConfirm }: AppChainCardProps) {
  const ownerDisplay = chain.ownerKind === "address" ? formatAddress(chain.owner) : chain.owner;

  return (
    <article className="border border-platinum/10 bg-ink-black/75 p-5" aria-labelledby={`appchain-card-${chain.id}`}>
      <div className="flex items-start justify-between gap-4 border-b border-platinum/10 pb-4">
        <div className="min-w-0">
          <p className="font-mono text-[10px] uppercase tracking-[0.16em] text-cyan-glow/70">APPCHAIN // L3</p>
          <h3 id={`appchain-card-${chain.id}`} className="mt-2 truncate font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum" title={chain.name}>
            {chain.name}
          </h3>
        </div>
        <span className="shrink-0 border border-cyan-glow/20 px-2 py-1 font-mono text-[10px] text-cyan-glow/75">ID_{chain.id}</span>
      </div>

      <dl className="grid gap-x-5 gap-y-4 py-5 sm:grid-cols-2">
        <div className="min-w-0">
          <dt className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/35"><Box className="h-3.5 w-3.5 text-cyan-glow/60" aria-hidden="true" /> Owner</dt>
          <dd className="mt-2 truncate font-mono text-xs text-platinum/75" title={chain.owner}>{ownerDisplay}</dd>
        </div>
        <div className="min-w-0">
          <dt className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/35"><Hash className="h-3.5 w-3.5 text-cyan-glow/60" aria-hidden="true" /> L1 Treasury</dt>
          <dd className="mt-2"><TreasuryAddress address={chain.accountAddress} compact /></dd>
        </div>
        <div>
          <dt className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/35"><Layers3 className="h-3.5 w-3.5 text-cyan-glow/60" aria-hidden="true" /> Anchors</dt>
          <dd className="mt-2 font-space-grotesk text-lg font-bold text-platinum">{valueOrUnavailable(chain.anchorCount)}</dd>
        </div>
        <div>
          <dt className="flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/35"><CircleDollarSign className="h-3.5 w-3.5 text-cyan-glow/60" aria-hidden="true" /> Balance</dt>
          <dd className="mt-2 font-space-grotesk text-lg font-bold text-platinum">{formatUltraAmount(chain.balanceBaseUnits)} <span className="font-mono text-[10px] text-cyan-glow">$ULTRA</span></dd>
        </div>
        <div>
          <dt className="font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/35">Anchor spend</dt>
          <dd className="mt-2 font-mono text-xs text-platinum/75">{formatUltraAmount(chain.anchorSpendBaseUnits)} $ULTRA</dd>
        </div>
        <div>
          <dt className="font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/35">Latest anchor</dt>
          <dd className="mt-2 font-mono text-xs text-platinum/75">{chain.latestAnchorAt === null ? "Never anchored" : formatTimestamp(chain.latestAnchorAt)}</dd>
        </div>
      </dl>

      <div className="border-t border-platinum/10 pt-4">
        <AnchorNowControl
          chainId={chain.id}
          availability={chain.anchorAvailability}
          anchorFeeBaseUnits={chain.anchorFeeBaseUnits}
          balanceBaseUnits={chain.balanceBaseUnits}
          controlId={`mobile-${chain.id}`}
          action={action}
          onArm={onArm}
          onCancel={onCancel}
          onConfirm={onConfirm}
        />
      </div>
    </article>
  );
}
