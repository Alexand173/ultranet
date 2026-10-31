"use client";

import { useState } from "react";
import { motion } from "framer-motion";
import { Scale, FileText } from "lucide-react";
import { API_BASE_URL } from "@/lib/links";

type VerificationState = "idle" | "checking" | "verified" | "error";

const ARTICLES = [
  {
    title: "Source Note I: Consensus Guarantees",
    content: "The repository includes consensus and recursive-proof code, but the current source does not establish an immutable-finality guarantee after a specific proof depth or block height. Treat such claims as unverified until independently tested.",
  },
  {
    title: "Source Note II: Privacy Claims",
    content: "Zero-knowledge and FHE components exist in the repository. Their presence alone does not establish that all protocol state is confidential or that privacy guarantees have been independently audited.",
  },
  {
    title: "Source Note III: Economic Parameters",
    content: "Source defines a 1,000,000 $ULTRA genesis allocation and a 31,557,600-block halving interval. The reviewed code does not establish a 21,000,000 $ULTRA supply cap or recursive-SNARK enforcement of these economic rules.",
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
        throw new Error(payload?.message || "The node did not validate its current chain state.");
      }

      setVerificationState("verified");
      setVerificationMessage("The connected node reports its chain as valid. This check does not audit the statements on this page.");
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
          Repository source notes for current protocol claims. The chain-validation button checks the connected node&apos;s chain only; it does not verify these notes as signed law or audit.
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
        <h3 className="text-2xl font-bold uppercase">Connected Node Chain Check</h3>
        <p className="text-sm text-platinum/40 font-mono">
          GET /api/validate reports whether the connected node considers its current chain valid. It does not sign or certify the website&apos;s static source notes.
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
          {verificationState === "checking" ? "CHECKING_NODE_CHAIN..." : "CHECK_NODE_CHAIN.sh"}
        </button>
      </div>
    </main>
  );
}
