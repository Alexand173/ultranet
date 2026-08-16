import DocPage from "@/components/docs/DocPage";
import { Terminal } from "lucide-react";

export default function RealTimeFinalityDoc() {
  return (
    <DocPage
      pillar="R"
      icon={<Terminal className="w-6 h-6" />}
      title="Real-time Finality"
      subtitle="Block-STM parallel execution engine delivering 100k+ TPS on commodity hardware."
      content={
        <div className="space-y-12">
          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">01. Sub-Second Confirmations</h2>
            <p>
              Latency is the enemy of adoption. UltraNet is engineered for <strong>Real-time Finality</strong>, with 
              confirmations typically occurring in under 600ms. This is achieved by pipelining the execution, 
              validation, and commitment phases of the block lifecycle.
            </p>
          </section>

          <section className="bg-cyan-glow/5 border border-cyan-glow/10 p-8 rounded-sm">
            <h2 className="text-xl font-bold text-cyan-glow uppercase tracking-tight mb-4">02. Optimized Proving Pipeline</h2>
            <p className="mb-6">
              The <strong>506ms Proving Time</strong> seen on our dashboard isn&apos;t just a number—it&apos;s the result of
              aggressive hardware-aware optimization of our ZK and FHE stacks.
            </p>
            <div className="space-y-4">
              <div className="flex items-center justify-between border-b border-platinum/10 pb-2">
                <span className="text-xs font-mono text-platinum/50 uppercase">Mempool Admission</span>
                <span className="text-xs font-mono text-cyan-glow">&lt; 10ms</span>
              </div>
              <div className="flex items-center justify-between border-b border-platinum/10 pb-2">
                <span className="text-xs font-mono text-platinum/50 uppercase">Block-STM Execution</span>
                <span className="text-xs font-mono text-cyan-glow">~ 40ms</span>
              </div>
              <div className="flex items-center justify-between border-b border-platinum/10 pb-2">
                <span className="text-xs font-mono text-platinum/50 uppercase">ZK/FHE Finalization</span>
                <span className="text-xs font-mono text-cyan-glow">~ 500ms</span>
              </div>
            </div>
          </section>

          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">03. Sharded Merkle Patricia Trie</h2>
            <p>
              State storage is often the primary bottleneck for TPS. UltraNet&apos;s <strong>Sharded MPT</strong> partitions
              the entire global state into 16 independent database shards (backed by <em>Sled</em>). This allows
              concurrent disk I/O, ensuring that state-heavy transactions don&apos;t block the rest of the network.
            </p>
          </section>

          <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
            <div className="p-6 border border-platinum/10 font-mono">
              <div className="text-[10px] text-platinum/30 uppercase mb-3">State_Pruning</div>
              <p className="text-xs text-platinum/70">
                Automatic mark-and-sweep garbage collection ensures that nodes remain lean. Old state roots are pruned 
                periodically while maintaining the integrity of the latest recursive SNARK.
              </p>
            </div>
            <div className="p-6 border border-platinum/10 font-mono">
              <div className="text-[10px] text-platinum/30 uppercase mb-3">Hot_Account_Triage</div>
              <p className="text-xs text-platinum/70">
                The Block-STM engine identifies accounts with high contention and automatically prioritizes them in 
                the next block pass to minimize retry latency.
              </p>
            </div>
          </div>
        </div>
      }
    />
  );
}
