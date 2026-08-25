import Link from "next/link";
import { ArrowRight, BookOpen, ExternalLink, ShieldCheck } from "lucide-react";
import { LEARNING_TRACKS } from "@/lib/learn-content";
import LearnIcon from "@/components/learn/LearnIcon";
import LiveNetworkSnapshot from "@/components/learn/LiveNetworkSnapshot";
import EducationalWalletCallout from "@/components/wallet/EducationalWalletCallout";

export default function LearnHub() {
  return (
    <main className="min-h-screen bg-learn-canvas text-learn-ink">
      <section className="mx-auto max-w-6xl px-5 pb-16 pt-28 sm:px-8 sm:pt-36">
        <div className="grid items-end gap-10 lg:grid-cols-[1.05fr_0.95fr]">
          <div>
            <p className="text-sm font-semibold uppercase tracking-[0.2em] text-learn-violet">UltraNet learning hub</p>
            <h1 className="mt-5 max-w-3xl font-space-grotesk text-5xl font-bold leading-[0.98] tracking-[-0.06em] sm:text-7xl">
              Understand the network before you use it.
            </h1>
            <p className="mt-7 max-w-2xl text-lg leading-relaxed text-learn-muted sm:text-xl">
              A plain-language guide to UltraNet: what it records, how validators keep it honest, and how to read the live network without confusing targets with real measurements.
            </p>
            <div className="mt-8 flex flex-wrap gap-3">
              <Link href="/learn/what-is-ultranet" className="inline-flex items-center gap-2 rounded-lg bg-learn-violet px-5 py-3 font-semibold text-white transition-transform hover:-translate-y-0.5 focus:outline-none focus:ring-2 focus:ring-learn-violet focus:ring-offset-2">
                Start with the basics <ArrowRight className="h-4 w-4" aria-hidden="true" />
              </Link>
              <Link href="/learn/network" className="inline-flex items-center gap-2 rounded-lg border border-learn-ink/20 bg-white px-5 py-3 font-semibold text-learn-ink transition-colors hover:border-learn-violet hover:text-learn-violet focus:outline-none focus:ring-2 focus:ring-learn-violet focus:ring-offset-2">
                See live metrics <ActivityIcon />
              </Link>
            </div>
          </div>
          <div className="rounded-[2rem] bg-learn-soft p-7 sm:p-9">
            <div className="flex items-start justify-between gap-4">
              <div>
                <p className="text-xs font-semibold uppercase tracking-[0.18em] text-learn-violet">The simple model</p>
                <h2 className="mt-3 font-space-grotesk text-3xl font-bold tracking-tight">Many nodes. One history.</h2>
              </div>
              <BookOpen className="h-7 w-7 shrink-0 text-learn-violet" aria-hidden="true" />
            </div>
            <div className="mt-8 space-y-3">
              {["Your wallet signs a request", "Validators verify and order it", "Execution changes shared state"].map((step, index) => (
                <div key={step} className="flex items-center gap-3 rounded-xl bg-white/75 px-4 py-3">
                  <span className="flex h-7 w-7 shrink-0 items-center justify-center rounded-full bg-learn-violet text-sm font-bold text-white">{index + 1}</span>
                  <span className="font-medium">{step}</span>
                </div>
              ))}
            </div>
          </div>
        </div>
      </section>

      <section className="mx-auto max-w-6xl px-5 pb-20 sm:px-8">
        <EducationalWalletCallout variant="light" placement="hub" />
      </section>

      <section className="mx-auto max-w-6xl px-5 pb-20 sm:px-8" aria-labelledby="learning-tracks-heading">
        <div className="flex flex-wrap items-end justify-between gap-4">
          <div>
            <p className="text-sm font-semibold uppercase tracking-[0.2em] text-learn-violet">Choose a path</p>
            <h2 id="learning-tracks-heading" className="mt-2 font-space-grotesk text-3xl font-bold tracking-tight sm:text-4xl">Five useful ways in</h2>
          </div>
          <p className="max-w-md text-sm leading-relaxed text-learn-muted">Short explanations first. Technical docs remain available when you want implementation detail.</p>
        </div>
        <div className="mt-8 grid gap-4 md:grid-cols-2 xl:grid-cols-3">
          {LEARNING_TRACKS.map((track) => (
            <Link key={track.slug} href={`/learn/${track.slug}`} className="group flex min-h-64 flex-col rounded-3xl border border-learn-ink/10 bg-white p-6 shadow-[0_8px_24px_rgba(16,17,20,0.04)] transition-all hover:-translate-y-1 hover:border-learn-violet/40 hover:shadow-[0_16px_35px_rgba(109,40,217,0.12)] focus:outline-none focus:ring-2 focus:ring-learn-violet focus:ring-offset-2">
              <div className="flex items-center justify-between">
                <span className="flex h-11 w-11 items-center justify-center rounded-2xl bg-learn-soft text-learn-violet"><LearnIcon name={track.icon} /></span>
                <ArrowRight className="h-5 w-5 text-learn-muted transition-transform group-hover:translate-x-1 group-hover:text-learn-violet" aria-hidden="true" />
              </div>
              <h3 className="mt-8 font-space-grotesk text-2xl font-bold tracking-tight">{track.title}</h3>
              <p className="mt-3 text-base leading-relaxed text-learn-muted">{track.description}</p>
            </Link>
          ))}
        </div>
      </section>

      <section className="mx-auto max-w-6xl px-5 pb-20 sm:px-8">
        <LiveNetworkSnapshot compact />
      </section>

      <section className="mx-auto max-w-6xl px-5 pb-24 sm:px-8">
        <div className="grid gap-4 rounded-[2rem] bg-learn-ink px-6 py-8 text-white sm:grid-cols-[1fr_auto] sm:items-center sm:px-10">
          <div>
            <p className="text-xs font-semibold uppercase tracking-[0.2em] text-cyan-glow">Already technical?</p>
            <h2 className="mt-3 font-space-grotesk text-3xl font-bold tracking-tight">Go deeper without losing the map.</h2>
            <p className="mt-3 max-w-2xl text-white/70">Read the canonical UltraNet technical whitepaper, validator guide, and protocol chapters after the beginner path.</p>
          </div>
          <Link href="/docs" className="inline-flex items-center justify-center gap-2 rounded-lg bg-white px-5 py-3 font-semibold text-learn-ink transition-colors hover:bg-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-learn-ink">
            Open technical docs <ExternalLink className="h-4 w-4" aria-hidden="true" />
          </Link>
        </div>
      </section>
    </main>
  );
}

function ActivityIcon() {
  return <ShieldCheck className="h-4 w-4" aria-hidden="true" />;
}
