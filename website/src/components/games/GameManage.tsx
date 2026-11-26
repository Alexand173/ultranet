"use client";

import Link from "next/link";
import { ArrowLeft } from "lucide-react";
import { useState } from "react";
import { GameApiErrorMessage } from "@/components/games/errors";
import { useGamesAuth } from "@/components/games/GamesSessionProvider";
import { AvailabilityBadge, EmptyState, ErrorState, Notice, Panel, Skeleton, WalletGate } from "@/components/games/ui";
import { claimDeposit, fetchDeposits, fetchGame, fetchPolicies, fetchPool, fetchPoolPayouts, proposePolicy, withdrawFromPool } from "@/lib/games-api";
import { GAME_ACTION, policySummary } from "@/lib/games-signing";
import { formatUltra, parseUltraUnits, payoutStateText, shortId } from "@/lib/games-status";
import type { StoreGame } from "@/lib/games.types";
import { useRemote } from "@/lib/use-remote";

function Row({ label, value }: { label: string; value: string }) {
  return (
    <div className="flex flex-wrap justify-between gap-2 border-b border-platinum/10 py-2 text-sm">
      <dt className="text-platinum-muted">{label}</dt>
      <dd className="font-mono text-platinum">{value}</dd>
    </div>
  );
}

function Field({ id, label, children, hint }: { id: string; label: string; children: React.ReactNode; hint?: string }) {
  return (
    <div className="mt-3">
      <label htmlFor={id} className="block font-mono text-[11px] uppercase tracking-[0.12em] text-platinum-muted">{label}</label>
      {children}
      {hint ? <p className="mt-1 text-xs text-platinum-muted">{hint}</p> : null}
    </div>
  );
}

const INPUT = "mt-1 min-h-11 w-full border border-platinum/20 bg-ink-black px-3 font-mono text-sm text-platinum focus:border-cyan-glow focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow";
const SUBMIT = "mt-4 min-h-11 bg-cyan-glow px-6 font-mono text-xs font-black uppercase tracking-[0.12em] text-ink-black disabled:opacity-50 focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black";

type Outcome = { tone: "success" | "error"; text: string } | null;

function Actions({ game, onChanged }: { game: StoreGame; onChanged: () => void }) {
  const auth = useGamesAuth();
  const [busy, setBusy] = useState<string | null>(null);
  const [outcome, setOutcome] = useState<Outcome>(null);
  const [txHash, setTxHash] = useState("");
  const [reward, setReward] = useState("");
  const [outcomes, setOutcomes] = useState("win");
  const [perPlayer, setPerPlayer] = useState("3");
  const [perGame, setPerGame] = useState("100");
  const [amount, setAmount] = useState("");

  async function run(name: string, work: () => Promise<string>) {
    setBusy(name);
    setOutcome(null);
    try {
      setOutcome({ tone: "success", text: await work() });
      onChanged();
    } catch (failure) {
      setOutcome({ tone: "error", text: GameApiErrorMessage(failure, "That did not go through.") });
    } finally {
      setBusy(null);
    }
  }

  const unlocked = auth.walletState === "unlocked";
  const rewardUnits = parseUltraUnits(reward);
  const withdrawUnits = parseUltraUnits(amount);
  const players = Number(perPlayer);
  const games = Number(perGame);
  const policyReady = rewardUnits !== null && Number.isInteger(players) && players > 0 && Number.isInteger(games) && games > 0;

  return (
    <Panel title="Owner actions" id="actions">
      <p className="text-sm text-platinum/80">
        Each action is signed by your wallet and checked by the service against the current L1 owner of AppChain #{game.chainId}. Nothing here can be done for you by anyone else.
      </p>
      {!unlocked ? <div className="mt-4"><WalletGate walletState={auth.walletState} returnTo={`/games/${game.gameId}/manage`} action="sign owner actions" /></div> : null}
      {outcome ? <div className="mt-4"><Notice tone={outcome.tone} title={outcome.tone === "success" ? "Done" : "Not done"}>{outcome.text}</Notice></div> : null}

      <form className="mt-6" onSubmit={(event) => { event.preventDefault(); void run("fund", async () => {
        const proof = await auth.signAdmin({ action: GAME_ACTION.fund, chainId: game.chainId, gameId: game.gameId, name: txHash, description: "" });
        const pool = await claimDeposit(game.gameId, txHash, proof);
        setTxHash("");
        return `Deposit claimed. The pool now holds ${formatUltra(pool.funded)} in total.`;
      }); }}>
        <h3 className="text-xs font-bold text-platinum">Claim a funding deposit</h3>
        <p className="mt-1 text-xs text-platinum-muted">Send ULTRA from your wallet to the payout address first, wait for the network to confirm it, then claim that transaction for this game. One transfer funds one game, once. UltraNet funds no game.</p>
        <Field id="fund-tx" label="Deposit transaction hash"><input id="fund-tx" value={txHash} onChange={(event) => setTxHash(event.target.value.trim().toLowerCase())} placeholder="64 hexadecimal characters" className={INPUT} /></Field>
        <button type="submit" disabled={busy !== null || !unlocked || !/^[0-9a-f]{64}$/.test(txHash)} className={SUBMIT}>{busy === "fund" ? "Signing…" : "Sign and claim deposit"}</button>
      </form>

      <form className="mt-8 border-t border-platinum/10 pt-6" onSubmit={(event) => { event.preventDefault(); if (!policyReady || rewardUnits === null) return; void run("policy", async () => {
        const summary = policySummary(rewardUnits, outcomes, players, games, 0);
        const proof = await auth.signAdmin({ action: GAME_ACTION.policyPropose, chainId: game.chainId, gameId: game.gameId, name: summary, description: "" });
        const policy = await proposePolicy(game.gameId, { rewardUnits, outcomes, perPlayerPerDay: players, perGamePerDay: games }, proof);
        return `Policy version ${policy.version} proposed. It pays nothing until the platform approves it, and approval requires a funded pool.`;
      }); }}>
        <h3 className="text-xs font-bold text-platinum">Propose a reward policy</h3>
        <p className="mt-1 text-xs text-platinum-muted">You propose; the UltraNet platform decides. Approval never funds a pool, and a policy can only pay what the pool actually holds.</p>
        <Field id="policy-reward" label="Reward per result (ULTRA)" hint="Up to six decimals, for example 0.0001."><input id="policy-reward" value={reward} onChange={(event) => setReward(event.target.value)} inputMode="decimal" className={INPUT} /></Field>
        <Field id="policy-outcomes" label="Outcomes that earn a reward">
          <select id="policy-outcomes" value={outcomes} onChange={(event) => setOutcomes(event.target.value)} className={INPUT}>
            <option value="win">Win</option>
            <option value="tie">Tie</option>
            <option value="win,tie">Win or tie</option>
          </select>
        </Field>
        <div className="grid gap-3 sm:grid-cols-2">
          <Field id="policy-player" label="Per player, per day"><input id="policy-player" value={perPlayer} onChange={(event) => setPerPlayer(event.target.value.replace(/\D/g, ""))} inputMode="numeric" className={INPUT} /></Field>
          <Field id="policy-game" label="Per game, per day"><input id="policy-game" value={perGame} onChange={(event) => setPerGame(event.target.value.replace(/\D/g, ""))} inputMode="numeric" className={INPUT} /></Field>
        </div>
        <button type="submit" disabled={busy !== null || !unlocked || !policyReady} className={SUBMIT}>{busy === "policy" ? "Signing…" : "Sign and propose policy"}</button>
      </form>

      <form className="mt-8 border-t border-platinum/10 pt-6" onSubmit={(event) => { event.preventDefault(); if (withdrawUnits === null) return; void run("withdraw", async () => {
        const proof = await auth.signAdmin({ action: GAME_ACTION.withdraw, chainId: game.chainId, gameId: game.gameId, name: String(withdrawUnits), description: "" });
        const pool = await withdrawFromPool(game.gameId, withdrawUnits, proof);
        setAmount("");
        return `Withdrawal of ${formatUltra(withdrawUnits)} queued. It is paid to the chain's current owner after the network confirms it, with its network fee taken from that amount. ${formatUltra(pool.available)} remains available in the pool.`;
      }); }}>
        <h3 className="text-xs font-bold text-platinum">Withdraw from the pool</h3>
        <p className="mt-1 text-xs text-platinum-muted">Only unreserved funds can be withdrawn, and only to the AppChain&apos;s current owner address. It is not paid until the network confirms it.</p>
        <Field id="withdraw-amount" label="Amount (ULTRA)"><input id="withdraw-amount" value={amount} onChange={(event) => setAmount(event.target.value)} inputMode="decimal" className={INPUT} /></Field>
        <button type="submit" disabled={busy !== null || !unlocked || withdrawUnits === null} className={SUBMIT}>{busy === "withdraw" ? "Signing…" : "Sign and withdraw"}</button>
      </form>
    </Panel>
  );
}

function PoolOverview({ game, version }: { game: StoreGame; version: number }) {
  const pool = useRemote((signal) => fetchPool(game.gameId, signal), [game.gameId, version]);
  const deposits = useRemote((signal) => fetchDeposits(game.gameId, signal), [game.gameId, version]);
  const policies = useRemote((signal) => fetchPolicies(game.gameId, signal), [game.gameId, version]);
  const payouts = useRemote((signal) => fetchPoolPayouts(game.gameId, signal), [game.gameId, version]);
  return (
    <>
      <Panel title="Reward pool" id="pool">
        {pool.state.status === "loading" ? <Skeleton lines={4} label="Loading the pool" /> : null}
        {pool.state.status === "error" ? <ErrorState title="Pool unavailable" message={pool.state.message} onRetry={pool.reload} /> : null}
        {pool.state.status === "ready" ? (
          <dl>
            <Row label="Available to reward or withdraw" value={formatUltra(pool.state.data.available)} />
            <Row label="Funded" value={formatUltra(pool.state.data.funded)} />
            <Row label="Reserved for payouts" value={formatUltra(pool.state.data.reserved)} />
            <Row label="Paid to players (confirmed)" value={formatUltra(pool.state.data.paid)} />
            <Row label="Network fees held / paid" value={`${formatUltra(pool.state.data.feesReserved)} / ${formatUltra(pool.state.data.feesPaid)}`} />
            <Row label="Withdrawn" value={formatUltra(pool.state.data.withdrawn)} />
          </dl>
        ) : null}
      </Panel>
      <Panel title="Funding history" id="funding">
        {deposits.state.status === "loading" ? <Skeleton lines={2} label="Loading the funding history" /> : null}
        {deposits.state.status === "error" ? <ErrorState title="History unavailable" message={deposits.state.message} onRetry={deposits.reload} /> : null}
        {deposits.state.status === "ready" ? (
          deposits.state.data.length === 0 ? <p className="text-sm text-platinum-muted">No deposits claimed yet.</p> : (
            <ul className="space-y-1 font-mono text-xs text-platinum/85">
              {deposits.state.data.map((deposit) => <li key={deposit.txHash}>{formatUltra(deposit.amountUnits)} · {shortId(deposit.txHash, 10, 8)} · {new Date(deposit.claimedAt * 1000).toLocaleDateString("en-GB")}</li>)}
            </ul>
          )
        ) : null}
      </Panel>
      <Panel title="Reward policies" id="policies">
        {policies.state.status === "ready" ? (
          policies.state.data.length === 0 ? <p className="text-sm text-platinum-muted">No policy proposed yet.</p> : (
            <ul className="space-y-2 text-sm text-platinum/85">
              {policies.state.data.map((policy) => (
                <li key={policy.version}>
                  Version {policy.version}: {formatUltra(policy.rewardUnits)} per {policy.eligibleOutcomes} · {policy.perPlayerPerDay} per player/day · {policy.perGamePerDay} per game/day ·{" "}
                  <strong className="font-bold">{policy.status === "approved" ? "approved by the platform" : policy.status === "proposed" ? "awaiting platform approval" : policy.status}</strong>
                  {policy.decisionNote ? ` (${policy.decisionNote})` : ""}
                </li>
              ))}
            </ul>
          )
        ) : policies.state.status === "error" ? <ErrorState title="Policies unavailable" message={policies.state.message} onRetry={policies.reload} /> : <Skeleton lines={2} label="Loading policies" />}
      </Panel>
      <Panel title="Payouts" id="payouts">
        {payouts.state.status === "ready" ? (
          payouts.state.data.length === 0 ? <p className="text-sm text-platinum-muted">No payouts yet.</p> : (
            <ul className="space-y-1 text-xs text-platinum/85">
              {payouts.state.data.slice(0, 20).map((payout, index) => (
                <li key={`${payout.toAddress}-${index}`} className="font-mono">
                  {payout.kind === "withdrawal" ? "Withdrawal" : "Reward"} {formatUltra(payout.amountUnits)} to {shortId(payout.toAddress, 8, 6)} · {payoutStateText(payout.state)}
                  {payout.transactionHash ? ` · ${shortId(payout.transactionHash, 8, 6)}` : ""}
                </li>
              ))}
            </ul>
          )
        ) : payouts.state.status === "error" ? <ErrorState title="Payouts unavailable" message={payouts.state.message} onRetry={payouts.reload} /> : <Skeleton lines={2} label="Loading payouts" />}
      </Panel>
    </>
  );
}

export default function GameManage({ gameId }: { gameId: string }) {
  const auth = useGamesAuth();
  const [version, setVersion] = useState(0);
  const { state, reload } = useRemote((signal) => fetchGame(gameId, signal), [gameId]);

  return (
    <div className="mx-auto max-w-4xl px-6 py-10">
      <Link href="/games/manage" className="inline-flex min-h-11 items-center gap-2 text-xs text-platinum-muted hover:text-cyan-glow focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow">
        <ArrowLeft aria-hidden="true" className="h-4 w-4" />
        My games
      </Link>
      {state.status === "loading" ? <div className="mt-6"><Skeleton lines={5} label="Loading the game" /></div> : null}
      {state.status === "error" ? <div className="mt-6"><ErrorState title="The game could not be loaded" message={state.message} onRetry={reload} /></div> : null}
      {state.status === "ready" && state.data === null ? <div className="mt-6"><EmptyState title="No such game">No game in the catalog has that id.</EmptyState></div> : null}
      {state.status === "ready" && state.data !== null ? (
        <>
          <div className="mt-4 flex flex-wrap items-center gap-3">
            <h1 className="text-3xl font-black normal-case tracking-tight text-platinum">{state.data.name}</h1>
            <AvailabilityBadge game={state.data} />
          </div>
          <p className="mt-2 font-mono text-[11px] text-platinum-muted">Game {state.data.gameId} · AppChain #{state.data.chainId} · owner {shortId(state.data.owner, 10, 8)}</p>
          <p className="mt-2 text-sm text-platinum/80"><Link className="text-cyan-glow underline" href={`/games/${state.data.gameId}`}>View the public page</Link></p>
          {!auth.address || auth.address !== state.data.owner ? (
            <div className="mt-6">
              <Notice tone="warning" title="You are not this chain's owner">
                {auth.address
                  ? "This wallet is not the L1 owner of this game's AppChain, so the owner actions are not offered. The figures below are public."
                  : "Connect the wallet that owns this game's AppChain to use the owner actions. The figures below are public."}
              </Notice>
            </div>
          ) : null}
          <div className="mt-6 space-y-6">
            <PoolOverview game={state.data} version={version} />
            {auth.address && auth.address === state.data.owner ? <Actions game={state.data} onChanged={() => setVersion((value) => value + 1)} /> : null}
          </div>
        </>
      ) : null}
    </div>
  );
}
