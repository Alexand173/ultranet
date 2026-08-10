import Link from "next/link";
import { ArrowRight, Blocks, BookOpen, ShieldCheck } from "lucide-react";

const PATHS = [
  {
    href: "/learn/what-is-ultranet",
    label: "Start here",
    title: "What is UltraNet?",
    description: "Learn the shared-ledger idea without protocol jargon.",
    icon: BookOpen,
  },
  {
    href: "/learn/how-it-works",
    label: "Follow a transaction",
    title: "How it works",
    description: "See how wallets, validators, consensus, and execution connect.",
    icon: Blocks,
  },
  {
    href: "/learn/validators",
    label: "Operate the network",
    title: "Become a validator",
    description: "Understand the real job before you run infrastructure.",
    icon: ShieldCheck,
  },
] as const;

export default function HomeLearningIntro({ className }: { className?: string }) {
  return (
    <section className={`relative flex min-h-[85vh] flex-col justify-center overflow-hidden bg-[#010B13] px-6 py-16 sm:px-10 2xl:min-h-0 ${className ?? ""}`} aria-labelledby="home-learning-title">
      <div className="absolute inset-0 dot-grid opacity-25" aria-hidden="true" />
      <div className="absolute -right-32 top-1/4 h-96 w-96 rounded-full bg-cyan-glow/10 blur-3xl" aria-hidden="true" />
      <div className="relative z-10 mx-auto w-full max-w-3xl">
        <p className="font-mono text-xs uppercase tracking-[0.24em] text-cyan-glow">A human guide to UltraNet</p>
        <h1 id="home-learning-title" className="mt-5 max-w-2xl font-space-grotesk text-4xl font-bold leading-[0.98] tracking-[-0.05em] text-platinum sm:text-6xl">
          A network you can actually understand.
        </h1>
        <p className="mt-6 max-w-2xl text-base leading-relaxed text-platinum/60 sm:text-lg">
          Start with the simple idea, then follow the evidence: what validators do, how state changes, and which numbers the live node can really report.
        </p>

        <div className="mt-10 grid gap-3 sm:grid-cols-3">
          {PATHS.map(({ href, label, title, description, icon: Icon }) => (
            <Link key={href} href={href} className="group flex min-h-44 flex-col border border-platinum/10 bg-platinum/[0.04] p-4 transition-all hover:-translate-y-1 hover:border-cyan-glow/50 hover:bg-cyan-glow/[0.06] focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
              <div className="flex items-center justify-between text-cyan-glow">
                <Icon className="h-5 w-5" aria-hidden="true" />
                <ArrowRight className="h-4 w-4 transition-transform group-hover:translate-x-1" aria-hidden="true" />
              </div>
              <span className="mt-7 font-mono text-[9px] uppercase tracking-[0.16em] text-platinum/35">{label}</span>
              <h2 className="mt-2 font-space-grotesk text-lg font-bold text-platinum">{title}</h2>
              <p className="mt-2 text-xs leading-relaxed text-platinum/50">{description}</p>
            </Link>
          ))}
        </div>

        <Link href="/learn" className="mt-8 inline-flex items-center gap-2 font-mono text-xs uppercase tracking-[0.18em] text-cyan-glow hover:text-white focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black">
          Open the learning hub <ArrowRight className="h-4 w-4" aria-hidden="true" />
        </Link>
      </div>
    </section>
  );
}
