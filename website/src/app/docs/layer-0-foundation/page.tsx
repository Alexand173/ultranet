import DocPage from "@/components/docs/DocPage";
import { Activity } from "lucide-react";

export default function Layer0FoundationDoc() {
  return (
    <DocPage
      pillar="L"
      icon={<Activity className="w-6 h-6" />}
      title="Layer 0 Foundation"
      subtitle="Repository architecture and public manifest claims; no live peer-latency benchmark is exposed."
      content={
        <div className="space-y-12">
          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">01. Mysticeti & Bullshark Consensus</h2>
            <p>
              The repository contains a <strong>Directed Acyclic Graph (DAG)</strong>-based consensus implementation. This description reflects code structure; live propagation latency, multi-operator participation, and consensus safety have not been independently benchmarked here.
            </p>
          </section>

          <section className="bg-cyan-glow/5 border border-cyan-glow/10 p-8 rounded-sm">
            <h2 className="text-xl font-bold text-cyan-glow uppercase tracking-tight mb-4">02. Asynchronous Finality</h2>
            <p className="mb-4">
              The <strong>Bullshark</strong> ordering rule deterministically linearizes the DAG vertices into a canonical transaction sequence. 
              This happens asynchronously, meaning a vertex can be added to the graph and referenced by others before the entire network 
              has even seen it.
            </p>
            <div className="grid grid-cols-1 gap-4 text-center sm:grid-cols-3">
              <div className="p-4 border border-platinum/10">
                <div className="text-xl font-bold text-cyan-glow tracking-tighter">27.79µs</div>
                <div className="text-[9px] text-platinum/40 uppercase mt-1">Manifest field; unverified benchmark</div>
              </div>
              <div className="p-4 border border-platinum/10">
                <div className="text-xl font-bold text-cyan-glow tracking-tighter">Unavailable</div>
                <div className="text-[9px] text-platinum/40 uppercase mt-1">Measured TPS</div>
              </div>
              <div className="p-4 border border-platinum/10">
                <div className="text-xl font-bold text-cyan-glow tracking-tighter">16</div>
                <div className="text-[9px] text-platinum/40 uppercase mt-1">Configured state shards</div>
              </div>
            </div>
            <p className="mt-4 text-xs leading-relaxed text-platinum/45">
              The public manifest returns a configured latency string without method, workload, or timestamp. It is not a live measurement or independent consensus/security attestation.
            </p>
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
              The current P2P implementation uses persistent libp2p Ed25519 identities with Noise transport authentication. Dilithium-5 signatures are used for UltraNet transaction authorization; they are not the P2P handshake identity. No security claim is absolute, and the implementation has not been independently audited here.
            </p>
          </section>
        </div>
      }
    />
  );
}
