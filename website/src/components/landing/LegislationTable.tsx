"use client";

import { motion } from "framer-motion";
import { CheckSquare, ShieldCheck } from "lucide-react";

const LAWS = [
  {
    id: "LAW_01A",
    title: "Consensus Finality",
    text: "No block height shall be reversed once recursive SNARK depth exceeds 10 rounds.",
    section: "Consensus Rules",
  },
  {
    id: "LAW_01B",
    title: "Emission Integrity",
    text: "Total supply capped at 21,000,000 $ULTRA. Halving interval strictly 31,557,600 seconds.",
    section: "Consensus Rules",
  },
  {
    id: "LAW_02A",
    title: "Double-Spend Slashing",
    text: "Validators submitting conflicting nullifiers will be pruned from the state trie immediately.",
    section: "Validator Conduct",
  },
  {
    id: "LAW_03A",
    title: "Sovereign Handover",
    text: "Genesis multi-sig authority expires automatically at block height 1,000,000.",
    section: "Governance",
  },
];

export default function LegislationTable() {
  const sections = Array.from(new Set(LAWS.map((l) => l.section)));

  return (
    <section className="py-16 px-4 max-w-5xl mx-auto space-y-12 relative z-10">
      <div className="text-center space-y-4">
        <ShieldCheck className="w-12 h-12 text-cyan-glow mx-auto" />
        <h2 className="text-4xl md:text-5xl font-bold uppercase tracking-widest">Sovereign Legislation</h2>
        <p className="text-platinum/40 font-mono">immutable_protocol_laws.json</p>
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
          DIGITAL_SIGNATURE_VERIFIED_BY_PROTOCOL_GENESIS_VAULT // 0x3B8EF...
        </p>
      </div>
    </section>
  );
}
