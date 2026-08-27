"use client";

import Link from "next/link";
import { Clock3, LogOut, ShieldCheck } from "lucide-react";
import type { AuthSession } from "@/lib/auth";

interface OperatorHeaderProps {
  session: AuthSession;
  isLoggingOut: boolean;
  onLogout: () => void;
}

function truncateNodeIdentifier(identifier: string): string {
  if (identifier.length <= 24) return identifier;
  return `${identifier.slice(0, 12)}…${identifier.slice(-8)}`;
}

function formatExpiry(expiresAt: number): string {
  return new Intl.DateTimeFormat("en-US", {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(expiresAt * 1000));
}

export default function OperatorHeader({ session, isLoggingOut, onLogout }: OperatorHeaderProps) {
  return (
    <header className="border-b border-platinum/10 pb-6">
      <div className="flex flex-col gap-6 lg:flex-row lg:items-end lg:justify-between">
        <div className="space-y-5">
          <div className="flex flex-wrap items-center gap-4">
            <Link
              href="/"
              className="inline-flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.2em] text-platinum/40 transition-colors hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"
            >
              <span aria-hidden="true">←</span> Public_Nexus
            </Link>
            <Link
              href="/appchains"
              className="inline-flex items-center gap-2 font-mono text-[10px] uppercase tracking-[0.2em] text-cyan-glow/70 transition-colors hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"
            >
              AppChain_Registry <span aria-hidden="true">→</span>
            </Link>
          </div>
          <div className="flex items-start gap-4">
            <div className="mt-1 flex h-11 w-11 shrink-0 items-center justify-center border border-cyan-glow/40 bg-cyan-glow/10 text-cyan-glow shadow-[0_0_20px_rgba(15,255,255,0.1)]">
              <ShieldCheck className="h-5 w-5" aria-hidden="true" />
            </div>
            <div>
              <p className="font-mono text-[10px] uppercase tracking-[0.26em] text-cyan-glow/70">
                Authenticated_Operator_Surface
              </p>
              <h1 className="mt-2 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum sm:text-4xl">
                Operator Console
              </h1>
            </div>
          </div>
        </div>

        <div className="flex flex-col items-start gap-3 font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/50 lg:items-end">
          <div className="flex flex-wrap items-center gap-2">
            <span className="text-platinum/30">NODE</span>
            <span className="text-cyan-glow" title={session.nodeIdentifier}>
              {truncateNodeIdentifier(session.nodeIdentifier)}
            </span>
            <span className="flex items-center gap-1.5 text-emerald-300">
              <span className="h-1.5 w-1.5 animate-pulse rounded-full bg-emerald-300" aria-hidden="true" />
              SESSION_VERIFIED
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            <span className="flex items-center gap-1.5">
              <Clock3 className="h-3.5 w-3.5 text-platinum/40" aria-hidden="true" />
              Expires {formatExpiry(session.expiresAt)}
            </span>
            <button
              type="button"
              onClick={onLogout}
              disabled={isLoggingOut}
              className="inline-flex min-h-10 items-center gap-2 border border-platinum/15 px-3 text-platinum/60 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow disabled:cursor-wait disabled:opacity-50"
            >
              <LogOut className="h-3.5 w-3.5" aria-hidden="true" />
              {isLoggingOut ? "LOGGING_OUT" : "LOG_OUT"}
            </button>
          </div>
        </div>
      </div>
    </header>
  );
}
