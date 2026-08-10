"use client";

import { useEffect, useState } from "react";
import HomeLearningIntro from "./HomeLearningIntro";
import StatusTicker from "./command-center/StatusTicker";
import NodeDiagnosticsTerminal from "./command-center/NodeDiagnosticsTerminal";
import ShardedStateTerminal from "./command-center/ShardedStateTerminal";
import LedgerTerminal from "./command-center/LedgerTerminal";
import AdminConsoleTerminal from "./command-center/AdminConsoleTerminal";
import { emptyNetworkStats, fetchNetworkStats, type LiveNetworkStats } from "@/lib/api";

function formatMetric(value: number | null, suffix = ""): string {
  return value === null ? "Unavailable" : `${new Intl.NumberFormat("en-US", { maximumFractionDigits: 2 }).format(value)}${suffix}`;
}

export default function CommandCenter() {
  const [stats, setStats] = useState<LiveNetworkStats>(() => emptyNetworkStats());

  useEffect(() => {
    let mounted = true;
    let controller: AbortController | null = null;

    const loadStats = async () => {
      controller?.abort();
      controller = new AbortController();
      try {
        const nextStats = await fetchNetworkStats(controller.signal);
        if (mounted) setStats(nextStats);
      } catch {
        if (mounted) setStats((current) => ({ ...current, status: "unavailable", updatedAt: null }));
      }
    };

    void loadStats();
    const interval = window.setInterval(() => void loadStats(), 10000);
    return () => {
      mounted = false;
      controller?.abort();
      window.clearInterval(interval);
    };
  }, []);

  const height = stats.totalBlocks === null ? "Unavailable" : new Intl.NumberFormat("en-US").format(stats.totalBlocks);
  const provingTime = formatMetric(stats.provingTimeMs, " ms");

  return (
    <section className="relative w-full mt-20 flex flex-col 2xl:h-[calc(100vh-5rem)] bg-[#010B13] border-b border-platinum/10">
      <StatusTicker position="top" blockHeight={height} provingTime={provingTime} />

      {/* The homepage uses the center lane to orient ordinary users first.
          The technical Coin Nexus remains available as a component for future
          protocol surfaces, while the learning hub becomes the first step here. */}
      <div className="flex-1 flex flex-col 2xl:grid 2xl:grid-cols-[minmax(260px,16vw)_minmax(560px,1fr)_minmax(260px,16vw)] gap-px bg-platinum/10 2xl:overflow-hidden">
        {/* Left column: TL + BL terminals (desktop only) */}
        <div className="hidden 2xl:flex 2xl:flex-col gap-px bg-platinum/10">
          <NodeDiagnosticsTerminal className="flex-1" />
          <ShardedStateTerminal className="flex-1" />
        </div>

        {/* Center: ordinary-user orientation */}
        <HomeLearningIntro className="flex-1 min-h-[85vh] 2xl:min-h-0" />

        {/* Right column: TR + BR terminals (desktop only) */}
        <div className="hidden 2xl:flex 2xl:flex-col gap-px bg-platinum/10">
          <LedgerTerminal className="flex-1" />
          <AdminConsoleTerminal className="flex-1" />
        </div>
      </div>

      {/* Mobile/tablet/laptop (<2xl): stack live terminals below the learning intro */}
      <div className="grid grid-cols-1 sm:grid-cols-2 gap-px bg-platinum/10 2xl:hidden">
        <NodeDiagnosticsTerminal className="h-96" />
        <LedgerTerminal className="h-96" />
        <ShardedStateTerminal className="h-96" />
        <AdminConsoleTerminal className="h-96" />
      </div>

      <StatusTicker position="bottom" blockHeight={height} provingTime={provingTime} />
    </section>
  );
}
