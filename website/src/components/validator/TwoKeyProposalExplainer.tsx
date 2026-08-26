import Link from "next/link";
import { ArrowDown, ArrowRight, KeyRound, Server, ShieldCheck, WalletCards } from "lucide-react";
import { getSendUltraHref, VALIDATOR_PROPOSAL_PATH } from "@/lib/links";
import { WINDOWS_VALIDATOR_PRIVATE_KEY_PATH } from "@/lib/validator";

interface KeyRoleCardProps {
  icon: typeof Server;
  eyebrow: string;
  title: string;
  description: string;
  artifacts: readonly string[];
  privatePath?: string;
  tone: "amber" | "cyan";
}

const ROLE_CARDS: readonly KeyRoleCardProps[] = [
  {
    icon: Server,
    eyebrow: "01 // Node / computer identity",
    title: "The computer being proposed",
    description: "Run UltraNetNode.exe on the computer or VPS. It creates or loads one stable local validator identity and lets you export its public half.",
    artifacts: [
      "DILITHIUM_PUB_KEY.hex — public key exported for this proposal",
      "validator_dilithium5_key.json — private node identity; never paste it",
    ],
    privatePath: WINDOWS_VALIDATOR_PRIVATE_KEY_PATH,
    tone: "amber",
  },
  {
    icon: WalletCards,
    eyebrow: "02 // You / personal wallet",
    title: "The person authorizing it",
    description: "Create or unlock your personal UltraWallet in Send Ultra. The wallet signs the complete proposal locally; the website receives only public signed fields.",
    artifacts: [
      "12-word recovery phrase — offline backup",
      "Wallet password — unlocks this browser wallet",
    ],
    tone: "cyan",
  },
];

function KeyRoleCard({ icon: Icon, eyebrow, title, description, artifacts, privatePath, tone }: KeyRoleCardProps) {
  const cyan = tone === "cyan";
  return (
    <article className={`min-w-0 border p-5 sm:p-6 ${cyan ? "border-cyan-glow/25 bg-cyan-glow/[0.04]" : "border-amber-200/25 bg-amber-200/[0.04]"}`}>
      <div className={`flex items-start gap-3 ${cyan ? "text-cyan-glow" : "text-amber-200"}`}>
        <Icon className="mt-0.5 h-5 w-5 shrink-0" aria-hidden="true" />
        <div className="min-w-0">
          <p className="font-mono text-[10px] uppercase tracking-[0.18em]">{eyebrow}</p>
          <h3 className="mt-2 font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">{title}</h3>
        </div>
      </div>
      <p className="mt-4 text-sm leading-7 text-platinum/65">{description}</p>
      <ul className="mt-5 space-y-3 border-t border-platinum/10 pt-4 font-mono text-[10px] leading-6 text-platinum/60">
        {artifacts.map((artifact) => <li key={artifact} className="break-words">{artifact}</li>)}
      </ul>
      {privatePath && (
        <div className="mt-4 border-t border-amber-200/15 pt-4">
          <p className="font-mono text-[9px] uppercase tracking-[0.14em] text-amber-200/75">Windows default private file</p>
          <code className="mt-2 block break-all font-mono text-[10px] leading-5 text-amber-100/80">{privatePath}</code>
        </div>
      )}
    </article>
  );
}

export default function TwoKeyProposalExplainer({ className = "" }: { className?: string }) {
  return (
    <section aria-labelledby="two-key-proposal-title" className={`relative border border-platinum/10 bg-platinum/[0.025] p-5 sm:p-7 ${className}`}>
      <div className="pointer-events-none absolute right-0 top-0 h-32 w-32 dot-grid opacity-20" aria-hidden="true" />
      <div className="relative z-10">
        <div className="flex flex-col gap-4 sm:flex-row sm:items-end sm:justify-between">
          <div>
            <div className="flex items-center gap-3 text-cyan-glow">
              <KeyRound className="h-5 w-5" aria-hidden="true" />
              <p className="font-mono text-[10px] uppercase tracking-[0.2em]">Proposal identity map</p>
            </div>
            <h2 id="two-key-proposal-title" className="mt-3 font-space-grotesk text-2xl font-bold tracking-tight text-platinum sm:text-3xl">Two keys. One signed proposal.</h2>
          </div>
          <p className="max-w-xs font-mono text-[10px] uppercase leading-5 tracking-[0.12em] text-platinum/35">The node says which computer. The wallet says who approved it.</p>
        </div>

        <div className="mt-7 grid gap-3 lg:grid-cols-[minmax(0,1fr)_auto_minmax(0,1fr)] lg:items-stretch">
          <KeyRoleCard {...ROLE_CARDS[0]} />
          <div className="flex items-center justify-center py-1 text-cyan-glow/65 lg:px-1">
            <ArrowRight className="hidden h-5 w-5 lg:block" aria-hidden="true" />
            <ArrowDown className="h-5 w-5 lg:hidden" aria-hidden="true" />
          </div>
          <KeyRoleCard {...ROLE_CARDS[1]} />
        </div>

        <div className="mx-auto mt-3 flex justify-center text-cyan-glow/65 lg:mt-4">
          <ArrowDown className="h-5 w-5" aria-hidden="true" />
        </div>
        <div className="mt-3 border border-cyan-glow/35 bg-cyan-glow/[0.06] p-5 text-center sm:p-6">
          <ShieldCheck className="mx-auto h-6 w-6 text-cyan-glow" aria-hidden="true" />
          <h3 className="mt-3 font-space-grotesk text-xl font-bold uppercase tracking-tight text-platinum">Signed validator proposal</h3>
          <p className="mx-auto mt-2 max-w-2xl text-sm leading-6 text-platinum/60">Paste the public node key into the form, connect or unlock the personal wallet in this browser, and let it sign. A successful submission enters governance; 2-of-3 Sovereign approval is still required before activation.</p>
          <div className="mt-4 flex flex-wrap justify-center gap-4 font-mono text-[10px] uppercase tracking-[0.14em]">
            <Link href={getSendUltraHref(VALIDATOR_PROPOSAL_PATH)} className="inline-flex min-h-11 items-center gap-2 text-cyan-glow hover:text-white focus:outline-hidden focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">Open Send Ultra <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" /></Link>
            <Link href="/#swarm" className="inline-flex min-h-11 items-center gap-2 text-platinum/55 hover:text-cyan-glow focus:outline-hidden focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">Open proposal form <ArrowRight className="h-3.5 w-3.5" aria-hidden="true" /></Link>
          </div>
        </div>
      </div>
    </section>
  );
}
