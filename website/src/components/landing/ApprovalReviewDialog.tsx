"use client";

import { useEffect, useRef, useState } from "react";
import { Check, CircleAlert, Copy, LoaderCircle, ShieldCheck, X } from "lucide-react";
import {
  approveApprovalIntent,
  createApprovalIntent,
  formatPublicKeyFingerprint,
  formatSubmittedAt,
  type ApprovalIntentStatus,
  type ApprovalStage,
  GovernanceRequestError,
  type PendingValidatorProposal,
} from "@/lib/governance";

interface ApprovalReviewDialogProps {
  proposal: PendingValidatorProposal | null;
  onClose: () => void;
  onStatusChange: (status: ApprovalIntentStatus) => void;
}

function statusCopy(stage: ApprovalStage): string {
  switch (stage) {
    case "created":
      return "Approval intent created. Requesting the isolated Sovereign signer.";
    case "signing":
      return "The isolated signer is checking the exact proposal digest.";
    case "awaiting_second_owner":
      return "Your approval is recorded. Awaiting a different Sovereign owner.";
    case "finalizing":
      return "Two signatures are verified. The node is finalizing activation.";
    case "approved":
    case "activated":
      return "Validator proposal approved and activated.";
    default:
      return "Approval request is ready for review.";
  }
}

function errorMessage(error: unknown): string {
  if (error instanceof Error) return error.message;
  return "Unable to request Sovereign approval.";
}

export default function ApprovalReviewDialog({ proposal, onClose, onStatusChange }: ApprovalReviewDialogProps) {
  const dialogRef = useRef<HTMLDivElement>(null);
  const closeButtonRef = useRef<HTMLButtonElement>(null);
  const [copied, setCopied] = useState(false);
  const [intentId, setIntentId] = useState<string | null>(proposal?.intentId ?? null);
  const [status, setStatus] = useState<ApprovalIntentStatus | null>(null);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [error, setError] = useState("");

  useEffect(() => {
    if (!proposal) return;
    setIntentId(proposal.intentId);
    setStatus(null);
    setError("");
    setCopied(false);
  }, [proposal]);

  useEffect(() => {
    if (!proposal) return;
    const previousFocus = document.activeElement as HTMLElement | null;
    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";
    window.setTimeout(() => dialogRef.current?.focus(), 0);

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key === "Escape" && !isSubmitting) {
        event.preventDefault();
        onClose();
      }
      if (event.key !== "Tab" || !dialogRef.current) return;
      const focusable = Array.from(
        dialogRef.current.querySelectorAll<HTMLElement>("button:not([disabled]), [href], [tabindex]:not([tabindex='-1'])"),
      );
      if (focusable.length === 0) return;
      const first = focusable[0];
      const last = focusable[focusable.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    document.addEventListener("keydown", handleKeyDown);
    return () => {
      document.removeEventListener("keydown", handleKeyDown);
      document.body.style.overflow = previousOverflow;
      previousFocus?.focus();
    };
  }, [isSubmitting, onClose, proposal]);

  if (!proposal) return null;

  const copyHash = async () => {
    try {
      await navigator.clipboard.writeText(proposal.proposalHash);
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1800);
    } catch {
      setError("The hash could not be copied. Select the full value manually.");
    }
  };

  const handleApprove = async () => {
    setError("");
    setIsSubmitting(true);
    try {
      const created = intentId
        ? null
        : await createApprovalIntent(proposal.proposalHash, proposal.proposalHash);
      const nextIntentId = intentId ?? created?.intentId;
      if (!nextIntentId) throw new Error("The approval service did not return an intent identifier.");
      setIntentId(nextIntentId);
      const nextStatus = await approveApprovalIntent(nextIntentId);
      setStatus(nextStatus);
      onStatusChange(nextStatus);
    } catch (nextError) {
      const typedError = nextError as GovernanceRequestError;
      setError(errorMessage(typedError));
    } finally {
      setIsSubmitting(false);
    }
  };

  const isFinal = status?.activated || status?.stage === "activated" || status?.stage === "approved";
  const isAwaiting = status?.stage === "awaiting_second_owner";

  return (
    <div className="fixed inset-0 z-[80] flex items-center justify-center bg-ink-black/85 px-4 py-6 backdrop-blur-sm" role="presentation">
      <div
        ref={dialogRef}
        role="dialog"
        aria-modal="true"
        aria-labelledby="approval-review-title"
        aria-describedby="approval-review-description"
        tabIndex={-1}
        className="relative max-h-[calc(100vh-3rem)] w-full max-w-2xl overflow-y-auto border border-cyan-glow/35 bg-[#070d14] p-5 shadow-[0_0_60px_rgba(15,255,255,0.14)] outline-hidden sm:p-8"
      >
        <div className="pointer-events-none absolute right-0 top-0 h-36 w-36 dot-grid opacity-20" aria-hidden="true" />
        <div className="relative">
          <div className="flex items-start justify-between gap-4 border-b border-platinum/10 pb-5">
            <div className="flex items-start gap-3">
              <ShieldCheck className="mt-0.5 h-5 w-5 shrink-0 text-cyan-glow" aria-hidden="true" />
              <div>
                <p className="font-mono text-[10px] uppercase tracking-[0.2em] text-cyan-glow">Sovereign review // validator</p>
                <h2 id="approval-review-title" className="mt-2 font-space-grotesk text-2xl font-bold uppercase tracking-tight text-platinum sm:text-3xl">Confirm proposal hash</h2>
              </div>
            </div>
            <button
              ref={closeButtonRef}
              type="button"
              onClick={onClose}
              disabled={isSubmitting}
              aria-label="Close review"
              title="Close review"
              className="inline-flex h-11 w-11 shrink-0 items-center justify-center border border-platinum/15 text-platinum/55 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-40"
            >
              <X className="h-4 w-4" aria-hidden="true" />
            </button>
          </div>

          <p id="approval-review-description" className="mt-5 text-sm leading-6 text-platinum/60">
            Verify every public field before requesting your Sovereign signature. Approval cannot be undone, and the isolated signer will receive this exact proposal hash.
          </p>

          <dl className="mt-6 grid gap-4 border-y border-platinum/10 py-5 sm:grid-cols-2">
            <div className="min-w-0">
              <dt className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/35">NODE_ALIAS</dt>
              <dd className="mt-2 break-words text-sm font-semibold text-platinum">{proposal.metadata || "Unnamed validator"}</dd>
            </div>
            <div className="min-w-0">
              <dt className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/35">SUBMITTED</dt>
              <dd className="mt-2 text-sm text-platinum/75">{formatSubmittedAt(proposal.submittedAt)}</dd>
            </div>
            <div className="min-w-0">
              <dt className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/35">PROPOSER</dt>
              <dd className="mt-2 break-all font-mono text-xs text-platinum/70">{proposal.proposer}</dd>
            </div>
            <div className="min-w-0">
              <dt className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/35">VALIDATOR_KEY_FINGERPRINT</dt>
              <dd className="mt-2 break-all font-mono text-xs text-platinum/70">{formatPublicKeyFingerprint(proposal.publicKey)}</dd>
            </div>
          </dl>

          <div className="mt-5 border border-cyan-glow/25 bg-cyan-glow/[0.04] p-4 sm:p-5">
            <div className="flex flex-wrap items-center justify-between gap-3">
              <p className="font-mono text-[10px] uppercase tracking-[0.18em] text-cyan-glow">PROPOSAL_HASH // exact 64 hex characters</p>
              <button
                type="button"
                onClick={() => void copyHash()}
                className="inline-flex min-h-11 items-center gap-2 font-mono text-[10px] uppercase tracking-[0.14em] text-cyan-glow transition-colors hover:text-white focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"
              >
                {copied ? <Check className="h-3.5 w-3.5" aria-hidden="true" /> : <Copy className="h-3.5 w-3.5" aria-hidden="true" />}
                {copied ? "HASH_COPIED" : "COPY_HASH"}
              </button>
            </div>
            <code className="mt-4 block break-all font-mono text-xs leading-6 text-platinum/85" title={proposal.proposalHash}>{proposal.proposalHash}</code>
          </div>

          <div className="mt-5 flex flex-wrap items-center gap-x-6 gap-y-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum/45">
            <span>ENVELOPE // VERSION 3</span>
            <span className="text-cyan-glow">SOVEREIGN // 2 OF 3</span>
            <span>RECORDED // {proposal.signedOwnerCount}/{proposal.threshold}</span>
          </div>

          {(status || error) && (
            <div
              className={`mt-5 flex items-start gap-3 border px-4 py-3 font-mono text-xs leading-6 ${error ? "border-red-300/40 bg-red-300/10 text-red-200" : isFinal ? "border-cyan-glow/40 bg-cyan-glow/10 text-cyan-glow" : "border-amber-200/30 bg-amber-200/[0.06] text-amber-100/85"}`}
              role={error ? "alert" : "status"}
              aria-live={error ? "assertive" : "polite"}
            >
              {error ? <CircleAlert className="mt-1 h-4 w-4 shrink-0" aria-hidden="true" /> : isFinal ? <Check className="mt-1 h-4 w-4 shrink-0" aria-hidden="true" /> : <LoaderCircle className={`mt-1 h-4 w-4 shrink-0 ${isSubmitting ? "animate-spin" : ""}`} aria-hidden="true" />}
              <span>{error || (status ? status.message || statusCopy(status.stage) : "")}</span>
            </div>
          )}

          <div className="mt-7 flex flex-col-reverse gap-3 sm:flex-row sm:items-center sm:justify-end">
            <button
              type="button"
              onClick={onClose}
              disabled={isSubmitting}
              className="inline-flex min-h-11 items-center justify-center border border-platinum/15 px-5 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-platinum/55 transition-colors hover:border-platinum/40 hover:text-platinum focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-40"
            >
              {isAwaiting || isFinal ? "CLOSE_REVIEW" : "CANCEL"}
            </button>
            {!isFinal && !isAwaiting && (
              <button
                type="button"
                onClick={() => void handleApprove()}
                disabled={isSubmitting}
                className="inline-flex min-h-11 items-center justify-center gap-2 bg-cyan-glow px-5 font-mono text-[10px] font-black uppercase tracking-[0.14em] text-ink-black transition-colors hover:bg-platinum focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-[#070d14] disabled:cursor-wait disabled:opacity-50"
              >
                {isSubmitting && <LoaderCircle className="h-3.5 w-3.5 animate-spin" aria-hidden="true" />}
                {isSubmitting ? "REQUESTING_SIGNER" : "CONFIRM_HASH_AND_APPROVE"}
              </button>
            )}
          </div>
          <p className="mt-5 border-t border-platinum/10 pt-4 font-mono text-[10px] leading-5 text-platinum/35">
            Private keys never enter this website. The request is signed by the isolated Sovereign signer and independently verified by the UltraNet node.
          </p>
        </div>
      </div>
    </div>
  );
}
