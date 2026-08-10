"use client";

import { useState } from "react";
import { motion } from "framer-motion";
import { Scale, FileText } from "lucide-react";
import { API_BASE_URL } from "@/lib/links";

type VerificationState = "idle" | "checking" | "verified" | "error";

const ARTICLES = [
  {
    title: "Article I: Protocol Immutability",
    content: "The consensus rules of UltraNet are hardcoded into the Genesis block and cannot be altered by any entity, including the Sovereign owners, after block height 1,000,000.",
  },
  {
    title: "Article II: Privacy Sovereignty",
    content: "Zero-knowledge proofs and Fully Homomorphic Encryption are core protocol invariants. No entity shall have the right to request decryption of the global state trie.",
  },
  {
    title: "Article III: Economic Sustainability",
    content: "The halving schedule and total supply cap are protected by recursive SNARK audits. Any deviation results in an immediate network stall to prevent inflation corruption.",
  },
];

export default function LegislationPage() {
  const [verificationState, setVerificationState] = useState<VerificationState>("idle");
  const [verificationMessage, setVerificationMessage] = useState("");

  const verifyLegislation = async () => {
    setVerificationState("checking");
    setVerificationMessage("");

    try {
      const response = await fetch(`${API_BASE_URL}/api/validate`);
      const payload = await response.json().catch(() => null);

      if (!response.ok || !payload?.success || payload.data?.valid !== true) {
        throw new Error(payload?.message || "The node did not verify the current chain.");
      }

      setVerificationState("verified");
      setVerificationMessage("Genesis chain integrity confirmed by the connected node.");
    } catch (error) {
      setVerificationState("error");
      setVerificationMessage(error instanceof Error ? error.message : "Unable to verify legislation against the node.");
    }
  };

  return (
    <main className="min-h-screen py-24 px-6 max-w-4xl mx-auto space-y-16 pt-32">
      <div className="space-y-6 text-center">
        <Scale className="w-16 h-12 text-cyan-glow mx-auto" aria-hidden="true" />
        <h1 className="text-4xl md:text-6xl font-bold tracking-tighter uppercase">Protocol Legislation</h1>
        <p className="text-platinum/40 font-mono text-sm max-w-2xl mx-auto uppercase">
          The legal framework governing the 100-year operation of the UltraNet blockchain.
          Mathematical law verified by code.
        </p>
      </div>

      <div className="space-y-12">
        {ARTICLES.map((article, i) => (
          <motion.div
            key={i}
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            transition={{ delay: i * 0.1 }}
            className="p-8 bg-zinc-950/50 border border-platinum/10 rounded-lg space-y-4 hover:border-cyan-glow/50 transition-colors group"
          >
            <div className="flex items-center gap-3">
              <FileText className="w-5 h-5 text-cyan-glow" aria-hidden="true" />
              <h2 className="text-xl font-bold text-platinum group-hover:text-cyan-glow transition-colors">{article.title}</h2>
            </div>
            <p className="text-platinum/60 leading-relaxed font-light italic">
              {article.content}
            </p>
          </motion.div>
        ))}
      </div>

      <div className="p-12 neon-inset text-center space-y-6">
        <h3 className="text-2xl font-bold uppercase">Digital Seal of Authority</h3>
        <p className="text-sm text-platinum/40 font-mono">
          SIGNED_BY: SOVEREIGN_GENESIS_VAULT<br />
          TIMESTAMP: 2026-07-21T15:53:31Z<br />
          HASH: 0x3B8EF38ADA262F3290BBAB6A89B9AE43692...
        </p>

        {verificationMessage && (
          <div
            role={verificationState === "error" ? "alert" : "status"}
            aria-live="polite"
            className={`border px-4 py-3 font-mono text-xs leading-relaxed ${
              verificationState === "verified"
                ? "border-cyan-glow/40 bg-cyan-glow/10 text-cyan-glow"
                : "border-red-300/40 bg-red-300/10 text-red-200"
            }`}
          >
            {verificationMessage}
          </div>
        )}

        <button
          type="button"
          onClick={verifyLegislation}
          disabled={verificationState === "checking"}
          aria-busy={verificationState === "checking"}
          className="px-8 py-3 bg-cyan-glow text-ink-black font-black uppercase text-xs tracking-widest hover:scale-105 disabled:cursor-wait disabled:opacity-60 transition-transform"
        >
          {verificationState === "checking" ? "VERIFYING_CHAIN..." : "VERIFY_LEGISLATION.sh"}
        </button>
      </div>
    </main>
  );
}
