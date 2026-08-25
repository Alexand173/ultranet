"use client";

import Link from "next/link";
import { BookOpen, ChevronLeft, Download, ExternalLink, FileText } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import { motion } from "framer-motion";
import EducationalWalletCallout from "@/components/wallet/EducationalWalletCallout";

const WHITEPAPER_HTML = "/docs/ultranet-whitepaper.html";
const WHITEPAPER_PDF = "/docs/ultranet-whitepaper.pdf";

export default function WhitepaperReader() {
  const iframeRef = useRef<HTMLIFrameElement>(null);
  const [progress, setProgress] = useState(0);

  useEffect(() => {
    const iframe = iframeRef.current;
    if (!iframe) return;

    let detachScrollListener: (() => void) | undefined;

    const attachScrollListener = () => {
      const frameWindow = iframe.contentWindow;
      const document = iframe.contentDocument;
      if (!frameWindow || !document) return;

      const updateProgress = () => {
        const scrollableHeight = document.documentElement.scrollHeight - frameWindow.innerHeight;
        const nextProgress = scrollableHeight > 0
          ? (frameWindow.scrollY / scrollableHeight) * 100
          : 0;
        setProgress(Math.min(100, Math.max(0, nextProgress)));
      };

      frameWindow.addEventListener("scroll", updateProgress, { passive: true });
      updateProgress();
      return () => frameWindow.removeEventListener("scroll", updateProgress);
    };

    const handleLoad = () => {
      detachScrollListener?.();
      detachScrollListener = attachScrollListener();
    };

    const retryTimer = window.setInterval(() => {
      if (detachScrollListener) {
        window.clearInterval(retryTimer);
        return;
      }
      handleLoad();
    }, 100);
    iframe.addEventListener("load", handleLoad);
    handleLoad();

    return () => {
      window.clearInterval(retryTimer);
      iframe.removeEventListener("load", handleLoad);
      detachScrollListener?.();
    };
  }, []);

  return (
    <div className="relative min-h-screen overflow-x-clip bg-ink-black text-platinum selection:bg-cyan-glow/30 selection:text-white">
      <div className="fixed inset-0 pointer-events-none dot-grid opacity-20" />
      <div className="fixed inset-0 pointer-events-none bg-[radial-gradient(circle_at_50%_35%,rgba(15,255,255,0.06)_0%,transparent_58%)]" />

      <main className="relative mx-auto max-w-[1680px] px-4 pb-8 pt-24 sm:px-6 lg:px-8">
        <div className="mb-5 flex flex-col gap-4 border-b border-platinum/10 pb-5 lg:flex-row lg:items-center lg:justify-between">
          <Link
            href="/docs"
            className="group inline-flex items-center gap-2 self-start font-mono text-[10px] uppercase tracking-[0.24em] text-platinum/50 transition-colors hover:text-cyan-glow"
          >
            <ChevronLeft className="h-4 w-4 transition-transform group-hover:-translate-x-1" aria-hidden="true" />
            Back to_Documentation_Hub
          </Link>

          <div className="flex flex-wrap items-center gap-x-5 gap-y-2 font-mono text-[9px] uppercase tracking-[0.18em] text-platinum/35">
            <span className="text-cyan-glow/80">CANONICAL_WHITEPAPER</span>
            <span>v7.1_SOVEREIGN</span>
            <span>AUTHOR // VLADAN_JOTOV</span>
            <span>LICENSE // ISC</span>
          </div>
        </div>

        <motion.section
          initial={{ opacity: 0, y: 14 }}
          animate={{ opacity: 1, y: 0 }}
          transition={{ duration: 0.35 }}
          className="overflow-hidden rounded-sm border border-cyan-glow/20 bg-[#071722] shadow-[0_0_50px_rgba(15,255,255,0.08)]"
          aria-labelledby="whitepaper-reader-title"
        >
          <div className="border-b border-cyan-glow/15 bg-ink-black/80 px-4 py-4 backdrop-blur-md sm:px-6">
            <div className="flex flex-col gap-4 xl:flex-row xl:items-center xl:justify-between">
              <div className="flex min-w-0 items-center gap-3">
                <div className="flex h-10 w-10 shrink-0 items-center justify-center rounded-sm border border-cyan-glow/30 bg-cyan-glow/10 text-cyan-glow">
                  <BookOpen className="h-5 w-5" aria-hidden="true" />
                </div>
                <div className="min-w-0">
                  <div className="font-mono text-[9px] uppercase tracking-[0.28em] text-cyan-glow/65">
                    Protocol_Document // Reader_Mode
                  </div>
                  <h1 id="whitepaper-reader-title" className="truncate text-lg font-bold uppercase tracking-tight text-platinum sm:text-xl">
                    UltraNet Technical Whitepaper
                  </h1>
                </div>
              </div>

              <div className="flex flex-wrap items-center gap-2 sm:gap-3">
                <a
                  href={WHITEPAPER_HTML}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-2 border border-platinum/15 px-3 py-2 font-mono text-[9px] uppercase tracking-[0.16em] text-platinum/60 transition-colors hover:border-cyan-glow/50 hover:text-cyan-glow"
                >
                  <ExternalLink className="h-3.5 w-3.5" aria-hidden="true" />
                  Open_HTML
                </a>
                <a
                  href={WHITEPAPER_PDF}
                  target="_blank"
                  rel="noreferrer"
                  className="inline-flex items-center gap-2 border border-cyan-glow/50 bg-cyan-glow/10 px-3 py-2 font-mono text-[9px] uppercase tracking-[0.16em] text-cyan-glow transition-colors hover:bg-cyan-glow/20"
                >
                  <Download className="h-3.5 w-3.5" aria-hidden="true" />
                  PDF_Export
                </a>
              </div>
            </div>

            <div className="mt-4 grid gap-2 border-t border-platinum/10 pt-3 font-mono text-[9px] uppercase tracking-[0.16em] text-platinum/35 sm:grid-cols-[1fr_auto] sm:items-center">
              <div className="flex min-w-0 items-center gap-2">
                <FileText className="h-3.5 w-3.5 shrink-0 text-cyan-glow/70" aria-hidden="true" />
                <span className="truncate">Source // ULTRA_NET_TECHNICAL_GUIDE.md // 34_CHAPTERS // 9_DIAGRAMS</span>
              </div>
              <span className="text-cyan-glow/80" aria-live="polite">{Math.round(progress)}%_READ</span>
            </div>

            <div
              className="mt-3 h-0.5 overflow-hidden bg-platinum/10"
              role="progressbar"
              aria-label="Whitepaper reading progress"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(progress)}
            >
              <div
                className="h-full bg-cyan-glow shadow-[0_0_12px_rgba(15,255,255,0.8)] transition-[width] duration-150"
                style={{ width: `${progress}%` }}
              />
            </div>
          </div>

          <div className="bg-[#dfe5e8] p-1 sm:p-2">
            <iframe
              ref={iframeRef}
              title="UltraNet v7.1 Sovereign Technical Whitepaper"
              src={WHITEPAPER_HTML}
              className="h-[calc(100vh-14rem)] min-h-[560px] w-full border-0 bg-white sm:h-[calc(100vh-12rem)] sm:min-h-[680px]"
            />
          </div>
        </motion.section>

        <div className="mt-8">
          <EducationalWalletCallout variant="dark" placement="whitepaper" />
        </div>
      </main>
    </div>
  );
}
