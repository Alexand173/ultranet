"use client";

import Link from "next/link";
import { ArrowLeft, ArrowRight, CheckCircle2 } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import type { LearnArticle as LearnArticleData } from "@/lib/learn-content";
import { LEARNING_TRACKS } from "@/lib/learn-content";
import LiveNetworkSnapshot from "@/components/learn/LiveNetworkSnapshot";
import EducationalWalletCallout from "@/components/wallet/EducationalWalletCallout";

function OnThisPage({ article }: { article: LearnArticleData }) {
  const [activeId, setActiveId] = useState(article.sections[0]?.id ?? "");

  useEffect(() => {
    const observer = new IntersectionObserver(
      (entries) => {
        const visible = entries.find((entry) => entry.isIntersecting);
        if (visible) setActiveId(visible.target.id);
      },
      { rootMargin: "-20% 0px -65%" },
    );
    const headings = article.sections.map((section) => document.getElementById(section.id)).filter(Boolean);
    headings.forEach((heading) => observer.observe(heading!));
    return () => observer.disconnect();
  }, [article.sections]);

  return (
    <details className="group rounded-2xl bg-learn-soft p-5 lg:sticky lg:top-28 lg:block" open>
      <summary className="cursor-pointer list-none font-space-grotesk font-bold marker:hidden">On this page</summary>
      <nav aria-label="On this page" className="mt-4 space-y-2 text-sm text-learn-muted">
        {article.sections.map((section, index) => (
          <a key={section.id} href={`#${section.id}`} className={`block rounded-md px-2 py-1 transition-colors hover:text-learn-violet ${activeId === section.id ? "font-semibold text-learn-violet" : ""}`}>
            {index + 1}. {section.title}
          </a>
        ))}
      </nav>
    </details>
  );
}

export default function LearnArticle({ article }: { article: LearnArticleData }) {
  const nextTrack = useMemo(() => {
    const index = LEARNING_TRACKS.findIndex((track) => track.slug === article.slug);
    return LEARNING_TRACKS[(index + 1) % LEARNING_TRACKS.length];
  }, [article.slug]);

  return (
    <main className="min-h-screen bg-learn-canvas text-learn-ink">
      <article className="mx-auto max-w-6xl px-5 pb-24 pt-28 sm:px-8 sm:pt-36">
        <Link href="/learn" className="inline-flex items-center gap-2 text-sm font-semibold text-learn-muted transition-colors hover:text-learn-violet focus:outline-none focus:ring-2 focus:ring-learn-violet focus:ring-offset-2">
          <ArrowLeft className="h-4 w-4" aria-hidden="true" /> Back to learning hub
        </Link>

        <div className="mt-10 grid gap-10 lg:grid-cols-[minmax(0,1fr)_260px]">
          <div>
            <header className="max-w-3xl border-b border-learn-ink/10 pb-10">
              <p className="text-sm font-semibold uppercase tracking-[0.2em] text-learn-violet">{article.eyebrow}</p>
              <h1 className="mt-5 font-space-grotesk text-5xl font-bold leading-[1] tracking-[-0.06em] sm:text-7xl">{article.title}</h1>
              <p className="mt-7 text-xl leading-relaxed text-learn-muted">{article.intro}</p>
              <p className="mt-5 text-sm font-medium text-learn-muted">{article.readTime} · Written for people meeting UltraNet for the first time</p>
            </header>

            <div className="mt-10">
              <EducationalWalletCallout variant="light" placement="article" />
            </div>

            <div className="mt-12 space-y-14">
              {article.sections.map((section) => (
                <section key={section.id} id={section.id} className="scroll-mt-28 space-y-5">
                  <h2 className="font-space-grotesk text-3xl font-bold tracking-tight sm:text-4xl">{section.title}</h2>
                  <div className="space-y-4 text-lg leading-relaxed text-learn-muted">
                    {section.paragraphs.map((paragraph) => <p key={paragraph}>{paragraph}</p>)}
                  </div>
                  {section.bullets && (
                    <ul className="grid gap-3 rounded-2xl bg-white p-6 text-base text-learn-ink shadow-[0_8px_24px_rgba(16,17,20,0.04)] sm:grid-cols-2">
                      {section.bullets.map((bullet) => <li key={bullet} className="flex gap-3"><CheckCircle2 className="mt-0.5 h-5 w-5 shrink-0 text-learn-violet" aria-hidden="true" />{bullet}</li>)}
                    </ul>
                  )}
                  {section.callout && <aside className="rounded-2xl border-l-4 border-learn-violet bg-learn-soft px-6 py-5 font-medium text-learn-ink">{section.callout}</aside>}
                </section>
              ))}
            </div>
          </div>

          <aside className="space-y-6">
            <OnThisPage article={article} />
            {article.slug === "network" && <LiveNetworkSnapshot compact />}
          </aside>
        </div>

        <nav aria-label="Continue learning" className="mt-20 grid gap-4 border-t border-learn-ink/10 pt-8 sm:grid-cols-2">
          <Link href="/learn" className="rounded-2xl border border-learn-ink/10 bg-white p-5 transition-colors hover:border-learn-violet/40 focus:outline-none focus:ring-2 focus:ring-learn-violet focus:ring-offset-2">
            <span className="text-sm text-learn-muted">All learning paths</span>
            <span className="mt-2 flex items-center gap-2 font-space-grotesk text-xl font-bold">Learning hub <ArrowLeft className="h-4 w-4" aria-hidden="true" /></span>
          </Link>
          <Link href={`/learn/${nextTrack.slug}`} className="rounded-2xl border border-learn-ink/10 bg-white p-5 text-right transition-colors hover:border-learn-violet/40 focus:outline-none focus:ring-2 focus:ring-learn-violet focus:ring-offset-2">
            <span className="text-sm text-learn-muted">Continue with</span>
            <span className="mt-2 flex items-center justify-end gap-2 font-space-grotesk text-xl font-bold">{nextTrack.title} <ArrowRight className="h-4 w-4" aria-hidden="true" /></span>
          </Link>
        </nav>
      </article>
    </main>
  );
}
