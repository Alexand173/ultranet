import DocPage from "@/components/docs/DocPage";
import { Activity } from "lucide-react";

export default function Layer0FoundationDoc() {
  return (
    <DocPage
      pillar="L"
      icon={<Activity className="w-6 h-6" />}
      title="Layer 0 Foundation"
      subtitle="Secure p2p networking with sub-millisecond propagation and DAG consensus."
      content={
        <div className="space-y-12">
          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">01. Mysticeti & Bullshark Consensus</h2>
            <p>
              UltraNet utilizes a <strong>Directed Acyclic Graph (DAG)</strong> based consensus mechanism. Unlike linear chains where 
              one validator proposes a single block, our validators broadcast vertices simultaneously. This removes the "leader bottleneck" 
              and significantly reduces latency.
            </p>
          </section>

          <section className="bg-cyan-glow/5 border border-cyan-glow/10 p-8 rounded-sm">
            <h2 className="text-xl font-bold text-cyan-glow uppercase tracking-tight mb-4">02. Asynchronous Finality</h2>
            <p className="mb-4">
              The <strong>Bullshark</strong> ordering rule deterministically linearizes the DAG vertices into a canonical transaction sequence. 
              This happens asynchronously, meaning a vertex can be added to the graph and referenced by others before the entire network 
              has even seen it.
            </p>
            <div className="grid grid-cols-3 gap-4 text-center">
              <div className="p-4 border border-platinum/10">
                <div className="text-xl font-bold text-cyan-glow tracking-tighter">27.8µs</div>
                <div className="text-[9px] text-platinum/40 uppercase mt-1">Vertex Latency</div>
              </div>
              <div className="p-4 border border-platinum/10">
                <div className="text-xl font-bold text-cyan-glow tracking-tighter">100k+</div>
                <div className="text-[9px] text-platinum/40 uppercase mt-1">Target TPS</div>
              </div>
              <div className="p-4 border border-platinum/10">
                <div className="text-xl font-bold text-cyan-glow tracking-tighter">BFT</div>
                <div className="text-[9px] text-platinum/40 uppercase mt-1">Security Model</div>
              </div>
            </div>
          </section>

          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">03. The Ultra-Net Gossip Protocol</h2>
            <p>
              Built on <strong>libp2p</strong>, the networking layer uses a custom gossipsub configuration optimized for large payloads (Dilithium-5 signatures). 
              Nodes utilize mDNS for local discovery and Kademlia DHT for wide-area peer discovery, ensuring the network remains resilient 
              against partitioned attacks.
            </p>
          </section>

          <section className="border-l-2 border-platinum/10 pl-8 py-4">
            <h4 className="text-sm font-bold text-platinum uppercase mb-2">Post-Quantum Handshake</h4>
            <p className="text-sm text-platinum/60 leading-relaxed">
              Every peer connection is authenticated using Dilithium-5 signatures from the handshake phase, ensuring that 
              man-in-the-middle attacks via future quantum computers are mathematically impossible from day one.
            </p>
          </section>
        </div>
      }
    />
  );
}
