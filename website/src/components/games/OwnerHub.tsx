"use client";

import Link from "next/link";
import { useState } from "react";
import { useGamesAuth } from "@/components/games/GamesSessionProvider";
import { AvailabilityBadge, EmptyState, ErrorState, Notice, Panel, Skeleton, WalletGate } from "@/components/games/ui";
import { fetchAppChainOverview } from "@/lib/appchains";
import { GameApiErrorMessage } from "@/components/games/errors";
import { fetchStore, registerGame } from "@/lib/games-api";
import { GAME_ACTION } from "@/lib/games-signing";
import { shortId } from "@/lib/games-status";
import type { StoreGame } from "@/lib/games.types";
import { useRemote } from "@/lib/use-remote";

const NAME_MAX = 64;

function RegisterForm({ chainId, onRegistered }: { chainId: number; onRegistered: (game: StoreGame) => void }) {
  const auth = useGamesAuth();
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const trimmed = name.trim();

  async function submit(event: React.FormEvent) {
    event.preventDefault();
    if (!trimmed || trimmed.length > NAME_MAX) {
      setError(`Give the game a name of 1 to ${NAME_MAX} characters.`);
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const proof = await auth.signAdmin({ action: GAME_ACTION.register, chainId, gameId: "", name: trimmed, description: description.trim() });
      const game = await registerGame(chainId, trimmed, description.trim(), proof);
      setName("");
      setDescription("");
      onRegistered(game);
    } catch (failure) {
      setError(GameApiErrorMessage(failure, "The game could not be registered."));
    } finally {
      setBusy(false);
    }
  }

  const unlocked = auth.walletState === "unlocked";
  return (
    <form onSubmit={(event) => void submit(event)} className="mt-4 border border-platinum/15 p-4" aria-label={`Register a game on AppChain ${chainId}`}>
      <h3 className="text-xs font-bold text-platinum">Register a game on AppChain #{chainId}</h3>
      <p className="mt-1 text-xs text-platinum-muted">Your wallet signs the registration. The service accepts it only if your address is this chain&apos;s current owner on the ledger.</p>
      <label htmlFor={`name-${chainId}`} className="mt-4 block font-mono text-[11px] uppercase tracking-[0.12em] text-platinum-muted">Game name (a label, not an identity)</label>
      <input id={`name-${chainId}`} value={name} onChange={(event) => setName(event.target.value)} maxLength={NAME_MAX + 8} className="mt-1 min-h-11 w-full border border-platinum/20 bg-ink-black px-3 text-sm text-platinum focus:border-cyan-glow focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow" />
      <label htmlFor={`desc-${chainId}`} className="mt-3 block font-mono text-[11px] uppercase tracking-[0.12em] text-platinum-muted">Description (optional)</label>
      <textarea id={`desc-${chainId}`} value={description} onChange={(event) => setDescription(event.target.value)} rows={3} className="mt-1 w-full border border-platinum/20 bg-ink-black px-3 py-2 text-sm text-platinum focus:border-cyan-glow focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow" />
      {!unlocked ? <div className="mt-4"><WalletGate walletState={auth.walletState} returnTo="/games/manage" action="register a game" /></div> : null}
      <button type="submit" disabled={busy || !unlocked || !trimmed} className="mt-4 min-h-11 bg-cyan-glow px-6 font-mono text-xs font-black uppercase tracking-[0.12em] text-ink-black disabled:opacity-50 focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black">
        {busy ? "Signing and registering…" : "Sign and register"}
      </button>
      {error ? <div className="mt-3"><Notice tone="error" title="Not registered">{error}</Notice></div> : null}
    </form>
  );
}

/** Where an AppChain owner starts: their chains (by L1 owner address), the games on them, and registering more. */
export default function OwnerHub() {
  const auth = useGamesAuth();
  const chains = useRemote((signal) => fetchAppChainOverview(signal), []);
  const games = useRemote((signal) => fetchStore({ signal }), []);
  // Kept here, not in the form: reloading the list replaces the form, and the confirmation must survive that.
  const [justRegistered, setJustRegistered] = useState<StoreGame | null>(null);

  if (!auth.address) {
    return (
      <div className="mx-auto max-w-4xl px-6 py-12">
        <h1 className="text-3xl font-black text-platinum">My games</h1>
        <p className="mt-3 text-platinum/80">AppChain owners register and manage their games here. Ownership is the L1 address that owns the chain on the ledger.</p>
        <div className="mt-6"><WalletGate walletState={auth.walletState} returnTo="/games/manage" action="manage your games" /></div>
      </div>
    );
  }
  const address = auth.address;

  return (
    <div className="mx-auto max-w-4xl px-6 py-12">
      <h1 className="text-3xl font-black text-platinum">My games</h1>
      <p className="mt-3 max-w-2xl text-platinum/80">
        Your AppChains are the ones whose L1 owner address is your wallet&apos;s address. A name never makes a chain yours. Games you register are visible
        immediately in the shared catalog, on this site and in the Android app.
      </p>
      {justRegistered ? (
        <div className="mt-6" data-testid="registered-notice">
          <Notice tone="success" title="Registered">
            The service assigned the game id {shortId(justRegistered.gameId, 8, 6)}. It is in the shared catalog now, so it shows on this site and in the Android app. It can be
            played once the platform installs its verifier, and it earns rewards only after a funded pool and an approved policy.{" "}
            <Link href={`/games/${justRegistered.gameId}`} className="text-cyan-glow underline">Open its page</Link>.
          </Notice>
        </div>
      ) : null}
      <div className="mt-8 space-y-6">
        {chains.state.status === "loading" || games.state.status === "loading" ? <Skeleton lines={4} label="Loading your AppChains" /> : null}
        {chains.state.status === "error" ? <ErrorState title="AppChains could not be loaded" message={chains.state.message} onRetry={chains.reload} /> : null}
        {games.state.status === "error" ? <ErrorState title="Games could not be loaded" message={games.state.message} onRetry={games.reload} /> : null}
        {chains.state.status === "ready" && games.state.status === "ready" ? (
          (() => {
            const owned = chains.state.data.chains.filter((chain) => chain.ownerKind === "address" && chain.owner === address);
            if (owned.length === 0) {
              return (
                <EmptyState title="You own no AppChain">
                  Register an AppChain with the UltraNet wallet app first (the website shows the registry, <Link className="text-cyan-glow underline" href="/appchains">AppChains</Link>), then come back to register games on it.
                </EmptyState>
              );
            }
            return owned.map((chain) => {
              const onChain = games.state.status === "ready" ? games.state.data.filter((game) => game.chainId === chain.id) : [];
              return (
                <Panel key={chain.id} title={`AppChain #${chain.id} · ${chain.name}`} id={`chain-${chain.id}`}>
                  <p className="font-mono text-[11px] text-platinum-muted">Owner {shortId(chain.owner, 10, 8)}</p>
                  {onChain.length === 0 ? (
                    <p className="mt-3 text-sm text-platinum-muted">No game is registered on this chain yet.</p>
                  ) : (
                    <ul className="mt-3 divide-y divide-platinum/10">
                      {onChain.map((game) => (
                        <li key={game.gameId} className="flex flex-wrap items-center justify-between gap-3 py-3">
                          <div>
                            <p className="font-bold normal-case tracking-normal text-platinum">{game.name}</p>
                            <p className="font-mono text-[11px] text-platinum-muted">Game {shortId(game.gameId, 8, 4)}</p>
                          </div>
                          <div className="flex flex-wrap items-center gap-3">
                            <AvailabilityBadge game={game} />
                            <Link href={`/games/${game.gameId}/manage`} className="inline-flex min-h-11 items-center border border-cyan-glow px-4 font-mono text-xs font-bold uppercase tracking-[0.12em] text-cyan-glow hover:bg-cyan-glow hover:text-ink-black focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow">Manage</Link>
                          </div>
                        </li>
                      ))}
                    </ul>
                  )}
                  <RegisterForm chainId={chain.id} onRegistered={(game) => { setJustRegistered(game); games.reload(); }} />
                </Panel>
              );
            });
          })()
        ) : null}
      </div>
    </div>
  );
}
