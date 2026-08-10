import DocPage from "@/components/docs/DocPage";
import { Shield } from "lucide-react";

export default function TrustedExecutionDoc() {
  return (
    <DocPage
      pillar="T"
      icon={<Shield className="w-6 h-6" />}
      title="Trusted Execution"
      subtitle="Fully Homomorphic Encryption (FHE) enclaves for private on-chain state."
      content={
        <div className="space-y-12">
          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">01. Computational Privacy</h2>
            <p>
              In UltraNet, privacy is not just about hiding transaction amounts; it's about <strong>Confidential Computing</strong>. 
              While Zero-Knowledge (ZK) proofs allow you to prove a statement about hidden data, <strong>Fully Homomorphic Encryption (FHE)</strong> 
              allows the network to <em>perform math</em> on that data without ever seeing the numbers.
            </p>
          </section>

          <section className="bg-cyan-glow/5 border border-cyan-glow/10 p-8 rounded-sm">
            <h2 className="text-xl font-bold text-cyan-glow uppercase tracking-tight mb-4">02. TFHE-rs Engine</h2>
            <p className="mb-4 text-sm font-mono text-platinum/80 leading-relaxed">
              We integrate the <strong>Zama TFHE-rs</strong> library into the Move Virtual Machine. This enables a new class of 
              confidential smart contracts. Imagine a decentralized exchange (DEX) where the order book is encrypted, 
              preventing front-running and MEV.
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
              UltraNet uses the <strong>Move VM</strong> for its resource-oriented safety. We have extended the Move runtime with 
              native FHE instructions. This ensures that assets are physically represented as "Resources" that cannot be 
              duplicated or dropped, even when their underlying values are encrypted ciphertexts.
            </p>
          </section>

          <div className="p-6 border border-cyan-glow/20 bg-[#0A1A1F]">
            <h4 className="text-sm font-bold text-cyan-glow uppercase mb-3 flex items-center gap-2">
              <div className="w-1.5 h-1.5 bg-cyan-glow" /> 
              FHE Gas Multiplier: 5000x
            </h4>
            <p className="text-xs text-platinum/60 leading-relaxed">
              FHE operations are computationally heavy. Our economic model applies a 5000x gas multiplier to homomorphic operations 
              to reflect the real hardware cost (Programmable Bootstrapping) and maintain network stability.
            </p>
          </div>
        </div>
      }
    />
  );
}
