import Link from "next/link";
import { ArrowRight, KeyRound, Server, WalletCards } from "lucide-react";
import type { WalletStatusVariant } from "@/components/wallet/wallet.types";

interface EducationalWalletCalloutProps {
  variant: WalletStatusVariant;
  placement: "hub" | "article" | "docs" | "whitepaper" | "transact";
}

export default function EducationalWalletCallout({ variant, placement }: EducationalWalletCalloutProps) {
  const dark = variant === "dark";
  const compact = placement === "article" || placement === "docs" || placement === "whitepaper" || placement === "transact";

  return (
    <section
      aria-labelledby={`two-key-guidance-${placement}`}
      className={`${compact ? "p-5 sm:p-6" : "p-6 sm:p-8"} ${dark ? "border border-cyan-glow/25 bg-cyan-glow/[0.04] text-platinum" : "rounded-[2rem] border border-learn-violet/15 bg-learn-soft text-learn-ink"}`}
    >
      <div className="flex flex-col gap-5 lg:flex-row lg:items-start lg:justify-between lg:gap-8">
        <div className="min-w-0 max-w-3xl">
          <div className={`flex items-center gap-3 font-mono text-[10px] uppercase tracking-[0.2em] ${dark ? "text-cyan-glow" : "text-learn-violet"}`}>
            <KeyRound className="h-4 w-4" aria-hidden="true" />
            Two identities // one proposal
          </div>
          <h2 id={`two-key-guidance-${placement}`} className={`mt-3 font-space-grotesk text-2xl font-bold tracking-tight sm:text-3xl ${dark ? "text-platinum" : "text-learn-ink"}`}>
            The node key identifies the computer. Your wallet authorizes it.
          </h2>
          <p className={`mt-4 text-sm leading-7 ${dark ? "text-platinum/65" : "text-learn-muted"}`}>
            First export the public <code className={dark ? "text-platinum/90" : "text-learn-ink"}>DILITHIUM_PUB_KEY.hex</code> from the computer running <code className={dark ? "text-platinum/90" : "text-learn-ink"}>UltraNetNode.exe</code>. Then create or unlock your personal wallet at <code className={dark ? "text-platinum/90" : "text-learn-ink"}>/transact</code>. UltraWallet signs the proposal locally so the network knows you authorized that node identity.
          </p>
          <p className={`mt-3 text-xs leading-6 ${dark ? "text-platinum/45" : "text-learn-muted"}`}>
            The private node JSON, wallet secret, recovery phrase, and wallet password stay local. The website never stores your password and cannot bypass the wallet&apos;s security prompts. A same-site local session can remain unlocked in memory; reload, lock, timeout, or wallet policy can require unlock again.
          </p>
        </div>
        <div className={`grid shrink-0 gap-3 text-xs sm:grid-cols-2 lg:w-[23rem] lg:grid-cols-1 ${dark ? "text-platinum/65" : "text-learn-muted"}`}>
          <div className={`flex items-start gap-3 border p-4 ${dark ? "border-amber-200/20 bg-amber-200/[0.04]" : "border-learn-ink/10 bg-white/70"}`}>
            <Server className={`mt-0.5 h-4 w-4 shrink-0 ${dark ? "text-amber-200" : "text-learn-violet"}`} aria-hidden="true" />
            <span><strong className={dark ? "text-amber-100" : "text-learn-ink"}>Computer key:</strong> public node identity in the proposal; private JSON stays on the node computer.</span>
          </div>
          <div className={`flex items-start gap-3 border p-4 ${dark ? "border-cyan-glow/20 bg-cyan-glow/[0.03]" : "border-learn-ink/10 bg-white/70"}`}>
            <WalletCards className={`mt-0.5 h-4 w-4 shrink-0 ${dark ? "text-cyan-glow" : "text-learn-violet"}`} aria-hidden="true" />
            <span><strong className={dark ? "text-cyan-glow" : "text-learn-ink"}>Personal wallet:</strong> local signature proving that you approved the registration.</span>
          </div>
        </div>
      </div>
      <div className="mt-5 flex flex-wrap gap-x-5 gap-y-2">
        <Link href="/transact" className={`inline-flex min-h-11 items-center gap-2 font-mono text-[10px] font-bold uppercase tracking-[0.14em] focus:outline-hidden focus:ring-2 focus:ring-offset-2 ${dark ? "text-cyan-glow hover:text-white focus:ring-cyan-glow focus:ring-offset-ink-black" : "text-learn-violet hover:text-learn-ink focus:ring-learn-violet focus:ring-offset-learn-canvas"}`}>
          Create or unlock wallet <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
        </Link>
        <Link href="/validator" className={`inline-flex min-h-11 items-center gap-2 font-mono text-[10px] font-bold uppercase tracking-[0.14em] focus:outline-hidden focus:ring-2 focus:ring-offset-2 ${dark ? "text-platinum/55 hover:text-cyan-glow focus:ring-cyan-glow focus:ring-offset-ink-black" : "text-learn-muted hover:text-learn-violet focus:ring-learn-violet focus:ring-offset-learn-canvas"}`}>
          Read validator steps <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" />
        </Link>
      </div>
    </section>
  );
}
