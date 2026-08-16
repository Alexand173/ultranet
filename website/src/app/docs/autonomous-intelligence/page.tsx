import DocPage from "@/components/docs/DocPage";
import { Brain } from "lucide-react";

export default function AutonomousIntelligenceDoc() {
  return (
    <DocPage
      pillar="A"
      icon={<Brain className="w-6 h-6" />}
      title="Autonomous Intelligence"
      subtitle="On-chain LLM inference and decentralized agentic governance modules."
      content={
        <div className="space-y-12">
          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">01. The AI Governor</h2>
            <p>
              UltraNet is the first blockchain with a <strong>Self-Regulating Economy</strong>. The AI Governor is an on-chain 
              module that analyzes network health metrics (congestion, stale block rates, staking ratio) and automatically 
              tunes the mining difficulty and token emission rates to maintain long-term stability.
            </p>
          </section>

          <section className="bg-cyan-glow/5 border border-cyan-glow/10 p-8 rounded-sm">
            <h2 className="text-xl font-bold text-cyan-glow uppercase tracking-tight mb-4">02. Sustainability Score</h2>
            <p className="mb-4">
              The Governor computes a <strong>Sustainability Score (0-100)</strong> every epoch. This score determines the 
              multiplier for validator rewards.
            </p>
            <div className="flex items-center gap-2 mb-8">
              <div className="flex-1 h-1 bg-platinum/10 rounded-full overflow-hidden">
                <div className="w-[88%] h-full bg-cyan-glow shadow-[0_0_10px_rgba(15,255,255,0.5)]" />
              </div>
              <span className="font-mono text-cyan-glow text-sm">88.4 / 100</span>
            </div>
            <ul className="grid grid-cols-2 gap-4 font-mono text-[10px] text-platinum/50 uppercase">
              <li>- Network_Health: Optimal</li>
              <li>- Stake_Ratio: 64.2%</li>
              <li>- Inflation_Target: 1.5%</li>
              <li>- Agentic_Bias: 0.02</li>
            </ul>
          </section>

          <section>
            <h2 className="text-2xl font-bold text-platinum uppercase tracking-tight mb-6">03. Agentic Governance</h2>
            <p>
              While the Sovereign Genesis Authority oversees the initial bootstrap phase, UltraNet is designed for 
              <strong>Agentic Governance</strong>. Decentralized AI agents, holding $ULTRA, can propose and vote 
              on protocol improvements, ensuring that the network can evolve at machine speed without human bottlenecks.
            </p>
          </section>

          <div className="p-6 border border-platinum/10 font-mono text-center">
            <div className="text-[10px] text-platinum/30 uppercase mb-4 tracking-[0.3em]">Governance_Model</div>
            <h4 className="text-sm font-bold text-platinum uppercase mb-2">Hybrid Sovereignty</h4>
            <p className="text-xs text-platinum/60 leading-relaxed max-w-xl mx-auto">
              A unique 2-of-3 multi-signature override remains active for emergency interventions, 
              providing a &quot;circuit breaker&quot; for the autonomous governor in unforeseen black-swan events.
            </p>
          </div>
        </div>
      }
    />
  );
}
