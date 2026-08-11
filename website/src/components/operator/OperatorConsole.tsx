"use client";

import { useCallback, useEffect, useRef, useState } from "react";
import Link from "next/link";
import { AlertTriangle, LoaderCircle, LockKeyhole, RotateCcw } from "lucide-react";
import { fetchAuthSession, logout, type AuthSession } from "@/lib/auth";
import { emptyNetworkStats, fetchLatestTransactions, fetchNetworkStats, type LatestTransaction, type LiveNetworkStats } from "@/lib/api";
import { mineBlock, OperatorRequestError, type MineBlockResult } from "@/lib/operator";
import OperatorActivity from "@/components/operator/OperatorActivity";
import OperatorHeader from "@/components/operator/OperatorHeader";
import MineControl, { type MineState } from "@/components/operator/MineControl";
import OperatorStatsGrid from "@/components/operator/OperatorStatsGrid";

export type OperatorSessionState = "checking" | "authenticated" | "unauthenticated" | "expired" | "unavailable";

function LoginLink() {
  return (
    <Link
      href="/login?next=%2Foperator"
      className="mt-6 inline-flex min-h-11 items-center justify-center border border-cyan-glow/60 px-5 font-mono text-xs font-bold uppercase tracking-[0.16em] text-cyan-glow transition-colors hover:bg-cyan-glow hover:text-ink-black focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"
    >
      RETURN_TO_LOGIN
    </Link>
  );
}

function AccessState({ state, onRetry }: { state: Exclude<OperatorSessionState, "authenticated" | "checking">; onRetry: () => void }) {
  const isExpired = state === "expired";
  const title = isExpired ? "SESSION_EXPIRED" : state === "unavailable" ? "AUTH_SERVICE_UNAVAILABLE" : "AUTHENTICATION_REQUIRED";
  const message = isExpired
    ? "Your operator session is no longer valid. Authenticate again before requesting a state-changing operation."
    : state === "unavailable"
      ? "The node could not confirm the operator session. Retry when the API is reachable."
      : "This surface is not public. Continue through the authenticated wallet or CLI login flow.";

  return (
    <main className="relative flex min-h-screen items-center justify-center overflow-hidden bg-ink-black px-4 py-10 dot-grid sm:px-8">
      <div className="pointer-events-none fixed inset-0 bg-[radial-gradient(circle_at_50%_45%,rgba(15,255,255,0.08),transparent_58%)]" />
      <section className="relative w-full max-w-lg border border-red-300/30 bg-ink-black/90 p-6 text-center shadow-[0_0_40px_rgba(15,255,255,0.08)] sm:p-10" aria-labelledby="operator-access-heading">
        <AlertTriangle className="mx-auto h-8 w-8 text-red-200" aria-hidden="true" />
        <p className="mt-5 font-mono text-[10px] uppercase tracking-[0.24em] text-red-200/75">Operator_Access_Gate</p>
        <h1 id="operator-access-heading" className="mt-3 font-space-grotesk text-3xl font-bold uppercase tracking-tight text-platinum">
          {title}
        </h1>
        <p className="mt-5 text-sm leading-relaxed text-platinum/55">{message}</p>
        <div className="flex flex-col items-center gap-3 sm:flex-row sm:justify-center">
          <LoginLink />
          {state === "unavailable" && (
            <button
              type="button"
              onClick={onRetry}
              className="mt-6 inline-flex min-h-11 items-center justify-center gap-2 border border-platinum/15 px-5 font-mono text-xs font-bold uppercase tracking-[0.16em] text-platinum/60 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow"
            >
              <RotateCcw className="h-3.5 w-3.5" aria-hidden="true" />
              RETRY_CHECK
            </button>
          )}
        </div>
      </section>
    </main>
  );
}

function CheckingState() {
  return (
    <main className="relative flex min-h-screen items-center justify-center overflow-hidden bg-ink-black px-4 py-10 dot-grid" aria-busy="true" aria-live="polite">
      <div className="relative flex items-center gap-3 border border-cyan-glow/30 bg-ink-black/90 px-6 py-5 font-mono text-xs uppercase tracking-[0.18em] text-cyan-glow">
        <LoaderCircle className="h-4 w-4 animate-spin" aria-hidden="true" />
        SECURITY_CHECK
      </div>
    </main>
  );
}

export default function OperatorConsole() {
  const [sessionState, setSessionState] = useState<OperatorSessionState>("checking");
  const [session, setSession] = useState<AuthSession | null>(null);
  const [stats, setStats] = useState<LiveNetworkStats>(() => emptyNetworkStats());
  const [transactions, setTransactions] = useState<LatestTransaction[]>([]);
  const [telemetryError, setTelemetryError] = useState("");
  const [isRefreshing, setIsRefreshing] = useState(false);
  const [isLoggingOut, setIsLoggingOut] = useState(false);
  const [mineState, setMineState] = useState<MineState>("idle");
  const [mineResult, setMineResult] = useState<MineBlockResult | null>(null);
  const [mineError, setMineError] = useState("");
  const telemetryController = useRef<AbortController | null>(null);

  const checkSession = useCallback(async () => {
    setSessionState("checking");
    try {
      const nextSession = await fetchAuthSession();
      if (!nextSession) {
        setSession(null);
        setSessionState("unauthenticated");
        window.location.assign(`/login?next=${encodeURIComponent("/operator")}`);
        return;
      }
      setSession(nextSession);
      setSessionState("authenticated");
    } catch {
      setSessionState("unavailable");
    }
  }, []);

  const refreshTelemetry = useCallback(async () => {
    if (!session) return;
    telemetryController.current?.abort();
    const controller = new AbortController();
    telemetryController.current = controller;
    setIsRefreshing(true);
    setTelemetryError("");

    try {
      const [nextStats, nextTransactions] = await Promise.all([
        fetchNetworkStats(controller.signal),
        fetchLatestTransactions(controller.signal),
      ]);
      setStats(nextStats);
      setTransactions(nextTransactions);
    } catch (error) {
      if (controller.signal.aborted) return;
      setStats((current) => ({ ...current, status: "unavailable", updatedAt: null }));
      setTelemetryError(error instanceof Error ? error.message : "Unable to read live node telemetry.");
    } finally {
      if (!controller.signal.aborted) setIsRefreshing(false);
    }
  }, [session]);

  useEffect(() => {
    void checkSession();
  }, [checkSession]);

  useEffect(() => {
    if (sessionState !== "authenticated" || !session) return;
    void refreshTelemetry();
    const interval = window.setInterval(() => void refreshTelemetry(), 15000);
    return () => {
      window.clearInterval(interval);
      telemetryController.current?.abort();
    };
  }, [refreshTelemetry, session, sessionState]);

  const handleLogout = async () => {
    setIsLoggingOut(true);
    try {
      await logout();
      window.location.assign("/login");
    } catch (error) {
      setIsLoggingOut(false);
      setTelemetryError(error instanceof Error ? error.message : "Unable to end the operator session.");
    }
  };

  const handleArm = () => {
    setMineError("");
    setMineResult(null);
    setMineState("armed");
  };

  const handleMine = async () => {
    setMineError("");
    setMineState("submitting");
    try {
      const result = await mineBlock();
      setMineResult(result);
      setMineState("success");
      await refreshTelemetry();
    } catch (error) {
      const status = error instanceof OperatorRequestError ? error.status : 0;
      if (status === 401) {
        setMineState("unavailable");
        setSessionState("expired");
        setMineError("Your authenticated operator session expired. Sign in again before retrying.");
        return;
      }
      setMineState(status === 400 ? "rejected" : "unavailable");
      setMineError(error instanceof Error ? error.message : "Unable to submit the mining request.");
    }
  };

  if (sessionState === "checking") return <CheckingState />;
  if (sessionState !== "authenticated" || !session) {
    const accessState = sessionState === "authenticated" ? "unavailable" : sessionState;
    return <AccessState state={accessState} onRetry={() => void checkSession()} />;
  }

  return (
    <main className="relative min-h-screen overflow-hidden bg-ink-black px-4 py-8 dot-grid sm:px-8 lg:px-10">
      <div className="pointer-events-none fixed inset-0 bg-[radial-gradient(circle_at_70%_20%,rgba(15,255,255,0.06),transparent_38%)]" />
      <div className="relative mx-auto max-w-7xl space-y-7 pt-2">
        <OperatorHeader session={session} isLoggingOut={isLoggingOut} onLogout={() => void handleLogout()} />

        {telemetryError && sessionState === "authenticated" && (
          <div className="border border-red-300/30 bg-red-300/10 px-4 py-3 font-mono text-xs text-red-200" role="alert" aria-live="assertive">
            {telemetryError}
          </div>
        )}

        <div className="grid gap-7 xl:grid-cols-[minmax(0,1fr)_minmax(320px,0.38fr)] xl:items-start">
          <OperatorStatsGrid stats={stats} isRefreshing={isRefreshing} onRefresh={() => void refreshTelemetry()} />
          <MineControl
            state={mineState}
            result={mineResult}
            error={mineError}
            onArm={handleArm}
            onCancel={() => setMineState("idle")}
            onConfirm={() => void handleMine()}
          />
        </div>

        <OperatorActivity transactions={transactions} status={stats.status} />

        <footer className="flex flex-col gap-2 border-t border-platinum/10 pt-5 font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/25 sm:flex-row sm:items-center sm:justify-between">
          <span>Operator_Surface // Read_Live_State // Write_With_Confirmation</span>
          <span>Private keys never enter this console</span>
        </footer>
      </div>
    </main>
  );
}
