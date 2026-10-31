import DocPage from "@/components/docs/DocPage";
import { Terminal } from "lucide-react";

export default function RealTimeFinalityDoc() {
  return (
    <DocPage
      pillar="R"
      icon={<Terminal className="w-6 h-6" />}
      title="Execution and Performance Evidence"
      subtitle="Implementation notes are not throughput, latency, or finality benchmarks. No measured TPS or proving duration is published by the current public stats endpoint."
      content={
        <div className="space-y-12">
          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">01. Current Public Measurements</h2>
            <p>
              The public stats endpoint currently exposes no TPS or proving-time measurement, and the current API does not publish a measured finality latency. No sub-second confirmation or throughput guarantee is asserted here. Check the live node snapshot for available measurements.
            </p>
          </section>

          <section className="bg-cyan-glow/5 border border-cyan-glow/10 p-8 rounded-sm">
            <h2 className="text-xl font-bold text-cyan-glow uppercase tracking-tight mb-4">02. Measurement Boundary</h2>
            <p className="mb-6">
              The node API returns <code>last_proving_time_ms: null</code> because it does not currently record elapsed proving time. The public response also contains no TPS value. Until reproducible, environment-scoped benchmarks are published, treat throughput and latency figures as unavailable.
            </p>
            <p className="text-sm leading-relaxed text-platinum/60">
              Block-STM and proof components exist in the repository, but component presence is not evidence of production throughput, end-to-end confirmation latency, or a service-level guarantee.
            </p>
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
