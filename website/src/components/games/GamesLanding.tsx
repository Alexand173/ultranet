"use client";

import Link from "next/link";
import { ArrowDown, Search, ShieldCheck, Sparkles } from "lucide-react";
import { useMemo, useState } from "react";
import { useGamesAuth } from "@/components/games/GamesSessionProvider";
import { AvailabilityBadge, EmptyState, ErrorState, GameArt } from "@/components/games/ui";
import { fetchStore } from "@/lib/games-api";
import { GAME_FILTERS, availabilityOf, featuredGames, filterGames, formatUltra, shortId, type GameFilter } from "@/lib/games-status";
import type { StoreGame } from "@/lib/games.types";
import { useRemote } from "@/lib/use-remote";

function GameCard({ game, featured = false }: { game: StoreGame; featured?: boolean }) {
  const availability = availabilityOf(game);
  return (
    <article className={`group relative flex w-full flex-col overflow-hidden border border-platinum/10 bg-platinum/[0.03] transition-colors hover:border-cyan-glow/50 focus-within:border-cyan-glow ${featured ? "lg:flex-row" : ""}`}>
      <GameArt gameId={game.gameId} chainId={game.chainId} className={`${featured ? "h-48 w-full shrink-0 lg:h-60 lg:w-2/5" : "h-40 w-full"} object-cover`} />
      <div className="flex flex-1 flex-col gap-3 p-5">
        <div className="flex flex-wrap items-start justify-between gap-2">
          <h3 className="text-lg font-bold normal-case tracking-normal text-platinum">
            <Link href={`/games/${game.gameId}`} className="after:absolute after:inset-0 focus:outline-none focus-visible:after:ring-2 focus-visible:after:ring-cyan-glow">
              {game.name}
            </Link>
          </h3>
          <AvailabilityBadge game={game} />
        </div>
        <p className="line-clamp-3 text-sm text-platinum/75">{game.description || "No description was given for this game."}</p>
        <p className="text-xs text-platinum-muted">
          {availability.kind === "rewards_available"
            ? `Reward on offer: ${formatUltra(game.rewardUnits)} per winning result, paid from this game's own pool.`
            : availability.reason}
        </p>
        <dl className="mt-auto grid grid-cols-2 gap-x-4 gap-y-1 border-t border-platinum/10 pt-3 font-mono text-[11px] text-platinum-muted">
          <div>
            <dt className="uppercase tracking-[0.12em]">AppChain</dt>
            <dd className="text-platinum">#{game.chainId}</dd>
          </div>
          <div>
            <dt className="uppercase tracking-[0.12em]">Game ID</dt>
            <dd className="text-platinum" title={game.gameId}>{shortId(game.gameId, 8, 4)}</dd>
          </div>
        </dl>
      </div>
    </article>
  );
}

function CardSkeletons() {
  return (
    <div className="grid gap-5 sm:grid-cols-2 xl:grid-cols-3" aria-hidden="true">
      {[0, 1, 2].map((index) => (
        <div key={index} className="h-72 border border-platinum/10 bg-platinum/[0.03] motion-safe:animate-pulse" />
      ))}
    </div>
  );
}

export default function GamesLanding() {
  const { address } = useGamesAuth();
  const { state, reload } = useRemote((signal) => fetchStore({ signal }), []);
  const [query, setQuery] = useState("");
  const [filter, setFilter] = useState<GameFilter>("all");
  const games = useMemo(() => (state.status === "ready" ? state.data : []), [state]);
  const featured = useMemo(() => featuredGames(games), [games]);
  const shown = useMemo(() => filterGames(games, query, filter, address), [games, query, filter, address]);

  return (
    <>
      <section aria-labelledby="games-hero" className="relative isolate overflow-hidden border-b border-platinum/10">
        <div aria-hidden="true" className="pointer-events-none absolute inset-0 -z-10 bg-[radial-gradient(60%_80%_at_20%_0%,rgba(15,255,255,0.16),transparent),radial-gradient(50%_70%_at_90%_30%,rgba(109,40,217,0.22),transparent)]" />
        <div aria-hidden="true" className="pointer-events-none absolute inset-0 -z-10 opacity-[0.07] [background-image:linear-gradient(rgba(255,255,255,1)_1px,transparent_1px),linear-gradient(90deg,rgba(255,255,255,1)_1px,transparent_1px)] [background-size:48px_48px]" />
        <div className="mx-auto max-w-7xl px-6 py-20 sm:py-28">
          <p className="font-mono text-xs font-bold uppercase tracking-[0.3em] text-cyan-glow">Games on UltraNet AppChains</p>
          <h1 id="games-hero" className="mt-4 text-5xl font-black leading-[0.95] tracking-tight text-platinum sm:text-7xl">
            ULTRANET <span className="text-cyan-glow">GAMES</span>
          </h1>
          <p className="mt-5 text-2xl font-bold normal-case tracking-normal text-platinum sm:text-3xl">Play. Compete. Earn ULTRA.</p>
          <p className="mt-5 max-w-2xl text-base text-platinum/80 sm:text-lg">
            Every game here runs on an UltraNet AppChain, owned by the wallet that registered the chain. Results are judged by a
            verifier the platform has approved, and rewards come only from the pool that game&apos;s own owner has funded.
          </p>
          <div className="mt-8 flex flex-wrap items-center gap-4">
            <a href="#catalog" className="inline-flex min-h-12 items-center gap-2 bg-cyan-glow px-7 font-mono text-sm font-black uppercase tracking-[0.12em] text-ink-black hover:shadow-[0_0_30px_rgba(15,255,255,0.45)] focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black">
              Explore games
              <ArrowDown aria-hidden="true" className="h-4 w-4" />
            </a>
            <Link href="/games/manage" className="inline-flex min-h-12 items-center border border-platinum/30 px-6 font-mono text-xs font-bold uppercase tracking-[0.12em] text-platinum hover:border-cyan-glow hover:text-cyan-glow focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow">
              Register a game
            </Link>
          </div>
          <p className="mt-8 flex max-w-2xl items-start gap-2 text-xs text-platinum-muted">
            <ShieldCheck aria-hidden="true" className="mt-0.5 h-4 w-4 shrink-0 text-cyan-glow" />
            A game promises a reward only when it has an approved policy and its pool holds enough ULTRA. A reserved reward is not a paid
            reward: a result reads &quot;Paid&quot; only after UltraNet confirms the payment.
          </p>
        </div>
      </section>

      <div className="mx-auto max-w-7xl px-6 py-14">
        {featured.length > 0 ? (
          <section aria-labelledby="featured-title" className="mb-16">
            <h2 id="featured-title" className="flex items-center gap-2 text-sm font-bold text-platinum">
              <Sparkles aria-hidden="true" className="h-4 w-4 text-cyan-glow" />
              Featured: rewards on offer now
            </h2>
            <p className="mt-1 text-xs text-platinum-muted">Games that are playable and have a reward available right now. This is not a ranking.</p>
            <div className="mt-5 grid gap-5">
              {featured.map((game) => <GameCard key={game.gameId} game={game} featured />)}
            </div>
          </section>
        ) : null}

        <section id="catalog" aria-labelledby="catalog-title" className="scroll-mt-24">
          <div className="flex flex-wrap items-end justify-between gap-4">
            <div>
              <h2 id="catalog-title" className="text-xl font-bold text-platinum">Game catalog</h2>
              <p aria-live="polite" className="mt-1 text-xs text-platinum-muted">
                {state.status === "ready" ? `${shown.length} of ${games.length} games` : "Reading the catalog"}
              </p>
            </div>
            <div className="relative w-full sm:w-80">
              <label htmlFor="game-search" className="sr-only">Search games by name or description</label>
              <Search aria-hidden="true" className="pointer-events-none absolute left-3 top-1/2 h-4 w-4 -translate-y-1/2 text-platinum-muted" />
              <input
                id="game-search"
                type="search"
                value={query}
                onChange={(event) => setQuery(event.target.value)}
                placeholder="Search name, description or id"
                className="min-h-11 w-full border border-platinum/20 bg-ink-black py-2 pl-10 pr-3 text-sm text-platinum placeholder:text-platinum-muted focus:border-cyan-glow focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow"
              />
            </div>
          </div>

          <div role="group" aria-label="Filter games" className="mt-5 flex flex-wrap gap-2">
            {GAME_FILTERS.map((option) => (
              <button
                key={option.id}
                type="button"
                aria-pressed={filter === option.id}
                onClick={() => setFilter(option.id)}
                className={`min-h-11 border px-4 font-mono text-xs font-bold uppercase tracking-[0.1em] focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow ${
                  filter === option.id ? "border-cyan-glow bg-cyan-glow/10 text-cyan-glow" : "border-platinum/20 text-platinum/75 hover:border-platinum/50"
                }`}
              >
                {option.label}
              </button>
            ))}
          </div>

          <div className="mt-8">
            {state.status === "loading" ? (
              <>
                <span className="sr-only" role="status">Loading games</span>
                <CardSkeletons />
              </>
            ) : state.status === "error" ? (
              <ErrorState title="The catalog could not be loaded" message={state.message} onRetry={reload} />
            ) : games.length === 0 ? (
              <EmptyState title="No games registered yet">
                An AppChain owner can register the first game from <Link className="text-cyan-glow underline" href="/games/manage">My games</Link>. It will appear here and in the
                Android app as soon as it is registered.
              </EmptyState>
            ) : shown.length === 0 ? (
              <EmptyState title="No game matches">
                Try a different search or filter.
                {filter === "mine" && !address ? " “On my AppChains” needs a wallet on this device." : null}
              </EmptyState>
            ) : (
              <ul className="grid gap-5 sm:grid-cols-2 xl:grid-cols-3">
                {shown.map((game) => (
                  <li key={game.gameId} className="flex">
                    <div className="flex w-full"><GameCard game={game} /></div>
                  </li>
                ))}
              </ul>
            )}
          </div>
        </section>
      </div>
    </>
  );
}
