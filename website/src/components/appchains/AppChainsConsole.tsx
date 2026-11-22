"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { AlertTriangle, LoaderCircle, RotateCcw } from "lucide-react";
import { fetchAuthSession, logout, type AuthSession } from "@/lib/auth";
import { AppChainRequestError, fetchAppChainOverview } from "@/lib/appchains";
import type { AppChainOverview } from "@/lib/appchains.types";
import AppChainOverviewView from "@/components/appchains/AppChainOverview";
import OperatorHeader from "@/components/operator/OperatorHeader";

export type AppChainsSessionState = "checking" | "authenticated" | "unauthenticated" | "expired" | "unavailable";

function LoginLink() {
  return <Link href="/login?next=%2Fappchains" className="mt-6 inline-flex min-h-11 items-center justify-center border border-cyan-glow/60 px-5 font-mono text-xs font-bold uppercase tracking-[0.16em] text-cyan-glow transition-colors hover:bg-cyan-glow hover:text-ink-black focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow">RETURN_TO_LOGIN</Link>;
}

function AccessState({ state, onRetry }: { state: Exclude<AppChainsSessionState, "authenticated" | "checking">; onRetry: () => void }) {
  const isExpired = state === "expired";
  const title = isExpired ? "SESSION_EXPIRED" : state === "unavailable" ? "AUTH_SERVICE_UNAVAILABLE" : "AUTHENTICATION_REQUIRED";
  const message = isExpired ? "Your operator session is no longer valid. Authenticate again before changing AppChain state." : state === "unavailable" ? "The node could not confirm the operator session. Retry when the API is reachable." : "This surface is not public. Continue through the authenticated operator login flow.";
  return <main id="main-content" tabIndex={-1} className="relative flex min-h-screen items-center justify-center overflow-hidden bg-ink-black px-4 py-10 dot-grid sm:px-8"><div className="pointer-events-none fixed inset-0 bg-[radial-gradient(circle_at_50%_45%,rgba(15,255,255,0.08),transparent_58%)]" /><section className="relative w-full max-w-lg border border-red-300/30 bg-ink-black/90 p-6 text-center shadow-[0_0_40px_rgba(15,255,255,0.08)] sm:p-10" aria-labelledby="appchains-access-heading"><AlertTriangle className="mx-auto h-8 w-8 text-red-200" aria-hidden="true" /><p className="mt-5 font-mono text-[10px] uppercase tracking-[0.24em] text-red-200/75">Operator_Access_Gate</p><h1 id="appchains-access-heading" className="mt-3 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum">{title}</h1><p className="mt-5 text-sm leading-relaxed text-platinum/55">{message}</p><div className="flex flex-col items-center gap-3 sm:flex-row sm:justify-center"><LoginLink />{state === "unavailable" && <button type="button" onClick={onRetry} className="mt-6 inline-flex min-h-11 items-center justify-center gap-2 border border-platinum/15 px-5 font-mono text-xs font-bold uppercase tracking-[0.16em] text-platinum/60 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"><RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />RETRY_CHECK</button>}</div></section></main>;
}

function CheckingState() {
  return <main id="main-content" tabIndex={-1} className="relative flex min-h-screen items-center justify-center overflow-hidden bg-ink-black px-4 py-10 dot-grid" aria-busy="true" aria-live="polite"><div className="relative flex items-center gap-3 border border-cyan-glow/30 bg-ink-black/90 px-6 py-5 font-mono text-xs uppercase tracking-[0.18em] text-cyan-glow"><LoaderCircle className="h-4 w-4 animate-spin" aria-hidden="true" />SECURITY_CHECK</div></main>;
}

export default function AppChainsConsole() {
  const [sessionState, setSessionState] = useState<AppChainsSessionState>("checking");
  const [session, setSession] = useState<AuthSession | null>(null);
  const [overview, setOverview] = useState<AppChainOverview | null>(null);
  const [overviewError, setOverviewError] = useState("");
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isLoggingOut, setIsLoggingOut] = useState(false);
  const overviewController = useRef<AbortController | null>(null);

  const checkSession = useCallback(async () => {
    setSessionState("checking");
    try {
      const nextSession = await fetchAuthSession();
      if (!nextSession) {
        setSession(null);
        setSessionState("unauthenticated");
        window.location.assign(`/login?next=${encodeURIComponent("/appchains")}`);
        return;
      }
      setSession(nextSession);
      setSessionState("authenticated");
    } catch {
      setSessionState("unavailable");
    }
  }, []);

  const refreshOverview = useCallback(async () => {
    if (!session) return;
    overviewController.current?.abort();
    const controller = new AbortController();
    overviewController.current = controller;
    setIsRefreshing(true);
    setOverviewError("");
    try {
      const nextOverview = await fetchAppChainOverview(controller.signal);
      if (controller.signal.aborted) return;
      setOverview(nextOverview);
    } catch (error) {
      if (controller.signal.aborted) return;
      if (error instanceof AppChainRequestError && error.status === 401) {
        setSessionState("expired");
        setOverviewError("Your authenticated operator session expired. Sign in again before reading the AppChain registry.");
      } else {
        setOverviewError(error instanceof Error ? error.message : "Unable to read the AppChain registry.");
      }
    } finally {
      if (!controller.signal.aborted) setIsRefreshing(false);
    }
  }, [session]);

  useEffect(() => { void checkSession(); }, [checkSession]);

  useEffect(() => {
    if (sessionState !== "authenticated" || !session) return;
    void refreshOverview();
    const interval = window.setInterval(() => void refreshOverview(), 15000);
    return () => {
      window.clearInterval(interval);
      overviewController.current?.abort();
    };
  }, [refreshOverview, session, sessionState]);

  const handleLogout = async () => {
    setIsLoggingOut(true);
    try {
      await logout();
      window.location.assign("/login");
    } catch (error) {
      setIsLoggingOut(false);
      setOverviewError(error instanceof Error ? error.message : "Unable to end the operator session.");
    }
  };

  // There is no anchor action here, and that is the design: anchoring is the owner's own signed
  // transaction from the mobile wallet. The retired server route answered 410, so a button that
  // called it could only fail — or lie.

  if (sessionState === "checking") return <CheckingState />;
  if (sessionState !== "authenticated" || !session) return <AccessState state={sessionState === "authenticated" ? "unavailable" : sessionState} onRetry={() => void checkSession()} />;

  return (
    <main id="main-content" tabIndex={-1} className="relative min-h-screen overflow-hidden bg-ink-black px-4 py-8 dot-grid sm:px-8 lg:px-10">
      <div className="pointer-events-none fixed inset-0 bg-[radial-gradient(circle_at_70%_20%,rgba(15,255,255,0.06),transparent_38%)]" />
      <div className="relative mx-auto max-w-7xl space-y-7 pt-2">
        <OperatorHeader session={session} isLoggingOut={isLoggingOut} onLogout={() => void handleLogout()} />
        <AppChainOverviewView overview={overview} isLoading={!overview && isRefreshing} isRefreshing={isRefreshing} error={overviewError} onRefresh={() => void refreshOverview()} />
        <footer className="flex flex-col gap-2 border-t border-platinum/10 pt-5 font-mono text-[10px] uppercase tracking-[0.16em] text-platinum-muted sm:flex-row sm:items-center sm:justify-between"><span>AppChain_Surface // Read_Live_State // Anchor_In_Owner_Wallet</span><span>Owner-signed version-5 transactions // no server key</span></footer>
      </div>
    </main>
  );
}
