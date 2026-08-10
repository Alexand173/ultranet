import DocPage from "@/components/docs/DocPage";
import { Cpu } from "lucide-react";

export default function UnifiedScalabilityDoc() {
  return (
    <DocPage
      pillar="U"
      icon={<Cpu className="w-6 h-6" />}
      title="Unified Scalability"
      subtitle="Cross-shard orchestration and atomic block finality via recursive ZK-SNARKs."
      content={
        <div className="space-y-12">
          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">01. The Fragmentation Problem</h2>
            <p>
              Traditional multi-chain and side-chain architectures fragment liquidity and force developers to manage complex asynchronous bridges. 
              UltraNet solves this through <strong>Unified Sharding</strong>, where the network maintains a single logical state while executing transactions 
              across 16 parallel shards.
            </p>
          </section>

          <section className="bg-cyan-glow/5 border border-cyan-glow/10 p-8 rounded-sm">
            <h2 className="text-xl font-bold text-cyan-glow uppercase tracking-tight mb-4">02. Block-STM Parallelism</h2>
            <p className="mb-4">
              Our <strong>Block-STM (Software Transactional Memory)</strong> engine allows transactions to execute speculatively in parallel. 
              On modern multi-core hardware, this delivers near-linear speedups without requiring developers to pre-declare dependency sets.
            </p>
            <ul className="list-disc list-inside space-y-2 text-platinum/80 font-mono text-sm">
              <li>Optimistic execution across 16 logical shards</li>
              <li>Automated conflict detection and sequential re-execution</li>
              <li>Zero fragmentation: Single Merkle root commitment</li>
            </ul>
          </section>

          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">03. Recursive SNARK Compression</h2>
            <p>
              To ensure the chain remains light and verifiable for 100 years, UltraNet utilizes <strong>Recursive SNARKs</strong>. 
              Each new block includes a proof that attests to the validity of the previous block's proof plus the current state transition.
              This allows a new node to verify the entire history of the blockchain in constant time (ms) rather than re-playing years of transactions.
            </p>
          </section>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div className="border border-platinum/10 p-6">
              <div className="text-[10px] text-platinum/30 uppercase mb-2">Technical_Spec</div>
              <h4 className="text-sm font-bold text-platinum uppercase mb-4">Cross-Shard Atomicity</h4>
              <p className="text-xs text-platinum/60 leading-relaxed">
                Atomic messaging protocol ensures that if a transaction debits Shard A, it is guaranteed to credit Shard B 
                within the same block boundary, or both revert.
              </p>
            </div>
            <div className="border border-platinum/10 p-6">
              <div className="text-[10px] text-platinum/30 uppercase mb-2">Hardware_Target</div>
              <h4 className="text-sm font-bold text-platinum uppercase mb-4">Linear Throughput</h4>
              <p className="text-xs text-platinum/60 leading-relaxed">
                The protocol is designed to scale with CPU core count. A 64-core validator can process 4x the throughput 
                of a 16-core validator with no software changes.
              </p>
            </div>
          </div>
        </div>
      }
    />
  );
}
