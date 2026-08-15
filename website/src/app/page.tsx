import Link from "next/link";
import CommandCenter from "@/components/landing/CommandCenter";
import BentoDocs from "@/components/landing/BentoDocs";
import SwarmOnboarding from "@/components/landing/SwarmOnboarding";
import LongevityTimeline from "@/components/landing/LongevityTimeline";
import LegislationTable from "@/components/landing/LegislationTable";
import { EXPLORER_URL } from "@/lib/links";

export default function Home() {
  return (
    <main className="min-h-screen overflow-x-clip bg-ink-black terminal-overlay">
      <CommandCenter />
      <div className="section-connector" />
      <BentoDocs />
      <div className="section-connector" />
      <LongevityTimeline />
      <div className="section-connector" />
      <SwarmOnboarding />
      <div className="section-connector" />
      <LegislationTable />

      {/* Simple Footer */}
      <footer className="py-12 border-t border-platinum/10 text-center space-y-4">
        <p className="text-sm text-platinum/40 font-mono tracking-widest uppercase">
          UltraNet Sovereign Node v7.1 // 100-Year Longevity Engine
        </p>
        <p className="text-[10px] text-platinum/30 font-mono tracking-[0.18em] uppercase">
          Original code and protocol documentation © 2026 Vladan Jotov // ISC License
        </p>
        <a
          href="mailto:contact@ultranetwork.cc"
          className="inline-block text-[10px] font-mono tracking-[0.18em] text-platinum/50 transition-colors hover:text-cyan-glow focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black"
        >
          contact@ultranetwork.cc
        </a>
        <div className="flex flex-wrap justify-center gap-8 text-xs font-bold uppercase tracking-widest text-platinum/60">
          <a href={EXPLORER_URL} target="_blank" rel="noreferrer" className="hover:text-cyan-glow transition-colors">Explorer</a>
          <Link href="/docs" className="hover:text-cyan-glow transition-colors">Whitepaper</Link>
          <Link href="/legislation" className="hover:text-cyan-glow transition-colors">Audit</Link>
        </div>
      </footer>
    </main>
  );
}
