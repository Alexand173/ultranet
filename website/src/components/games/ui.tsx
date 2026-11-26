"use client";

import Link from "next/link";
import { AlertTriangle, Ban, CheckCircle2, CircleDashed, Clock3, Coins, FileText, Inbox, Lock, RotateCcw, Wallet } from "lucide-react";
import { useId, type ReactNode } from "react";
import { getSendUltraHref } from "@/lib/links";
import { availabilityOf, stageLabel, type AvailabilityKind } from "@/lib/games-status";
import type { StoreGame } from "@/lib/games.types";

const KIND_STYLE: Record<AvailabilityKind, { cls: string; Icon: typeof Coins }> = {
  rewards_available: { cls: "border-cyan-glow/60 bg-cyan-glow/10 text-cyan-glow", Icon: Coins },
  rewards_unavailable: { cls: "border-platinum/30 bg-platinum/5 text-platinum", Icon: CircleDashed },
  record_only: { cls: "border-amber-300/50 bg-amber-300/10 text-amber-200", Icon: FileText },
  unavailable: { cls: "border-rose-300/50 bg-rose-300/10 text-rose-200", Icon: Ban },
};

/** The availability badge: text and an icon, never colour alone. */
export function AvailabilityBadge({ game }: { game: StoreGame }) {
  const availability = availabilityOf(game);
  const { cls, Icon } = KIND_STYLE[availability.kind];
  return (
    <span className={`inline-flex items-center gap-1.5 border px-2.5 py-1 font-mono text-[11px] font-bold uppercase tracking-[0.12em] ${cls}`}>
      <Icon aria-hidden="true" className="h-3.5 w-3.5" />
      {availability.label}
    </span>
  );
}

const STAGE_STYLE: Record<string, { cls: string; Icon: typeof Coins }> = {
  reward_paid: { cls: "border-cyan-glow/70 bg-cyan-glow/10 text-cyan-glow", Icon: CheckCircle2 },
  reward_reserved: { cls: "border-amber-300/50 bg-amber-300/10 text-amber-200", Icon: Clock3 },
  reward_released: { cls: "border-rose-300/50 bg-rose-300/10 text-rose-200", Icon: RotateCcw },
  verified: { cls: "border-platinum/30 bg-platinum/5 text-platinum", Icon: CheckCircle2 },
};

/** A result's stage. Only the network-confirmed stage reads "Paid"; everything else says what it is not. */
export function StageBadge({ stage }: { stage: string }) {
  const { cls, Icon } = STAGE_STYLE[stage] ?? { cls: "border-platinum/30 text-platinum", Icon: AlertTriangle };
  return (
    <span className={`inline-flex items-center gap-1.5 border px-2.5 py-1 font-mono text-[11px] font-bold uppercase tracking-[0.1em] ${cls}`}>
      <Icon aria-hidden="true" className="h-3.5 w-3.5" />
      {stageLabel(stage)}
    </span>
  );
}

/**
 * Artwork derived from the game's id. The catalog has no artwork field, so the picture is generated from the id alone:
 * the same game looks the same on every device, and it says nothing about the game that the id does not.
 */
export function GameArt({ gameId, chainId, className = "" }: { gameId: string; chainId: number; className?: string }) {
  const seed = Array.from({ length: 16 }, (_, index) => parseInt(gameId.slice(index * 2, index * 2 + 2) || "0", 16) || 0);
  const hue = (seed[0] * 360) / 256;
  const hue2 = (hue + 40 + (seed[1] % 90)) % 360;
  const uid = useId().replace(/:/g, "");
  const circles = [2, 5, 8, 11].map((offset, index) => ({
    cx: 20 + (seed[offset] % 60),
    cy: 20 + (seed[offset + 1] % 60),
    r: 14 + (seed[offset + 2] % 22),
    o: 0.18 + index * 0.07,
  }));
  const bars = Array.from({ length: 7 }, (_, index) => 8 + (seed[index + 4] % 40));
  return (
    <svg role="img" aria-label={`Generated artwork for game ${gameId.slice(0, 8)} on AppChain ${chainId}`} viewBox="0 0 100 100" preserveAspectRatio="xMidYMid slice" className={className}>
      <defs>
        <linearGradient id={`${uid}g`} x1="0" y1="0" x2="1" y2="1">
          <stop offset="0" stopColor={`hsl(${hue} 85% 22%)`} />
          <stop offset="1" stopColor={`hsl(${hue2} 90% 12%)`} />
        </linearGradient>
      </defs>
      <rect width="100" height="100" fill={`url(#${uid}g)`} />
      {circles.map((circle, index) => (
        <circle key={index} cx={circle.cx} cy={circle.cy} r={circle.r} fill={`hsl(${(hue + index * 35) % 360} 95% 60%)`} opacity={circle.o} />
      ))}
      {bars.map((height, index) => (
        <rect key={index} x={6 + index * 13} y={100 - height} width="7" height={height} fill="rgba(255,255,255,0.08)" />
      ))}
      <path d="M0 62 H100 M0 78 H100 M31 0 V100 M69 0 V100" stroke="rgba(255,255,255,0.07)" strokeWidth="0.4" />
      <text x="6" y="14" fill="rgba(255,255,255,0.7)" fontSize="6.5" fontFamily="monospace" letterSpacing="0.6">{`CHAIN #${chainId}`}</text>
    </svg>
  );
}

export function Panel({ title, children, action, id }: { title?: string; children: ReactNode; action?: ReactNode; id?: string }) {
  return (
    <section id={id} aria-labelledby={title && id ? `${id}-title` : undefined} className="border border-platinum/10 bg-platinum/[0.03] p-5 sm:p-6">
      {title ? (
        <div className="mb-4 flex items-center justify-between gap-3">
          <h2 id={id ? `${id}-title` : undefined} className="text-xs font-bold text-platinum">{title}</h2>
          {action}
        </div>
      ) : null}
      {children}
    </section>
  );
}

export function Skeleton({ lines = 3, label = "Loading" }: { lines?: number; label?: string }) {
  return (
    <div role="status" aria-live="polite" aria-busy="true">
      <span className="sr-only">{label}</span>
      <div aria-hidden="true" className="space-y-3">
        {Array.from({ length: lines }, (_, index) => (
          <div key={index} className="h-4 bg-platinum/10 motion-safe:animate-pulse" style={{ width: `${92 - index * 14}%` }} />
        ))}
      </div>
    </div>
  );
}

export function ErrorState({ title = "Could not load", message, onRetry }: { title?: string; message: string; onRetry?: () => void }) {
  return (
    <div role="alert" className="border border-rose-300/40 bg-rose-300/5 p-5">
      <p className="flex items-center gap-2 font-mono text-xs font-bold uppercase tracking-[0.12em] text-rose-200">
        <AlertTriangle aria-hidden="true" className="h-4 w-4" />
        {title}
      </p>
      <p className="mt-2 text-sm text-platinum/85">{message}</p>
      {onRetry ? (
        <button type="button" onClick={onRetry} className="mt-4 min-h-11 border border-cyan-glow px-4 font-mono text-xs font-bold uppercase tracking-[0.12em] text-cyan-glow hover:bg-cyan-glow hover:text-ink-black focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow">
          Try again
        </button>
      ) : null}
    </div>
  );
}

export function EmptyState({ title, children }: { title: string; children: ReactNode }) {
  return (
    <div className="border border-dashed border-platinum/20 p-8 text-center">
      <Inbox aria-hidden="true" className="mx-auto h-8 w-8 text-platinum-muted" />
      <p className="mt-3 font-mono text-xs font-bold uppercase tracking-[0.12em] text-platinum">{title}</p>
      <div className="mt-2 text-sm text-platinum-muted">{children}</div>
    </div>
  );
}

export function Notice({ tone = "info", title, children }: { tone?: "info" | "success" | "warning" | "error"; title?: string; children: ReactNode }) {
  const cls = {
    info: "border-cyan-glow/30 bg-cyan-glow/5",
    success: "border-cyan-glow/60 bg-cyan-glow/10",
    warning: "border-amber-300/40 bg-amber-300/5",
    error: "border-rose-300/40 bg-rose-300/5",
  }[tone];
  return (
    <div role={tone === "error" ? "alert" : "status"} className={`border p-4 text-sm text-platinum/90 ${cls}`}>
      {title ? <p className="mb-1 font-mono text-xs font-bold uppercase tracking-[0.12em] text-platinum">{title}</p> : null}
      {children}
    </div>
  );
}

/** What a visitor must do to act with a wallet. Browsing never needs one; this appears only at the action. */
export function WalletGate({ walletState, returnTo, action }: { walletState: "checking" | "none" | "locked" | "unlocked"; returnTo: string; action: string }) {
  if (walletState === "unlocked") return null;
  const href = getSendUltraHref(returnTo);
  const message =
    walletState === "checking"
      ? "Checking for a wallet on this device…"
      : walletState === "none"
        ? `Create or restore an UltraNet wallet to ${action}. You can browse every game without one.`
        : `Your wallet is locked. Unlock it to ${action}.`;
  return (
    <div className="border border-platinum/15 bg-ink-black/60 p-4">
      <p className="flex items-center gap-2 text-sm text-platinum/90">
        {walletState === "locked" ? <Lock aria-hidden="true" className="h-4 w-4 text-cyan-glow" /> : <Wallet aria-hidden="true" className="h-4 w-4 text-cyan-glow" />}
        {message}
      </p>
      {walletState === "checking" ? null : (
        <Link href={href} className="mt-3 inline-flex min-h-11 items-center border border-cyan-glow px-4 font-mono text-xs font-bold uppercase tracking-[0.12em] text-cyan-glow hover:bg-cyan-glow hover:text-ink-black focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow">
          {walletState === "none" ? "Set up wallet" : "Unlock wallet"}
        </Link>
      )}
      <p className="mt-3 text-xs text-platinum-muted">Games never asks for your recovery phrase. Signing happens in this browser and only a signature leaves it.</p>
    </div>
  );
}
