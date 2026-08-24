import { Network, ShieldCheck } from "lucide-react";
import {
  GENESIS_API_PORT,
  GENESIS_MULTIADDR,
  GENESIS_P2P_PORT,
  GENESIS_TRANSPORTS,
} from "@/lib/validator";
import CopyMultiaddrButton from "@/components/validator/CopyMultiaddrButton";

export default function GenesisMultiaddr() {
  return (
    <section
      id="genesis-connection"
      aria-labelledby="genesis-connection-title"
      className="cut-corner neon-inset relative overflow-hidden p-6 sm:p-8"
    >
      <div className="absolute right-0 top-0 h-36 w-36 dot-grid opacity-30" aria-hidden="true" />
      <div className="relative z-10">
        <div className="flex items-start justify-between gap-6">
          <div>
            <p className="font-mono text-[10px] uppercase tracking-[0.22em] text-cyan-glow">Network // Genesis dial</p>
            <h2 id="genesis-connection-title" className="mt-3 font-space-grotesk text-2xl font-bold tracking-tight text-platinum sm:text-3xl">
              Connect to Genesis
            </h2>
          </div>
          <Network className="h-6 w-6 shrink-0 text-cyan-glow" aria-hidden="true" />
        </div>

        <p className="mt-4 max-w-2xl text-sm leading-relaxed text-platinum/60">
          This is the public P2P dial address for validator discovery and block gossip. It is not an HTTP API URL, and it does not contain a secret.
        </p>

        <div className="mt-6 border border-platinum/15 bg-ink-black/70 p-4 sm:p-5">
          <p className="font-mono text-[10px] uppercase tracking-[0.18em] text-platinum/40">GENESIS_MULTIADDR</p>
          <code className="mt-3 block break-all font-mono text-xs leading-6 text-cyan-glow sm:text-sm">{GENESIS_MULTIADDR}</code>
          <div className="mt-5">
            <CopyMultiaddrButton value={GENESIS_MULTIADDR} />
          </div>
        </div>

        <dl className="mt-6 grid gap-px border border-platinum/10 bg-platinum/10 sm:grid-cols-3">
          <div className="bg-ink-black/80 p-4">
            <dt className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/40">P2P port</dt>
            <dd className="mt-2 font-mono text-sm text-platinum/80">{GENESIS_TRANSPORTS.join(" + ")} {GENESIS_P2P_PORT}</dd>
          </div>
          <div className="bg-ink-black/80 p-4">
            <dt className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/40">API port</dt>
            <dd className="mt-2 font-mono text-sm text-platinum/80">TCP {GENESIS_API_PORT} private</dd>
          </div>
          <div className="bg-ink-black/80 p-4">
            <dt className="font-mono text-[10px] uppercase tracking-[0.16em] text-platinum/40">Handshake</dt>
            <dd className="mt-2 flex items-center gap-2 font-mono text-sm text-emerald-300"><ShieldCheck className="h-4 w-4" aria-hidden="true" /> libp2p</dd>
          </div>
        </dl>
      </div>
    </section>
  );
}
