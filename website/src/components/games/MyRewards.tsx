"use client";

import Link from "next/link";
import { useGamesAuth } from "@/components/games/GamesSessionProvider";
import { EmptyState, ErrorState, Notice, Skeleton, StageBadge, WalletGate } from "@/components/games/ui";
import { fetchPlayerResults } from "@/lib/games-api";
import { formatUltra, payoutStateText, shortId } from "@/lib/games-status";
import { useRemote } from "@/lib/use-remote";

/** A wallet's rewards across every game, as the service reports them. Only the network-confirmed stage reads Paid. */
export default function MyRewards() {
  const auth = useGamesAuth();
  const results = useRemote((signal) => fetchPlayerResults(auth.address as string, signal), [auth.address], auth.address !== null);
  return (
    <div className="mx-auto max-w-3xl px-6 py-12">
      <h1 className="text-3xl font-black text-platinum">My rewards</h1>
      <p className="mt-3 text-platinum/80">Every reward your wallet has earned or had reserved, in any game. The same list appears in the Android app.</p>
      <div className="mt-6">
        <Notice title="How to read this">
          <strong>Reserved</strong> means the game&apos;s pool has set the reward aside. It becomes <strong>Paid</strong> only when UltraNet confirms the payment.
        </Notice>
      </div>
      <div className="mt-8">
        {!auth.address ? (
          <WalletGate walletState={auth.walletState} returnTo="/games/rewards" action="see your rewards" />
        ) : results.state.status === "loading" ? (
          <Skeleton lines={4} label="Loading your rewards" />
        ) : results.state.status === "error" ? (
          <ErrorState title="Rewards could not be loaded" message={results.state.message} onRetry={results.reload} />
        ) : results.state.data.length === 0 ? (
          <EmptyState title="No rewards yet">Play a game with a reward on offer. <Link className="text-cyan-glow underline" href="/games">Browse games</Link>.</EmptyState>
        ) : (
          <ul className="space-y-3">
            {results.state.data.map((row) => (
              <li key={`${row.gameId}-${row.resultId}`} className="border border-platinum/10 bg-platinum/[0.03] p-4">
                <div className="flex flex-wrap items-center justify-between gap-2">
                  <span className="font-mono text-lg text-platinum">{formatUltra(row.rewardUnits)}</span>
                  <StageBadge stage={row.stage} />
                </div>
                <p className="mt-2 text-sm text-platinum-muted">
                  <Link className="text-cyan-glow underline" href={`/games/${row.gameId}`}>Game {shortId(row.gameId, 8, 4)}</Link> · result {shortId(row.resultId, 12, 4)} ·{" "}
                  {new Date(row.at * 1000).toLocaleString("en-GB")}
                </p>
                {payoutStateText(row.payoutState) ? <p className="mt-1 text-xs text-platinum-muted">{payoutStateText(row.payoutState)}</p> : null}
                {row.transactionHash ? <p className="mt-1 font-mono text-[11px] text-platinum-muted">Transaction {shortId(row.transactionHash, 12, 8)}</p> : null}
              </li>
            ))}
          </ul>
        )}
      </div>
    </div>
  );
}
