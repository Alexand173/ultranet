"use client";

import { useEffect, useState } from "react";
import { Brain } from "lucide-react";
import DocPage from "@/components/docs/DocPage";
import { API_BASE_URL } from "@/lib/links";

type GovernorResponse = {
  success?: unknown;
  history?: unknown;
  sustainability_score?: unknown;
};

type GovernorSnapshot = {
  status: "loading" | "live" | "unavailable";
  score: number | null;
  sampleCount: number | null;
  updatedAt: string | null;
};

const INITIAL_SNAPSHOT: GovernorSnapshot = {
  status: "loading",
  score: null,
  sampleCount: null,
  updatedAt: null,
};

function formatSnapshot(snapshot: GovernorSnapshot): string {
  if (snapshot.status !== "live" || snapshot.score === null || snapshot.sampleCount === null) {
    return "Unavailable";
  }
  if (snapshot.sampleCount === 0) return `Default ${snapshot.score.toFixed(1)}`;
  return snapshot.score.toFixed(1);
}

export default function AutonomousIntelligenceDoc() {
  const [snapshot, setSnapshot] = useState<GovernorSnapshot>(INITIAL_SNAPSHOT);

  useEffect(() => {
    let mounted = true;
    let controller: AbortController | null = null;

    const load = async () => {
      controller?.abort();
      const request = new AbortController();
      controller = request;

      try {
        const response = await fetch(`${API_BASE_URL}/api/ai/history`, {
          signal: request.signal,
          cache: "no-store",
          headers: { Accept: "application/json" },
        });
        if (!response.ok) throw new Error("Governor telemetry is unavailable.");

        const payload = await response.json() as GovernorResponse;
        if (
          payload.success !== true ||
          !Array.isArray(payload.history) ||
          typeof payload.sustainability_score !== "number" ||
          !Number.isFinite(payload.sustainability_score)
        ) {
          throw new Error("Governor telemetry response is incomplete.");
        }

        if (mounted) {
          setSnapshot({
            status: "live",
            score: payload.sustainability_score,
            sampleCount: payload.history.length,
            updatedAt: new Date().toISOString(),
          });
        }
      } catch {
        if (mounted && !request.signal.aborted) {
          setSnapshot({ status: "unavailable", score: null, sampleCount: null, updatedAt: null });
        }
      }
    };

    void load();
    const interval = window.setInterval(() => void load(), 30000);
    return () => {
      mounted = false;
      controller?.abort();
      window.clearInterval(interval);
    };
  }, []);

  const scoreStatus = snapshot.status === "live"
    ? snapshot.sampleCount === 0 ? "DEFAULT // NO SAMPLES" : "API_SAMPLES_REPORTED"
    : snapshot.status === "loading" ? "CONNECTING" : "UNAVAILABLE";

  return (
    <DocPage
      pillar="A"
      icon={<Brain className="w-6 h-6" />}
      title="AI Governor Implementation"
      subtitle="A repository-level, rule-based governor; no on-chain LLM or decentralized agent voting is demonstrated by the current implementation."
      content={
        <div className="space-y-12">
          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">01. Current Code Path</h2>
            <p>
              The Rust <strong>AIGovernor</strong> stores a bounded in-process history of chain metrics and computes a heuristic sustainability score. It can suggest difficulty changes and calculates a reward from block count and transaction density. This is a rule-based module, not evidence of an on-chain LLM, autonomous agents, or independently validated economic governance.
            </p>
          </section>

          <section className="bg-cyan-glow/5 border border-cyan-glow/10 p-8 rounded-sm">
            <div className="flex flex-wrap items-start justify-between gap-4">
              <div>
                <h2 className="text-xl font-bold text-cyan-glow uppercase tracking-tight">02. Public Governor Snapshot</h2>
                <p className="mt-3 max-w-2xl text-sm leading-relaxed text-platinum/60">
                  Values come from the connected node&apos;s <code>/api/ai/history</code> response and refresh every 30 seconds. A score with zero history entries is the module&apos;s initialized default, not a measured network-health score.
                </p>
              </div>
              <span className="font-mono text-[10px] uppercase tracking-widest text-platinum/45" aria-live="polite">
                {scoreStatus}
              </span>
            </div>

            <div className="mt-8 grid gap-4 sm:grid-cols-2">
              <div className="border border-platinum/10 p-5">
                <p className="font-mono text-[10px] uppercase tracking-widest text-platinum/40">Sustainability score</p>
                <p className="mt-3 font-space-grotesk text-2xl font-bold text-cyan-glow">{formatSnapshot(snapshot)}</p>
                <p className="mt-2 text-xs text-platinum/45">Not a reward multiplier or an audited stability guarantee.</p>
              </div>
              <div className="border border-platinum/10 p-5">
                <p className="font-mono text-[10px] uppercase tracking-widest text-platinum/40">Recorded samples</p>
                <p className="mt-3 font-space-grotesk text-2xl font-bold text-platinum">
                  {snapshot.sampleCount === null ? "Unavailable" : new Intl.NumberFormat("en-US").format(snapshot.sampleCount)}
                </p>
                <p className="mt-2 text-xs text-platinum/45">
                  {snapshot.updatedAt ? `Read ${new Intl.DateTimeFormat("en-US", { timeStyle: "medium" }).format(new Date(snapshot.updatedAt))}` : "No API observation available"}
                </p>
              </div>
            </div>
          </section>

          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">03. Governance Boundary</h2>
            <p>
              Validator proposals require the configured 2-of-3 Sovereign approval threshold. The public source does not show AI agents holding tokens and voting on protocol proposals. Treat the AI Governor as implementation code and its API output as node-reported telemetry, not decentralized governance or an independent audit.
            </p>
          </section>
        </div>
      }
    />
  );
}
