import DocPage from "@/components/docs/DocPage";
import { Shield } from "lucide-react";

export default function TrustedExecutionDoc() {
  return (
    <DocPage
      pillar="T"
      icon={<Shield className="w-6 h-6" />}
      title="FHE Implementation Notes"
      subtitle="FHE-related code is present in the repository; this page does not claim production privacy, enclave isolation, or an independently audited deployment."
      content={
        <div className="space-y-12">
          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">01. Computational Privacy</h2>
            <p>
              <strong>Fully Homomorphic Encryption (FHE)</strong> can support computation over ciphertexts. The presence of FHE-related code does not by itself establish that UltraNet currently provides private on-chain state or end-to-end confidential transactions.
            </p>
          </section>

          <section className="bg-cyan-glow/5 border border-cyan-glow/10 p-8 rounded-sm">
            <h2 className="text-xl font-bold text-cyan-glow uppercase tracking-tight mb-4">02. TFHE-rs Engine</h2>
            <p className="mb-4 text-sm font-mono text-platinum/80 leading-relaxed">
              The repository includes TFHE-related code and Move runtime integration. Production support for confidential contracts, encrypted order books, front-running protection, or MEV resistance is not established by the current public telemetry.
            </p>
            <div className="flex gap-4 overflow-x-auto pb-2">
              <code className="text-[10px] bg-platinum/5 border border-platinum/10 p-2 whitespace-nowrap">compute_add(cipher_a, cipher_b)</code>
              <code className="text-[10px] bg-platinum/5 border border-platinum/10 p-2 whitespace-nowrap">compute_mul(cipher_a, cipher_b)</code>
              <code className="text-[10px] bg-platinum/5 border border-platinum/10 p-2 whitespace-nowrap">verify_stark(fhe_trace)</code>
            </div>
          </section>

          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">03. The Move Virtual Machine</h2>
            <p>
              The repository contains a Move VM adapter and FHE-related operations. Their execution and security properties depend on the implemented runtime path; this page does not treat them as an independently verified production guarantee.
            </p>
          </section>

          <div className="p-6 border border-cyan-glow/20 bg-[#0A1A1F]">
            <h4 className="text-sm font-bold text-cyan-glow uppercase mb-3 flex items-center gap-2">
              <div className="w-1.5 h-1.5 bg-cyan-glow" /> 
              API-Reported FHE Gas Multiplier: 5000x
            </h4>
            <p className="text-xs text-platinum/60 leading-relaxed">
              The current FHE stats endpoint reports a configured multiplier value of 5000. This API field alone does not demonstrate that the factor is applied across execution or establish measured hardware cost; verify the relevant runtime path before treating it as an economic rule.
            </p>
          </div>
        </div>
      }
    />
  );
}
