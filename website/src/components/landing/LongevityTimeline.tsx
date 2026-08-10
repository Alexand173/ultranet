"use client";

import { motion } from "framer-motion";
import { cn } from "@/lib/utils";

const MILESTONES = [
  { era: "GENESIS", year: "2026", goal: "Sovereign Bootstrapping", status: "ACTIVE" },
  { era: "DECENTRALIZATION", year: "2031", goal: "Validator Swarm Maturity", status: "PENDING" },
  { era: "STABILITY", year: "2046", goal: "Full Autonomous AI Governance", status: "FUTURE" },
  { era: "LEGACY", year: "2126", goal: "100-Year Protocol Attestation", status: "FUTURE" },
];

export default function LongevityTimeline() {
  return (
    <section className="py-16 px-6 max-w-7xl mx-auto space-y-16 relative z-10">
      <div className="flex flex-col md:flex-row md:items-end justify-between gap-8">
        <div className="space-y-4">
          <p className="text-cyan-glow font-mono text-sm tracking-[0.3em] uppercase">Chronology // Longevity</p>
          <h2 className="text-4xl md:text-6xl font-bold tracking-tighter uppercase">The 100-Year Vision</h2>
        </div>
        <p className="max-w-md text-platinum/40 text-sm leading-relaxed uppercase tracking-wider font-mono">
          A ledger engineered to survive geopolitical shifts, hardware evolution, and the quantum transition.
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-4 gap-8">
        {MILESTONES.map((m, i) => (
          <motion.div
            key={m.era}
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            transition={{ delay: i * 0.1 }}
            className="group relative p-8 border border-platinum/10 cut-corner hover:border-cyan-glow/30 transition-colors bg-zinc-950/40"
          >
            <div className="space-y-6 relative z-10">
              <div className="flex justify-between items-start">
                <span className="text-4xl font-black text-platinum/10 group-hover:text-cyan-glow/20 transition-colors leading-none">
                  {m.year}
                </span>
                <span className={cn(
                  "text-[8px] font-bold px-2 py-1 border rounded-sm tracking-widest",
                  m.status === "ACTIVE" ? "border-cyan-glow text-cyan-glow shadow-[0_0_10px_#0FFFFF]" : "border-platinum/20 text-platinum/20"
                )}>
                  {m.status}
                </span>
              </div>
              <div className="space-y-2">
                <h4 className="text-lg font-bold tracking-widest">{m.era}</h4>
                <p className="text-xs text-platinum/40 leading-relaxed uppercase">{m.goal}</p>
              </div>
            </div>
            
            {/* Hover Decorative Line */}
            <div className="absolute bottom-0 left-0 w-full h-[1px] bg-gradient-to-r from-transparent via-cyan-glow/0 to-transparent group-hover:via-cyan-glow/50 transition-all duration-700" />
          </motion.div>
        ))}
      </div>
    </section>
  );
}
