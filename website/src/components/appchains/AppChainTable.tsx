"use client";

import { Box, CircleDollarSign, Hash, Layers3 } from "lucide-react";
import { formatAddress, formatTimestamp, formatUltraAmount } from "@/lib/transaction-format";
import type { AppChainAction, AppChainSummary } from "@/lib/appchains.types";
import AnchorNowControl from "@/components/appchains/AnchorNowControl";
import AppChainCard from "@/components/appchains/AppChainCard";
import TreasuryAddress from "@/components/appchains/TreasuryAddress";

interface AppChainTableProps {
  chains: AppChainSummary[];
  actions: Record<number, AppChainAction>;
  onArm: (chainId: number) => void;
  onCancel: (chainId: number) => void;
  onConfirm: (chainId: number) => void;
}

function ownerDisplay(chain: AppChainSummary): string {
  return chain.ownerKind === "address" ? formatAddress(chain.owner) : chain.owner;
}

function valueOrUnavailable(value: number | null): string {
  return value === null ? "Unavailable" : new Intl.NumberFormat("en-US").format(value);
}

export default function AppChainTable({ chains, actions, onArm, onCancel, onConfirm }: AppChainTableProps) {
  return (
    <section aria-labelledby="appchain-registry-list-heading" className="border border-platinum/10 bg-ink-black/70">
      <div className="flex flex-wrap items-center justify-between gap-3 border-b border-platinum/10 px-5 py-4 sm:px-6">
        <div className="flex items-center gap-3">
          <Layers3 className="h-4 w-4 text-cyan-glow" aria-hidden="true" />
          <h2 id="appchain-registry-list-heading" className="font-mono text-xs font-bold uppercase tracking-[0.18em] text-platinum/70">Registered_AppChains</h2>
        </div>
        <span className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/30">{chains.length} RECORDS</span>
      </div>

      <div className="hidden overflow-x-auto lg:block">
        <table className="w-full min-w-[1040px] border-collapse text-left">
          <caption className="sr-only">Registered UltraNet AppChains and their anchor accounting</caption>
          <thead className="border-b border-platinum/10 bg-platinum/[0.02]">
            <tr className="font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/35">
              <th scope="col" className="px-5 py-4 font-normal">AppChain</th>
              <th scope="col" className="px-4 py-4 font-normal">Owner</th>
              <th scope="col" className="px-4 py-4 font-normal">L1 Treasury</th>
              <th scope="col" className="px-4 py-4 font-normal">ID</th>
              <th scope="col" className="px-4 py-4 font-normal">Anchors</th>
              <th scope="col" className="px-4 py-4 font-normal">Balance</th>
              <th scope="col" className="px-4 py-4 font-normal">Anchor spend</th>
              <th scope="col" className="px-5 py-4 text-right font-normal">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-platinum/5">
            {chains.map((chain) => {
              const action = actions[chain.id] ?? { state: "idle", error: "", result: null };
              return (
                <tr key={chain.id} className="align-top transition-colors hover:bg-cyan-glow/[0.025]">
                  <th scope="row" className="max-w-52 px-5 py-5 font-normal">
                    <div className="flex items-start gap-3">
                      <div className="mt-0.5 flex h-8 w-8 shrink-0 items-center justify-center border border-cyan-glow/20 bg-cyan-glow/[0.04] text-cyan-glow/75"><Box className="h-4 w-4" aria-hidden="true" /></div>
                      <div className="min-w-0">
                        <p className="truncate font-space-grotesk text-sm font-bold uppercase tracking-tight text-platinum" title={chain.name}>{chain.name}</p>
                        <p className="mt-1 truncate font-mono text-[10px] text-platinum/30" title={chain.latestStateRoot ?? undefined}>{chain.latestAnchorAt === null ? "NEVER_ANCHORED" : `LAST ${formatTimestamp(chain.latestAnchorAt)}`}</p>
                      </div>
                    </div>
                  </th>
                  <td className="max-w-48 px-4 py-5 font-mono text-xs text-platinum/65"><span className="block truncate" title={chain.owner}>{ownerDisplay(chain)}</span></td>
                  <td className="max-w-48 px-4 py-5"><TreasuryAddress address={chain.accountAddress} compact /></td>
                  <td className="px-4 py-5"><span className="inline-flex items-center gap-1.5 font-mono text-xs text-cyan-glow/75"><Hash className="h-3.5 w-3.5" aria-hidden="true" />{chain.id}</span></td>
                  <td className="px-4 py-5"><span className="inline-flex items-center gap-1.5 font-mono text-xs tabular-nums text-platinum/75"><Layers3 className="h-3.5 w-3.5 text-cyan-glow/50" aria-hidden="true" />{valueOrUnavailable(chain.anchorCount)}</span></td>
                  <td className="whitespace-nowrap px-4 py-5"><span className="inline-flex items-center gap-1.5 font-mono text-xs tabular-nums text-platinum/75"><CircleDollarSign className="h-3.5 w-3.5 text-cyan-glow/50" aria-hidden="true" />{formatUltraAmount(chain.balanceBaseUnits)} <span className="text-[10px] text-cyan-glow">$ULTRA</span></span></td>
                  <td className="whitespace-nowrap px-4 py-5 font-mono text-xs tabular-nums text-platinum/55">{formatUltraAmount(chain.anchorSpendBaseUnits)} $ULTRA</td>
                  <td className="px-5 py-4 text-right"><div className="flex justify-end"><AnchorNowControl chainId={chain.id} availability={chain.anchorAvailability} anchorFeeBaseUnits={chain.anchorFeeBaseUnits} balanceBaseUnits={chain.balanceBaseUnits} controlId={`desktop-${chain.id}`} action={action} onArm={() => onArm(chain.id)} onCancel={() => onCancel(chain.id)} onConfirm={() => onConfirm(chain.id)} /></div></td>
                </tr>
              );
            })}
          </tbody>
        </table>
      </div>

      <div className="grid gap-4 p-4 lg:hidden">
        {chains.map((chain) => (
          <AppChainCard
            key={chain.id}
            chain={chain}
            action={actions[chain.id] ?? { state: "idle", error: "", result: null }}
            onArm={() => onArm(chain.id)}
            onCancel={() => onCancel(chain.id)}
            onConfirm={() => onConfirm(chain.id)}
          />
        ))}
      </div>
    </section>
  );
}
