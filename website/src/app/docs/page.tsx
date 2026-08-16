"use client";

import { useState, useMemo } from "react";
import Link from "next/link";
import { motion, AnimatePresence } from "framer-motion";
import { BookOpen, Search, ChevronRight, ExternalLink } from "lucide-react";
import { CHAPTERS, getIcon, type Chapter } from "@/lib/constants";

export default function DocsHub() {
  const [search, setSearch] = useState("");
  const [activeCategory, setActiveCategory] = useState("All");

  const categories = useMemo(() => 
    ["All", ...Array.from(new Set(CHAPTERS.map(c => c.category)))], 
  []);

  const filteredChapters = useMemo(() => {
    return CHAPTERS.filter(c => {
      const matchesSearch = c.title.toLowerCase().includes(search.toLowerCase());
      const matchesCategory = activeCategory === "All" || c.category === activeCategory;
      return matchesSearch && matchesCategory;
    });
  }, [search, activeCategory]);

  return (
    <div className="min-h-screen overflow-x-clip bg-[#010B13] text-platinum font-sans selection:bg-cyan-glow/30 selection:text-white pb-20">
      {/* Background Grids */}
      <div className="fixed inset-0 dot-grid opacity-20 pointer-events-none" />
      <div className="fixed inset-0 bg-[radial-gradient(circle_at_50%_50%,rgba(15,255,255,0.03)_0%,transparent_70%)] pointer-events-none" />

      {/* Hero Header */}
      <section className="relative pt-24 pb-16 px-6 border-b border-platinum/10 bg-[#010B13]/40 backdrop-blur-sm">
        <div className="max-w-7xl mx-auto">
          <div className="flex items-center gap-4 mb-6">
            <Link href="/" className="text-cyan-glow hover:underline font-mono text-xs uppercase tracking-widest">
              &lt; Back to_Nexus
            </Link>
            <div className="h-px flex-1 bg-platinum/10" />
            <div className="flex items-center gap-2">
              <div className="w-2 h-2 bg-cyan-glow animate-pulse" />
              <span className="font-mono text-[10px] text-platinum/40 uppercase tracking-widest">
                System_Status // Online
              </span>
            </div>
          </div>

          <h1 className="text-5xl md:text-7xl font-bold tracking-tighter uppercase glitch-text mb-8" data-text="Documentation Hub">
            Documentation Hub
          </h1>
          <p className="mb-8 font-mono text-[10px] uppercase tracking-[0.22em] text-platinum/40">
            Created and maintained by Vladan Jotov // Original UltraNet protocol documentation // ISC_License
          </p>

          <div className="flex flex-col lg:flex-row gap-6 items-start lg:items-center justify-between">
            {/* Search Interface */}
            <div className="relative w-full lg:w-[480px] group">
              <Search className="absolute left-4 top-1/2 -translate-y-1/2 w-4 h-4 text-platinum/40 group-focus-within:text-cyan-glow transition-colors" />
              <input 
                type="text"
                placeholder="Search protocol chapters, subsystems, or constants..."
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                className="w-full bg-[#010B13]/80 border border-platinum/10 px-12 py-4 font-mono text-sm uppercase tracking-widest focus:outline-none focus:border-cyan-glow/50 transition-colors"
              />
              <div className="absolute right-4 top-1/2 -translate-y-1/2 font-mono text-[9px] text-platinum/20">
                [ ESC to clear ]
              </div>
            </div>

            {/* Category Filter */}
            <div className="flex flex-wrap gap-2">
              {categories.map(cat => (
                <button
                  key={cat}
                  onClick={() => setActiveCategory(cat)}
                  className={`px-4 py-2 font-mono text-[10px] uppercase tracking-widest border transition-all ${
                    activeCategory === cat 
                      ? "bg-cyan-glow/10 border-cyan-glow text-cyan-glow shadow-[0_0_15px_rgba(15,255,255,0.1)]" 
                      : "bg-platinum/5 border-platinum/10 text-platinum/40 hover:border-platinum/30 hover:text-platinum"
                  }`}
                >
                  {cat}
                </button>
              ))}
            </div>
          </div>
        </div>
      </section>

      {/* Canonical Whitepaper */}
      <section className="relative max-w-7xl mx-auto px-6 pt-12">
        <Link
          href="/docs/whitepaper"
          className="group block overflow-hidden border border-cyan-glow/25 bg-cyan-glow/[0.03] p-6 transition-colors hover:border-cyan-glow/60 hover:bg-cyan-glow/[0.06] sm:p-8"
        >
          <div className="absolute right-0 top-0 h-full w-1/3 bg-[radial-gradient(circle_at_75%_50%,rgba(15,255,255,0.12),transparent_65%)] opacity-70 transition-opacity group-hover:opacity-100" />
          <div className="relative flex flex-col gap-6 lg:flex-row lg:items-center lg:justify-between">
            <div className="flex items-start gap-4">
              <div className="flex h-12 w-12 shrink-0 items-center justify-center border border-cyan-glow/30 bg-cyan-glow/10 text-cyan-glow shadow-[0_0_20px_rgba(15,255,255,0.08)]">
                <BookOpen className="h-5 w-5" aria-hidden="true" />
              </div>
              <div>
                <div className="mb-2 font-mono text-[10px] uppercase tracking-[0.24em] text-cyan-glow/70">
                  Canonical_Source // v7.1_Sovereign
                </div>
                <h2 className="text-2xl font-bold uppercase tracking-tight text-platinum transition-colors group-hover:text-white sm:text-3xl">
                  Read the UltraNet Technical Whitepaper
                </h2>
                <p className="mt-3 max-w-3xl font-mono text-xs leading-relaxed text-platinum/45">
                  The complete 34-chapter protocol guide with rendered architecture diagrams, implementation references, and the source-of-truth genesis notice.
                </p>
              </div>
            </div>
            <div className="flex shrink-0 items-center gap-3 font-mono text-[10px] uppercase tracking-[0.18em] text-cyan-glow">
              Open_Reader
              <ExternalLink className="h-4 w-4 transition-transform group-hover:translate-x-1" aria-hidden="true" />
            </div>
          </div>
        </Link>
      </section>

      {/* Chapter Grid */}
      <main className="max-w-7xl mx-auto px-6 py-16">
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-px bg-platinum/10 border border-platinum/10 shadow-2xl overflow-hidden">
          <AnimatePresence mode="popLayout">
            {filteredChapters.map((chapter) => (
              <motion.div
                key={chapter.id}
                layout
                initial={{ opacity: 0, scale: 0.98 }}
                animate={{ opacity: 1, scale: 1 }}
                exit={{ opacity: 0, scale: 0.98 }}
                className="bg-[#010B13]/80 group relative"
              >
                {chapter.slug ? (
                  <Link href={`/docs/${chapter.slug}`} className="block p-8 h-full hover:bg-cyan-glow/[0.02] transition-colors relative">
                    <ChapterContent chapter={chapter} />
                  </Link>
                ) : (
                  <div className="p-8 h-full opacity-60 grayscale cursor-not-allowed">
                    <ChapterContent chapter={chapter} isComingSoon />
                  </div>
                )}
              </motion.div>
            ))}
          </AnimatePresence>
        </div>

        {filteredChapters.length === 0 && (
          <div className="py-32 text-center">
            <div className="font-mono text-xs text-platinum/20 uppercase tracking-[0.4em] mb-4">
              [ 0_MATCHES_FOUND ]
            </div>
            <p className="text-platinum/40 text-sm">No chapters found matching &quot;{search}&quot; in {activeCategory}.</p>
          </div>
        )}
      </main>

      {/* Stats Bar */}
      <div className="fixed bottom-0 left-0 right-0 z-50 flex items-center justify-between overflow-hidden border-t border-platinum/10 bg-[#010B13] px-4 py-3 sm:px-8">
        <div className="flex min-w-0 gap-4 sm:gap-8">
          <div className="flex items-center gap-2">
            <span className="text-[9px] text-platinum/30 uppercase font-mono tracking-widest">Chapters:</span>
            <span className="text-[10px] text-cyan-glow font-mono">34/34</span>
          </div>
          <div className="flex items-center gap-2">
            <span className="text-[9px] text-platinum/30 uppercase font-mono tracking-widest">Protocol:</span>
            <span className="text-[10px] text-cyan-glow font-mono">v7.1_Sovereign</span>
          </div>
        </div>
        <div className="hidden min-w-0 max-w-[45%] truncate font-mono text-[9px] text-platinum/20 uppercase tracking-widest sm:block">
          Sovereign_Genesis_Authority // Encrypted_Ref_0x442A
        </div>
      </div>
    </div>
  );
}

function ChapterContent({ chapter, isComingSoon }: { chapter: Chapter; isComingSoon?: boolean }) {
  return (
    <>
      <div className="flex justify-between items-start mb-6">
        <div className="p-2.5 bg-platinum/5 border border-platinum/10 rounded-sm text-platinum/50 group-hover:text-cyan-glow group-hover:border-cyan-glow/30 transition-colors">
          {getIcon(chapter.iconName)}
        </div>
        <span className="font-mono text-[10px] text-platinum/20">
          CH_{chapter.id.toString().padStart(2, '0')}
        </span>
      </div>

      <h3 className="text-lg font-bold text-platinum uppercase tracking-tighter mb-2 group-hover:text-white transition-colors">
        {chapter.title}
      </h3>
      
      <div className="flex items-center justify-between mt-auto">
        <span className="font-mono text-[9px] text-platinum/30 uppercase tracking-widest px-2 py-0.5 border border-platinum/10">
          {chapter.category}
        </span>
        {!isComingSoon && (
          <ChevronRight className="w-4 h-4 text-cyan-glow opacity-0 group-hover:opacity-100 group-hover:translate-x-1 transition-all" />
        )}
        {isComingSoon && (
          <span className="font-mono text-[8px] text-platinum/20 uppercase tracking-[0.2em] italic">
            [ Encrypted ]
          </span>
        )}
      </div>

      {/* Hover Line */}
      <div className="absolute bottom-0 left-0 h-[1px] bg-cyan-glow w-0 group-hover:w-full transition-all duration-300 shadow-[0_0_8px_rgba(15,255,255,0.8)]" />
    </>
  );
}
