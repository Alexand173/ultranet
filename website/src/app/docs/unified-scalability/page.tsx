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
              using 16 configured logical state shards; runtime parallelism and throughput depend on execution path and workload.
            </p>
          </section>

          <section className="bg-cyan-glow/5 border border-cyan-glow/10 p-8 rounded-sm">
            <h2 className="text-xl font-bold text-cyan-glow uppercase tracking-tight mb-4">02. Block-STM Parallelism</h2>
            <p className="mb-4">
              The repository contains a <strong>Block-STM (Software Transactional Memory)</strong> implementation for speculative parallel execution. Throughput and scaling depend on workload and hardware; this page does not publish a reproducible benchmark.
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
              The repository contains <strong>Recursive SNARK</strong> code and proof-history structures. The current source does not establish constant-time verification of the entire chain history or a 100-year verification guarantee; that behavior requires end-to-end validation and independent review.
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
              <div className="text-[10px] text-platinum/30 uppercase mb-2">Performance_Evidence</div>
              <h4 className="text-sm font-bold text-platinum uppercase mb-4">No Published Scaling Benchmark</h4>
              <p className="text-xs text-platinum/60 leading-relaxed">
                A 64-core versus 16-core throughput claim needs a reproducible workload, configuration, and measured results. No such benchmark is provided here.
              </p>
            </div>
          </div>
        </div>
      }
    />
  );
}
