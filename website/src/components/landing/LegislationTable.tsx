"use client";

import { motion } from "framer-motion";
import { CheckSquare, ShieldCheck } from "lucide-react";

const LAWS = [
  {
    id: "SRC_01",
    title: "Consensus Finality",
    text: "Recursive proof components exist in the repository; the current implementation does not establish an irreversible-finality guarantee after 10 rounds.",
    section: "Implementation Notes",
  },
  {
    id: "SRC_02",
    title: "Supply and Halving",
    text: "The code defines a 1,000,000 $ULTRA genesis allocation and a 31,557,600-block halving interval. No enforced 21,000,000 $ULTRA cap is established here.",
    section: "Implementation Notes",
  },
  {
    id: "SRC_03",
    title: "Nullifier Handling",
    text: "Nullifier storage exists in the transaction path; automatic validator pruning for conflicting nullifiers is not established by the reviewed implementation.",
    section: "Implementation Notes",
  },
  {
    id: "SRC_04",
    title: "Sovereign Approval",
    text: "The configured threshold is 2-of-3. No automatic authority expiry at block height 1,000,000 is established by the current source.",
    section: "Implementation Notes",
  },
];

export default function LegislationTable() {
  const sections = Array.from(new Set(LAWS.map((l) => l.section)));

  return (
    <section className="py-16 px-4 max-w-5xl mx-auto space-y-12 relative z-10">
      <div className="text-center space-y-4">
        <ShieldCheck className="w-12 h-12 text-cyan-glow mx-auto" />
        <h2 className="text-4xl md:text-5xl font-bold uppercase tracking-widest">Protocol Source Notes</h2>
        <p className="text-platinum/40 font-mono">REPOSITORY SUMMARY // NOT AN AUDIT OR SIGNED LAW</p>
      </div>

      <div className="space-y-12">
        {sections.map((section, sIndex) => (
          <div key={section} className="space-y-6">
            <div className="flex items-center gap-4">
              <div className="w-2 h-2 rounded-full bg-cyan-glow shadow-[0_0_10px_#0FFFFF]" />
              <h3 className="text-sm font-bold uppercase text-cyan-glow tracking-widest">{section}</h3>
              <div className="h-[1px] flex-1 bg-platinum/10" />
            </div>

            <div className="grid grid-cols-1 gap-px bg-platinum/10 border border-platinum/10">
              {LAWS.filter((l) => l.section === section).map((law, lIndex) => (
                <motion.div
                  key={law.id}
                  initial={{ opacity: 0, x: -20 }}
                  whileInView={{ opacity: 1, x: 0 }}
                  transition={{ delay: (sIndex * 0.2) + (lIndex * 0.1) }}
                  className="bg-ink-black p-6 grid grid-cols-1 md:grid-cols-[120px_1fr_40px] items-center gap-6 group hover:bg-platinum/[0.02] transition-colors"
                >
                  <span className="font-mono text-xs text-platinum/40 group-hover:text-cyan-glow transition-colors">{law.id}</span>
                  <div className="space-y-1">
                    <p className="font-bold text-lg">{law.title}</p>
                    <p className="text-sm text-platinum/50 leading-relaxed italic">&quot;{law.text}&quot;</p>
                  </div>
                  <CheckSquare className="w-5 h-5 text-cyan-glow/20 group-hover:text-cyan-glow transition-colors" />
                </motion.div>
              ))}
            </div>
          </div>
        ))}
      </div>

      <div className="p-8 border border-dashed border-platinum/20 text-center rounded-lg">
        <p className="text-xs text-platinum/30 font-mono">
          STATIC_REPOSITORY_SUMMARY // NO_DIGITAL_SEAL_VERIFICATION_PERFORMED
        </p>
      </div>
    </section>
  );
}
