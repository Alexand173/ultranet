"use client";

import Link from "next/link";
import { useCallback, useEffect, useRef, useState } from "react";
import { Check, CircleAlert, Clipboard, LoaderCircle, LockKeyhole, RefreshCw, ShieldCheck } from "lucide-react";
import ApprovalReviewDialog from "@/components/landing/ApprovalReviewDialog";
import {
  fetchValidatorReview,
  formatApprovalProgress,
  formatHashFingerprint,
  formatPublicKeyFingerprint,
  formatSubmittedAt,
  GovernanceRequestError,
  type ApprovalIntentStatus,
  type PendingValidatorProposal,
  type ValidatorReviewResponse,
} from "@/lib/governance";
import { getSendUltraHref, VALIDATOR_PROPOSAL_PATH } from "@/lib/links";

const REFRESH_INTERVAL_MS = 15_000;
const ACTIVE_INTENT_POLL_MS = 5_000;

type LoadState = "checking" | "ready" | "unauthorized" | "forbidden" | "unavailable";

function AccessGate({ state, onRetry }: { state: Exclude<LoadState, "checking" | "ready">; onRetry: () => void }) {
  const isForbidden = state === "forbidden";
  const isUnavailable = state === "unavailable";
  return (
    <section aria-labelledby="pending-approvals-title" className="border border-platinum/10 bg-platinum/[0.025] p-5 sm:p-7">
      <div className="flex items-start gap-3">
        {isForbidden ? <LockKeyhole className="mt-0.5 h-5 w-5 shrink-0 text-amber-200" aria-hidden="true" /> : <ShieldCheck className="mt-0.5 h-5 w-5 shrink-0 text-cyan-glow" aria-hidden="true" />}
        <div className="min-w-0">
          <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-cyan-glow">Validator-only governance review</p>
          <h2 id="pending-approvals-title" className="mt-2 font-space-grotesk text-2xl font-bold uppercase tracking-tight text-platinum sm:text-3xl">
            Pending approvals
          </h2>
          <p className="mt-3 max-w-2xl text-sm leading-6 text-platinum/55">
            {isForbidden
              ? "Your validator session can review network state, but it is not an authorized Sovereign owner. Approval controls remain hidden."
              : isUnavailable
                ? "The node could not confirm the validator approval service. No proposal approval was requested."
                : "Authenticate with an authorized validator wallet before viewing proposal details."}
          </p>
          <div className="mt-5 flex flex-wrap gap-4 font-mono text-[10px] uppercase tracking-[0.14em]">
            {!isForbidden && !isUnavailable && (
              <Link href={getSendUltraHref(VALIDATOR_PROPOSAL_PATH)} className="inline-flex min-h-11 items-center gap-2 text-cyan-glow hover:text-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow">
                Open validator login <span aria-hidden="true">→</span>
              </Link>
            )}
            {isUnavailable && (
              <button type="button" onClick={onRetry} className="inline-flex min-h-11 items-center gap-2 border border-platinum/15 px-4 text-platinum/60 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow">
                <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" /> Retry check
              </button>
            )}
          </div>
        </div>
      </div>
    </section>
  );
}

function CheckingState() {
  return (
    <section aria-labelledby="pending-approvals-title" aria-busy="true" className="border border-cyan-glow/20 bg-cyan-glow/[0.025] p-5 sm:p-7">
      <div className="flex items-center gap-3 font-mono text-xs uppercase tracking-[0.18em] text-cyan-glow" role="status" aria-live="polite">
        <LoaderCircle className="h-4 w-4 animate-spin" aria-hidden="true" />
        Checking validator authorization
      </div>
    </section>
  );
}

function ProposalRow({
  proposal,
  canApprove,
  onReview,
  onCopy,
}: {
  proposal: PendingValidatorProposal;
  canApprove: boolean;
  onReview: () => void;
  onCopy: () => void;
}) {
  const isCurrentOwnerSigned = proposal.currentOwnerSigned;
  const isAwaiting = proposal.signedOwnerCount >= proposal.threshold || isCurrentOwnerSigned;
  return (
    <li className="grid min-w-0 gap-5 border-t border-platinum/10 px-4 py-5 first:border-t-0 sm:px-5 lg:grid-cols-[minmax(0,1.15fr)_minmax(0,1.45fr)_minmax(130px,0.45fr)_auto] lg:items-center lg:gap-6">
      <div className="min-w-0">
        <p className="truncate font-space-grotesk text-lg font-bold uppercase tracking-tight text-platinum" title={proposal.metadata}>{proposal.metadata || "Unnamed validator"}</p>
        <p className="mt-2 font-mono text-[10px] uppercase tracking-[0.12em] text-platinum/35">Submitted {formatSubmittedAt(proposal.submittedAt)}</p>
        <p className="mt-2 break-all font-mono text-[10px] text-platinum/45" title={proposal.proposer}>PROPOSER // {formatHashFingerprint(proposal.proposer)}</p>
      </div>
      <div className="min-w-0">
        <div className="flex flex-wrap items-center justify-between gap-2">
          <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-cyan-glow/70">PROPOSAL_HASH</p>
          <button type="button" onClick={onCopy} title="Copy proposal hash" className="inline-flex min-h-10 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.12em] text-platinum/50 transition-colors hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow">
            <Clipboard className="h-3.5 w-3.5" aria-hidden="true" /> Copy
          </button>
        </div>
        <code className="mt-2 block break-all font-mono text-xs leading-5 text-platinum/75" title={proposal.proposalHash}>{proposal.proposalHash}</code>
        <p className="mt-2 break-all font-mono text-[10px] text-platinum/35" title={proposal.publicKey}>VALIDATOR_KEY // {formatPublicKeyFingerprint(proposal.publicKey)}</p>
      </div>
      <div className="min-w-0">
        <p className="font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/35">SIGNED_OWNERS</p>
        <p className="mt-2 font-space-grotesk text-2xl font-bold text-cyan-glow">{formatApprovalProgress(proposal.signedOwnerCount, proposal.threshold)}</p>
        <p className="mt-1 font-mono text-[10px] uppercase tracking-[0.12em] text-platinum/40">of {proposal.totalOwners} configured</p>
      </div>
      <div className="flex min-w-0 flex-col items-start gap-3 lg:items-end">
        {isCurrentOwnerSigned ? (
          <span className="inline-flex min-h-10 items-center gap-2 border border-cyan-glow/25 bg-cyan-glow/[0.05] px-3 font-mono text-[10px] uppercase tracking-[0.1em] text-cyan-glow/75">
            <Check className="h-3.5 w-3.5" aria-hidden="true" /> Your approval recorded
          </span>
        ) : isAwaiting ? (
          <span className="inline-flex min-h-10 items-center border border-amber-200/25 bg-amber-200/[0.04] px-3 font-mono text-[10px] uppercase tracking-[0.1em] text-amber-100/75">
            Awaiting finalization
          </span>
        ) : canApprove ? (
          <button type="button" onClick={onReview} className="inline-flex min-h-11 items-center justify-center border border-cyan-glow/60 bg-cyan-glow px-4 font-mono text-[10px] font-black uppercase tracking-[0.14em] text-ink-black transition-colors hover:bg-platinum focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black">
            Review &amp; approve
          </button>
        ) : (
          <span className="inline-flex min-h-10 items-center border border-platinum/10 px-3 font-mono text-[10px] uppercase tracking-[0.1em] text-platinum/35">
            Owner authorization required
          </span>
        )}
      </div>
    </li>
  );
}

export default function PendingApprovalsPanel() {
  const [loadState, setLoadState] = useState<LoadState>("checking");
  const [review, setReview] = useState<ValidatorReviewResponse | null>(null);
  const [error, setError] = useState("");
  const [selectedProposal, setSelectedProposal] = useState<PendingValidatorProposal | null>(null);
  const [notice, setNotice] = useState("");
  const [isRefreshing, setIsRefreshing] = useState(false);
  const controllerRef = useRef<AbortController | null>(null);
  const mountedRef = useRef(true);

  const loadReview = useCallback(async () => {
    controllerRef.current?.abort();
    const controller = new AbortController();
    controllerRef.current = controller;
    setIsRefreshing(true);
    setError("");
    try {
      const nextReview = await fetchValidatorReview(controller.signal);
      if (!mountedRef.current || controller.signal.aborted) return;
      setReview(nextReview);
      setLoadState("ready");
    } catch (nextError) {
      if (!mountedRef.current || controller.signal.aborted) return;
      const status = nextError instanceof GovernanceRequestError ? nextError.status : 0;
      setLoadState(status === 401 ? "unauthorized" : status === 403 ? "forbidden" : "unavailable");
      setError(nextError instanceof Error ? nextError.message : "Unable to load pending validator approvals.");
    } finally {
      if (mountedRef.current && !controller.signal.aborted) setIsRefreshing(false);
    }
  }, []);

  useEffect(() => {
    mountedRef.current = true;
    void loadReview();
    const interval = window.setInterval(() => void loadReview(), REFRESH_INTERVAL_MS);
    return () => {
      mountedRef.current = false;
      controllerRef.current?.abort();
      window.clearInterval(interval);
    };
  }, [loadReview]);

  useEffect(() => {
    if (!review || !review.proposals.some((proposal) => proposal.intentId && proposal.signedOwnerCount === 1)) return;
    const interval = window.setInterval(() => void loadReview(), ACTIVE_INTENT_POLL_MS);
    return () => window.clearInterval(interval);
  }, [loadReview, review]);

  const canApprove = review?.capabilities.capabilities.includes("sovereign_approve") ?? false;

  const handleCopy = async (hash: string) => {
    try {
      await navigator.clipboard.writeText(hash);
      setNotice("Proposal hash copied.");
      window.setTimeout(() => setNotice(""), 1800);
    } catch {
      setNotice("Copy failed. Select the full hash manually.");
    }
  };

  const handleStatusChange = (status: ApprovalIntentStatus) => {
    setNotice(status.activated ? "Validator activated." : status.message);
    window.setTimeout(() => setNotice(""), 5000);
    void loadReview();
  };

  if (loadState === "checking") return <CheckingState />;
  if (loadState !== "ready") return <AccessGate state={loadState} onRetry={() => void loadReview()} />;
  if (!review) return <CheckingState />;

  return (
    <section id="pending-approvals" aria-labelledby="pending-approvals-title" className="relative z-10 border border-cyan-glow/20 bg-ink-black/45">
      <div className="pointer-events-none absolute right-0 top-0 h-32 w-32 dot-grid opacity-15" aria-hidden="true" />
      <div className="relative flex flex-col gap-5 border-b border-platinum/10 px-5 py-5 sm:flex-row sm:items-end sm:justify-between sm:px-7">
        <div>
          <div className="flex items-center gap-3 text-cyan-glow"><ShieldCheck className="h-5 w-5" aria-hidden="true" /><p className="font-mono text-[10px] uppercase tracking-[0.2em]">Validator-only governance review</p></div>
          <h2 id="pending-approvals-title" className="mt-3 font-space-grotesk text-2xl font-bold uppercase tracking-tight text-platinum sm:text-3xl">Pending approvals</h2>
          <p className="mt-2 max-w-2xl text-sm leading-6 text-platinum/50">Review the exact proposal hash before requesting an isolated Sovereign signature. The node still requires two distinct owners.</p>
        </div>
        <div className="flex flex-wrap items-center gap-4 font-mono text-[10px] uppercase tracking-[0.14em]">
          <span className={canApprove ? "text-cyan-glow" : "text-amber-200/75"}>{canApprove ? "OWNER_AUTHORITY // READY" : "REVIEW_ONLY_SESSION"}</span>
          <span className="text-platinum/35">THRESHOLD // 2 OF 3</span>
          <button type="button" onClick={() => void loadReview()} disabled={isRefreshing} title="Refresh pending approvals" className="inline-flex min-h-10 items-center gap-2 border border-platinum/15 px-3 text-platinum/50 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-40">
            <RefreshCw className={`h-3.5 w-3.5 ${isRefreshing ? "animate-spin" : ""}`} aria-hidden="true" /> Refresh
          </button>
        </div>
      </div>

      {notice && <div role="status" aria-live="polite" className="border-b border-cyan-glow/20 bg-cyan-glow/[0.05] px-5 py-3 font-mono text-xs text-cyan-glow sm:px-7">{notice}</div>}
      {error && <div role="alert" aria-live="assertive" className="flex items-start gap-3 border-b border-red-300/30 bg-red-300/[0.06] px-5 py-3 font-mono text-xs leading-5 text-red-200 sm:px-7"><CircleAlert className="mt-0.5 h-4 w-4 shrink-0" aria-hidden="true" />{error}</div>}

      {review.proposals.length === 0 ? (
        <div className="px-5 py-12 text-center sm:px-7"><Check className="mx-auto h-6 w-6 text-cyan-glow/60" aria-hidden="true" /><p className="mt-4 font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">No pending proposals</p><p className="mx-auto mt-2 max-w-md text-sm leading-6 text-platinum/45">The validator queue is clear. New signed applications will appear here after they enter governance.</p></div>
      ) : (
        <ul aria-label="Pending validator proposals">
          {review.proposals.map((proposal) => (
            <ProposalRow
              key={proposal.proposalHash}
              proposal={proposal}
              canApprove={canApprove}
              onReview={() => setSelectedProposal(proposal)}
              onCopy={() => void handleCopy(proposal.proposalHash)}
            />
          ))}
        </ul>
      )}

      <div className="flex flex-col gap-2 border-t border-platinum/10 px-5 py-4 font-mono text-[10px] uppercase tracking-[0.12em] text-platinum/30 sm:flex-row sm:items-center sm:justify-between sm:px-7">
        <span>Private keys never enter this dashboard.</span>
        <span>Final authority // UltraNet node verifier</span>
      </div>
      <ApprovalReviewDialog proposal={selectedProposal} onClose={() => setSelectedProposal(null)} onStatusChange={handleStatusChange} />
    </section>
  );
}
