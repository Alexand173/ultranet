"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { AlertTriangle, LoaderCircle, RotateCcw } from "lucide-react";
import { fetchAuthSession, logout, type AuthSession } from "@/lib/auth";
import { anchorAppChain, AppChainRequestError, createAppChain, fetchAppChainOverview } from "@/lib/appchains";
import type { AppChainAction, AppChainOverview, CreateAppChainInput } from "@/lib/appchains.types";
import AppChainOverviewView from "@/components/appchains/AppChainOverview";
import RegisterAppChainModal, { type RegistrationState } from "@/components/appchains/RegisterAppChainModal";
import OperatorHeader from "@/components/operator/OperatorHeader";

export type AppChainsSessionState = "checking" | "authenticated" | "unauthenticated" | "expired" | "unavailable";

function LoginLink() {
  return <Link href="/login?next=%2Fappchains" className="mt-6 inline-flex min-h-11 items-center justify-center border border-cyan-glow/60 px-5 font-mono text-xs font-bold uppercase tracking-[0.16em] text-cyan-glow transition-colors hover:bg-cyan-glow hover:text-ink-black focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow">RETURN_TO_LOGIN</Link>;
}

function AccessState({ state, onRetry }: { state: Exclude<AppChainsSessionState, "authenticated" | "checking">; onRetry: () => void }) {
  const isExpired = state === "expired";
  const title = isExpired ? "SESSION_EXPIRED" : state === "unavailable" ? "AUTH_SERVICE_UNAVAILABLE" : "AUTHENTICATION_REQUIRED";
  const message = isExpired ? "Your operator session is no longer valid. Authenticate again before changing AppChain state." : state === "unavailable" ? "The node could not confirm the operator session. Retry when the API is reachable." : "This surface is not public. Continue through the authenticated operator login flow.";
  return <main className="relative flex min-h-screen items-center justify-center overflow-hidden bg-ink-black px-4 py-10 dot-grid sm:px-8"><div className="pointer-events-none fixed inset-0 bg-[radial-gradient(circle_at_50%_45%,rgba(15,255,255,0.08),transparent_58%)]" /><section className="relative w-full max-w-lg border border-red-300/30 bg-ink-black/90 p-6 text-center shadow-[0_0_40px_rgba(15,255,255,0.08)] sm:p-10" aria-labelledby="appchains-access-heading"><AlertTriangle className="mx-auto h-8 w-8 text-red-200" aria-hidden="true" /><p className="mt-5 font-mono text-[10px] uppercase tracking-[0.24em] text-red-200/75">Operator_Access_Gate</p><h1 id="appchains-access-heading" className="mt-3 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum">{title}</h1><p className="mt-5 text-sm leading-relaxed text-platinum/55">{message}</p><div className="flex flex-col items-center gap-3 sm:flex-row sm:justify-center"><LoginLink />{state === "unavailable" && <button type="button" onClick={onRetry} className="mt-6 inline-flex min-h-11 items-center justify-center gap-2 border border-platinum/15 px-5 font-mono text-xs font-bold uppercase tracking-[0.16em] text-platinum/60 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"><RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />RETRY_CHECK</button>}</div></section></main>;
}

function CheckingState() {
  return <main className="relative flex min-h-screen items-center justify-center overflow-hidden bg-ink-black px-4 py-10 dot-grid" aria-busy="true" aria-live="polite"><div className="relative flex items-center gap-3 border border-cyan-glow/30 bg-ink-black/90 px-6 py-5 font-mono text-xs uppercase tracking-[0.18em] text-cyan-glow"><LoaderCircle className="h-4 w-4 animate-spin" aria-hidden="true" />SECURITY_CHECK</div></main>;
}

const idleAction = (): AppChainAction => ({ state: "idle", error: "", result: null });

export default function AppChainsConsole() {
  const [sessionState, setSessionState] = useState<AppChainsSessionState>("checking");
  const [session, setSession] = useState<AuthSession | null>(null);
  const [overview, setOverview] = useState<AppChainOverview | null>(null);
  const [overviewError, setOverviewError] = useState("");
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [registrationOpen, setRegistrationOpen] = useState(false);
  const [registrationState, setRegistrationState] = useState<RegistrationState>("idle");
  const [registrationError, setRegistrationError] = useState("");
  const [notice, setNotice] = useState("");
  const [anchorActions, setAnchorActions] = useState<Record<number, AppChainAction>>({});
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
      setAnchorActions((current) => {
        const next = { ...current };
        for (const chain of nextOverview.chains) next[chain.id] ??= idleAction();
        return next;
      });
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

  const handleCreate = async (input: CreateAppChainInput) => {
    setRegistrationState("submitting");
    setRegistrationError("");
    try {
      const result = await createAppChain(input);
      setRegistrationState("idle");
      setRegistrationOpen(false);
      setNotice(result.message);
      await refreshOverview();
    } catch (error) {
      if (error instanceof AppChainRequestError && error.status === 401) setSessionState("expired");
      setRegistrationState("error");
      setRegistrationError(error instanceof Error ? error.message : "Unable to create the AppChain.");
    }
  };

  const updateAnchorAction = (chainId: number, update: (current: AppChainAction) => AppChainAction) => {
    setAnchorActions((current) => ({ ...current, [chainId]: update(current[chainId] ?? idleAction()) }));
  };

  const handleAnchorArm = (chainId: number) => updateAnchorAction(chainId, (current) => ({ ...current, state: "confirming", error: "", result: null }));
  const handleAnchorCancel = (chainId: number) => updateAnchorAction(chainId, () => idleAction());
  const handleAnchorConfirm = async (chainId: number) => {
    updateAnchorAction(chainId, (current) => ({ ...current, state: "submitting", error: "" }));
    try {
      const result = await anchorAppChain(chainId);
      updateAnchorAction(chainId, () => ({ state: "success", error: "", result }));
      setNotice(`${result.message} Treasury: ${result.accountAddress ? `${result.accountAddress.slice(0, 10)}…${result.accountAddress.slice(-8)}` : "unavailable"}.`);
      await refreshOverview();
    } catch (error) {
      const status = error instanceof AppChainRequestError ? error.status : 0;
      if (status === 401) setSessionState("expired");
      updateAnchorAction(chainId, (current) => ({ ...current, state: status === 400 ? "rejected" : "unavailable", error: error instanceof Error ? error.message : "Unable to anchor the AppChain." }));
    }
  };

  if (sessionState === "checking") return <CheckingState />;
  if (sessionState !== "authenticated" || !session) return <AccessState state={sessionState === "authenticated" ? "unavailable" : sessionState} onRetry={() => void checkSession()} />;

  return (
    <main className="relative min-h-screen overflow-hidden bg-ink-black px-4 py-8 dot-grid sm:px-8 lg:px-10">
      <div className="pointer-events-none fixed inset-0 bg-[radial-gradient(circle_at_70%_20%,rgba(15,255,255,0.06),transparent_38%)]" />
      <div className="relative mx-auto max-w-7xl space-y-7 pt-2">
        <OperatorHeader session={session} isLoggingOut={isLoggingOut} onLogout={() => void handleLogout()} />
        {notice && <div className="border border-emerald-300/25 bg-emerald-300/[0.06] px-4 py-3 font-mono text-xs text-emerald-100/80" role="status" aria-live="polite">{notice}</div>}
        <AppChainOverviewView overview={overview} isLoading={!overview && isRefreshing} isRefreshing={isRefreshing} error={overviewError} onRefresh={() => void refreshOverview()} onRegister={() => { setRegistrationError(""); setRegistrationState("idle"); setRegistrationOpen(true); }} actions={anchorActions} onArm={handleAnchorArm} onCancel={handleAnchorCancel} onConfirm={(chainId) => void handleAnchorConfirm(chainId)} />
        <footer className="flex flex-col gap-2 border-t border-platinum/10 pt-5 font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/25 sm:flex-row sm:items-center sm:justify-between"><span>AppChain_Surface // Read_Live_State // Write_With_Confirmation</span><span>Server proof + real L1 treasury debit</span></footer>
      </div>
      <RegisterAppChainModal open={registrationOpen} state={registrationState} error={registrationError} onClose={() => { if (registrationState !== "submitting") setRegistrationOpen(false); }} onSubmit={handleCreate} />
    </main>
  );
}
