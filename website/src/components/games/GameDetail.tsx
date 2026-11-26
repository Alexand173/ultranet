"use client";

import Link from "next/link";
import { ArrowLeft, Settings2 } from "lucide-react";
import { useEffect, useState } from "react";
import { useGamesAuth } from "@/components/games/GamesSessionProvider";
import { AvailabilityBadge, EmptyState, ErrorState, GameArt, Notice, Panel, Skeleton, StageBadge, WalletGate } from "@/components/games/ui";
import { GamesApiError, fetchDeposits, fetchGame, fetchPlayerResults, fetchPolicies, fetchPool, fetchResult, play } from "@/lib/games-api";
import { bytesToPayloadHex, gameInterfaceFor, payloadHexError } from "@/lib/games-play-adapters";
import { availabilityOf, formatUltra, notRewardedText, payoutStateText, shortId } from "@/lib/games-status";
import type { PlayReport, PolicyReport, StoreGame } from "@/lib/games.types";
import { useRemote } from "@/lib/use-remote";

function newResultId(): string {
  const bytes = crypto.getRandomValues(new Uint8Array(8));
  return `web-${Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

function Fact({ label, value, mono = false }: { label: string; value: React.ReactNode; mono?: boolean }) {
  return (
    <div className="grid gap-1 border-b border-platinum/10 py-3 sm:grid-cols-[14rem_1fr]">
      <dt className="font-mono text-[11px] uppercase tracking-[0.12em] text-platinum-muted">{label}</dt>
      <dd className={`break-all text-sm text-platinum ${mono ? "font-mono" : ""}`}>{value}</dd>
    </div>
  );
}

/** The result of one play, as the service reported it. A reserved reward is shown as reserved, never as paid. */
function ResultCard({ report, onUpdate }: { report: PlayReport; onUpdate: (next: PlayReport) => void }) {
  const [checking, setChecking] = useState(false);
  const [checkError, setCheckError] = useState<string | null>(null);

  useEffect(() => {
    if (report.stage !== "reward_reserved") return;
    // The network, not this page, decides when a payment is final: ask the service again now and then.
    const timer = window.setInterval(() => {
      fetchResult(report.gameId, report.resultId).then(onUpdate).catch(() => undefined);
    }, 15_000);
    return () => window.clearInterval(timer);
  }, [onUpdate, report.gameId, report.resultId, report.stage]);

  async function check() {
    setChecking(true);
    setCheckError(null);
    try {
      onUpdate(await fetchResult(report.gameId, report.resultId));
    } catch (error) {
      setCheckError(error instanceof Error ? error.message : "The status could not be read.");
    } finally {
      setChecking(false);
    }
  }

  const reason = notRewardedText(report.notRewarded);
  return (
    <div data-testid="play-result" className="border border-platinum/15 bg-ink-black/60 p-5" aria-live="polite">
      <StageBadge stage={report.stage} />
      <dl className="mt-4">
        <Fact label="Outcome" value={report.outcome} />
        {report.rewardUnits !== null ? <Fact label="Reward" value={formatUltra(report.rewardUnits)} /> : null}
        {payoutStateText(report.payoutState) ? <Fact label="Payment" value={payoutStateText(report.payoutState)} /> : null}
        <Fact label="Result ID" value={report.resultId} mono />
      </dl>
      {reason ? <p className="mt-3 text-sm text-platinum/85">{reason}</p> : null}
      {report.stage === "reward_reserved" ? (
        <div className="mt-4">
          <p className="text-sm text-platinum/85">The reward is set aside from this game&apos;s pool. It is paid only when UltraNet confirms the transfer.</p>
          <button type="button" onClick={() => void check()} disabled={checking} className="mt-3 min-h-11 border border-cyan-glow px-4 font-mono text-xs font-bold uppercase tracking-[0.12em] text-cyan-glow hover:bg-cyan-glow hover:text-ink-black disabled:opacity-50 focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow">
            {checking ? "Checking…" : "Check payment status"}
          </button>
          {checkError ? <p role="alert" className="mt-2 text-sm text-rose-200">{checkError}</p> : null}
        </div>
      ) : null}
    </div>
  );
}

function PlayPanel({ game }: { game: StoreGame }) {
  const auth = useGamesAuth();
  const Interface = gameInterfaceFor(game.adapterHash);
  const [busy, setBusy] = useState(false);
  const [report, setReport] = useState<PlayReport | null>(null);
  const [failure, setFailure] = useState<string | null>(null);
  const [raw, setRaw] = useState("01");
  const [rawError, setRawError] = useState<string | null>(null);

  async function submit(payloadHex: string) {
    setBusy(true);
    setFailure(null);
    setReport(null);
    try {
      const token = await auth.ensureToken();
      if (!auth.address) throw new Error("Unlock your wallet to play.");
      setReport(await play(token, auth.address, game.gameId, newResultId(), payloadHex));
    } catch (error) {
      if (error instanceof GamesApiError && error.code === "auth_required") {
        auth.dropSession();
        setFailure("Your Games sign-in is no longer valid. Press play again to sign in.");
      } else {
        setFailure(error instanceof Error ? error.message : "The game could not be played.");
      }
    } finally {
      setBusy(false);
    }
  }

  const unlocked = auth.walletState === "unlocked";
  return (
    <Panel title="Play" id="play">
      {!game.playable ? (
        <Notice tone="warning" title="Not available">{availabilityOf(game).reason}</Notice>
      ) : (
        <div className="space-y-5">
          <p className="text-sm text-platinum/80">
            Any wallet can play. You do not need to own this AppChain. Your wallet signs a one-time message to prove it is yours; the
            service then judges your input twice with the game&apos;s approved verifier and tells you what happened.
          </p>
          <WalletGate walletState={auth.walletState} returnTo={`/games/${game.gameId}`} action="play this game" />
          {unlocked ? (
            <div className="space-y-4">
              <p aria-live="polite" className="font-mono text-xs text-platinum-muted">
                {auth.status === "signed-in" ? `Signed in as ${shortId(auth.address, 8, 6)}` : auth.status === "signing" ? "Signing in…" : "Not signed in yet. You will be asked to sign when you play."}
              </p>
              {auth.status === "error" && auth.error ? <Notice tone="error" title="Sign-in failed">{auth.error}</Notice> : null}
              {Interface ? (
                <Interface disabled={busy} onSubmit={(payload) => void submit(bytesToPayloadHex(payload))} />
              ) : (
                <details className="border border-platinum/15 p-4">
                  <summary className="cursor-pointer font-mono text-xs font-bold uppercase tracking-[0.12em] text-platinum">Developer input</summary>
                  <p className="mt-3 text-sm text-platinum-muted">
                    This game has no play interface on this site yet. As a developer you can send the raw input its verifier expects, as hexadecimal. It
                    is judged exactly like any other play.
                  </p>
                  <label htmlFor="raw-payload" className="mt-4 block font-mono text-[11px] uppercase tracking-[0.12em] text-platinum-muted">Raw verifier input (hex)</label>
                  <input
                    id="raw-payload"
                    value={raw}
                    onChange={(event) => { setRaw(event.target.value.trim()); setRawError(null); }}
                    aria-invalid={rawError !== null}
                    aria-describedby={rawError ? "raw-payload-error" : undefined}
                    className="mt-1 min-h-11 w-full border border-platinum/20 bg-ink-black px-3 font-mono text-sm text-platinum focus:border-cyan-glow focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow"
                  />
                  {rawError ? <p id="raw-payload-error" role="alert" className="mt-1 text-sm text-rose-200">{rawError}</p> : null}
                  <button
                    type="button"
                    disabled={busy}
                    onClick={() => {
                      const problem = payloadHexError(raw);
                      setRawError(problem);
                      if (!problem) void submit(raw.toLowerCase());
                    }}
                    className="mt-4 min-h-12 bg-cyan-glow px-8 font-mono text-sm font-black uppercase tracking-[0.12em] text-ink-black hover:shadow-[0_0_24px_rgba(15,255,255,0.4)] disabled:opacity-50 focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black"
                  >
                    {busy ? "Judging…" : "Play now"}
                  </button>
                </details>
              )}
              {busy ? <p role="status" className="text-sm text-platinum/85">Waiting for the service to judge your input…</p> : null}
            </div>
          ) : null}
          {failure ? <Notice tone="error" title="Not played">{failure}</Notice> : null}
          {report ? <ResultCard report={report} onUpdate={setReport} /> : null}
        </div>
      )}
    </Panel>
  );
}

function bestPolicy(policies: PolicyReport[]): { active: PolicyReport | null; pending: PolicyReport | null } {
  const ordered = [...policies].sort((a, b) => b.version - a.version);
  return { active: ordered.find((policy) => policy.status === "approved") ?? null, pending: ordered.find((policy) => policy.status === "proposed") ?? null };
}

function RewardsPanel({ game }: { game: StoreGame }) {
  const policies = useRemote((signal) => fetchPolicies(game.gameId, signal), [game.gameId]);
  const pool = useRemote((signal) => fetchPool(game.gameId, signal), [game.gameId]);
  const deposits = useRemote((signal) => fetchDeposits(game.gameId, signal), [game.gameId]);
  const availability = availabilityOf(game);

  return (
    <Panel title="Rewards" id="rewards">
      <Notice tone={availability.kind === "rewards_available" ? "success" : "info"} title={availability.label}>
        {availability.kind === "rewards_available"
          ? `A reward of ${formatUltra(game.rewardUnits)} is on offer for a winning result, from this game's own pool.`
          : availability.reason}
      </Notice>
      <h3 className="mt-6 text-xs font-bold text-platinum">Reward policy</h3>
      {policies.state.status === "loading" ? <div className="mt-3"><Skeleton lines={2} label="Loading the reward policy" /></div> : null}
      {policies.state.status === "error" ? <div className="mt-3"><ErrorState title="Policy unavailable" message={policies.state.message} onRetry={policies.reload} /></div> : null}
      {policies.state.status === "ready" ? (
        (() => {
          const { active, pending } = bestPolicy(policies.state.data);
          if (!active && !pending) return <p className="mt-3 text-sm text-platinum-muted">No reward policy has been proposed for this game.</p>;
          return (
            <div className="mt-3 space-y-3">
              {active ? (
                <dl>
                  <Fact label="Approved policy" value={`Version ${active.version}`} />
                  <Fact label="Reward per result" value={`${formatUltra(active.rewardUnits)} (${active.eligibleOutcomes})`} />
                  <Fact label="Limits" value={`${active.perPlayerPerDay} per player per day · ${active.perGamePerDay} per game per day`} />
                </dl>
              ) : (
                <p className="text-sm text-platinum-muted">No policy is approved yet, so no reward can be promised.</p>
              )}
              {pending ? <p className="text-sm text-platinum-muted">A newer policy (version {pending.version}) is awaiting platform approval and pays nothing until it is approved.</p> : null}
            </div>
          );
        })()
      ) : null}
      <h3 className="mt-6 text-xs font-bold text-platinum">Reward pool</h3>
      {pool.state.status === "loading" ? <div className="mt-3"><Skeleton lines={3} label="Loading the reward pool" /></div> : null}
      {pool.state.status === "error" ? <div className="mt-3"><ErrorState title="Pool unavailable" message={pool.state.message} onRetry={pool.reload} /></div> : null}
      {pool.state.status === "ready" ? (
        <dl className="mt-3">
          <Fact label="Available for rewards" value={formatUltra(pool.state.data.available)} />
          <Fact label="Funded by the owner" value={formatUltra(pool.state.data.funded)} />
          <Fact label="Reserved, not yet paid" value={formatUltra(pool.state.data.reserved)} />
          <Fact label="Paid and confirmed" value={formatUltra(pool.state.data.paid)} />
        </dl>
      ) : null}
      <p className="mt-3 text-xs text-platinum-muted">These are the pool&apos;s own books. Funds a pool holds are not a promise of earnings for any player.</p>
      {deposits.state.status === "ready" ? (
        <div className="mt-4">
          <h3 className="text-xs font-bold text-platinum">Funding history</h3>
          {deposits.state.data.length === 0 ? (
            <p className="mt-2 text-sm text-platinum-muted">The owner has not funded this pool.</p>
          ) : (
            <ul className="mt-2 space-y-1 text-sm text-platinum/85">
              {deposits.state.data.map((deposit) => (
                <li key={deposit.txHash} className="font-mono text-xs">{formatUltra(deposit.amountUnits)} · transaction {shortId(deposit.txHash, 10, 8)} · {new Date(deposit.claimedAt * 1000).toLocaleDateString("en-GB")}</li>
              ))}
            </ul>
          )}
        </div>
      ) : null}
    </Panel>
  );
}

function ResultsPanel({ game }: { game: StoreGame }) {
  const auth = useGamesAuth();
  const results = useRemote(
    async (signal) => (await fetchPlayerResults(auth.address as string, signal)).filter((row) => row.gameId === game.gameId),
    [auth.address, game.gameId],
    auth.address !== null,
  );
  return (
    <Panel title="Your results in this game" id="results">
      {!auth.address ? (
        <p className="text-sm text-platinum-muted">Connect a wallet to see your own results and rewards here.</p>
      ) : results.state.status === "loading" ? (
        <Skeleton lines={2} label="Loading your results" />
      ) : results.state.status === "error" ? (
        <ErrorState title="Results unavailable" message={results.state.message} onRetry={results.reload} />
      ) : results.state.data.length === 0 ? (
        <EmptyState title="No rewards yet">Results that earn or reserve a reward appear here, first as reserved and then as paid once the network confirms.</EmptyState>
      ) : (
        <ul className="space-y-3">
          {results.state.data.map((row) => (
            <li key={row.resultId} className="border border-platinum/10 p-3">
              <div className="flex flex-wrap items-center justify-between gap-2">
                <span className="font-mono text-sm text-platinum">{formatUltra(row.rewardUnits)}</span>
                <StageBadge stage={row.stage} />
              </div>
              <p className="mt-2 font-mono text-[11px] text-platinum-muted">
                Result {shortId(row.resultId, 12, 4)}
                {row.transactionHash ? ` · transaction ${shortId(row.transactionHash, 10, 8)}` : ""}
              </p>
              {payoutStateText(row.payoutState) ? <p className="mt-1 text-xs text-platinum-muted">{payoutStateText(row.payoutState)}</p> : null}
            </li>
          ))}
        </ul>
      )}
    </Panel>
  );
}

export default function GameDetail({ gameId }: { gameId: string }) {
  const auth = useGamesAuth();
  const { state, reload } = useRemote((signal) => fetchGame(gameId, signal), [gameId]);

  return (
    <div className="mx-auto max-w-7xl px-6 py-10">
      <Link href="/games" className="inline-flex min-h-11 items-center gap-2 text-xs text-platinum-muted hover:text-cyan-glow focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow">
        <ArrowLeft aria-hidden="true" className="h-4 w-4" />
        All games
      </Link>
      {state.status === "loading" ? (
        <div className="mt-6"><Skeleton lines={6} label="Loading the game" /></div>
      ) : state.status === "error" ? (
        <div className="mt-6"><ErrorState title="The game could not be loaded" message={state.message} onRetry={reload} /></div>
      ) : state.data === null ? (
        <div className="mt-6"><EmptyState title="No such game">There is no game with the id {shortId(gameId, 8, 6)} in the catalog. <Link className="text-cyan-glow underline" href="/games">Browse the catalog</Link>.</EmptyState></div>
      ) : (
        <>
          <header className="mt-4 grid gap-6 lg:grid-cols-[22rem_1fr]">
            <GameArt gameId={state.data.gameId} chainId={state.data.chainId} className="h-56 w-full border border-platinum/10 object-cover lg:h-full" />
            <div>
              <div className="flex flex-wrap items-center gap-3">
                <AvailabilityBadge game={state.data} />
                <span className="font-mono text-[11px] uppercase tracking-[0.12em] text-platinum-muted">{state.data.validationScheme === "verified_adapter" ? "Verified results" : "Results recorded only"}</span>
              </div>
              <h1 className="mt-3 text-4xl font-black normal-case tracking-tight text-platinum sm:text-5xl">{state.data.name}</h1>
              <p className="mt-4 max-w-2xl text-platinum/80">{state.data.description || "No description was given for this game."}</p>
              <div className="mt-6 flex flex-wrap gap-3">
                {state.data.playable ? (
                  <a href="#play" className="inline-flex min-h-12 items-center bg-cyan-glow px-8 font-mono text-sm font-black uppercase tracking-[0.12em] text-ink-black hover:shadow-[0_0_24px_rgba(15,255,255,0.4)] focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black">
                    Play now
                  </a>
                ) : (
                  <span className="inline-flex min-h-12 items-center border border-platinum/20 px-6 font-mono text-xs font-bold uppercase tracking-[0.12em] text-platinum-muted">Not playable</span>
                )}
                {auth.address && auth.address === state.data.owner ? (
                  <Link href={`/games/${state.data.gameId}/manage`} className="inline-flex min-h-12 items-center gap-2 border border-cyan-glow px-6 font-mono text-xs font-bold uppercase tracking-[0.12em] text-cyan-glow hover:bg-cyan-glow hover:text-ink-black focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow">
                    <Settings2 aria-hidden="true" className="h-4 w-4" />
                    Manage this game
                  </Link>
                ) : null}
              </div>
            </div>
          </header>

          <div className="mt-10 grid gap-6 lg:grid-cols-[1fr_24rem]">
            <div className="space-y-6">
              <Panel title="About this game" id="about">
                <dl>
                  <Fact label="Game ID" value={state.data.gameId} mono />
                  <Fact label="AppChain ID" value={`#${state.data.chainId}`} mono />
                  <Fact label="AppChain owner (L1 address)" value={state.data.owner ?? "Could not be read from the ledger right now"} mono />
                  <Fact label="Catalog status" value={state.data.status} />
                  <Fact label="Validation" value={state.data.validationScheme === "verified_adapter" ? "A platform-approved verifier judges every result" : "Record only: results are stored, not verified"} />
                  <Fact label="Verifier" value={state.data.adapterHash ?? "None bound"} mono />
                  <Fact label="Availability" value={availabilityOf(state.data).reason ?? "Playable, with a reward on offer"} />
                </dl>
                <p className="mt-3 text-xs text-platinum-muted">The owner shown is the address that owns this game&apos;s AppChain on the ledger now. A game&apos;s identity is its id and AppChain id, never its name.</p>
              </Panel>
              <RewardsPanel game={state.data} />
            </div>
            <div className="space-y-6">
              <PlayPanel game={state.data} />
              <ResultsPanel game={state.data} />
            </div>
          </div>
        </>
      )}
    </div>
  );
}
