"use client";

import { motion, useMotionValue, useSpring, useTransform, AnimatePresence } from "framer-motion";
import { useEffect, useState, useRef } from "react";
import { Sparkles } from "lucide-react";
import { clsx } from "clsx";
import Image from "next/image";
import Link from "next/link";

const SCRAMBLE_CHARS = "01$!@#%^&*()_+{}:<>?[]";
const COIN_IMAGE = "/images/ultra-coin.png";
const NEXUS_TITLE = "SOVEREIGN NEXUS";

const SPARKLE_POSITIONS = [
  { top: "4%", left: "8%", delay: 0, size: 30 },
  { top: "12%", left: "88%", delay: 1.1, size: 22 },
  { top: "84%", left: "90%", delay: 2.1, size: 26 },
  { top: "90%", left: "12%", delay: 0.6, size: 20 },
];

const LETTER_CONTENT: Record<string, { title: string; desc: string; slug: string }> = {
  U: { title: "Unified_Scalability", desc: "Cross-shard orchestration and atomic block finality via recursive ZK-SNARKs.", slug: "unified-scalability" },
  L: { title: "Layer_0_Foundation", desc: "Secure p2p networking with sub-millisecond propagation and DAG consensus.", slug: "layer-0-foundation" },
  T: { title: "Trusted_Execution", desc: "Fully Homomorphic Encryption (FHE) enclaves for private on-chain state.", slug: "trusted-execution" },
  R: { title: "Real-time_Finality", desc: "Block-STM parallel execution engine delivering 100k+ TPS on commodity hardware.", slug: "real-time-finality" },
  A: { title: "Autonomous_Intelligence", desc: "On-chain LLM inference and decentralized agentic governance modules.", slug: "autonomous-intelligence" },
};

export default function CoinNexus({ className }: { className?: string }) {
  const [scrambledText, setScrambledText] = useState(NEXUS_TITLE);
  const [hoveredLetter, setHoveredLetter] = useState<string | null>(null);
  const coinRef = useRef<HTMLDivElement>(null);

  const mouseX = useMotionValue(0);
  const mouseY = useMotionValue(0);
  const springX = useSpring(mouseX, { stiffness: 100, damping: 30 });
  const springY = useSpring(mouseY, { stiffness: 100, damping: 30 });

  const rotateX = useTransform(springY, [-0.5, 0.5], [12, -12]);
  const rotateY = useTransform(springX, [-0.5, 0.5], [-12, 12]);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      const x = e.clientX / window.innerWidth - 0.5;
      const y = e.clientY / window.innerHeight - 0.5;
      mouseX.set(x);
      mouseY.set(y);

      if (coinRef.current) {
        const rect = coinRef.current.getBoundingClientRect();
        const maskX = e.clientX - rect.left;
        const maskY = e.clientY - rect.top;
        coinRef.current.style.setProperty("--mask-x", `${maskX}px`);
        coinRef.current.style.setProperty("--mask-y", `${maskY}px`);
      }
    };
    window.addEventListener("mousemove", handleMouseMove);

    let iteration = 0;
    const scrambleInterval = setInterval(() => {
      setScrambledText(() =>
        NEXUS_TITLE.split("")
          .map((char, index) => {
            if (char === " ") return " ";
            if (index < iteration) return char;
            return SCRAMBLE_CHARS[Math.floor(Math.random() * SCRAMBLE_CHARS.length)];
          })
          .join("")
      );
      if (iteration >= NEXUS_TITLE.length) clearInterval(scrambleInterval);
      iteration += 1 / 2;
    }, 30);

    return () => {
      window.removeEventListener("mousemove", handleMouseMove);
      clearInterval(scrambleInterval);
    };
  }, [mouseX, mouseY]);

  return (
    <div className={`relative flex flex-col items-center justify-center crosshair-lines dot-grid overflow-hidden ${className ?? ""}`}>
      {/* Corner brackets spanning the nexus column */}
      <div className="absolute top-3 left-3 w-8 h-8 border-t border-l border-cyan-glow/15 pointer-events-none z-10" />
      <div className="absolute bottom-3 right-3 w-8 h-8 border-b border-r border-cyan-glow/15 pointer-events-none z-10" />

      <motion.p
        initial={{ opacity: 0, y: -8 }}
        animate={{ opacity: 1, y: 0 }}
        className="text-cyan-glow font-mono text-xs md:text-sm tracking-[0.35em] uppercase opacity-70 z-10"
      >
        {"// Sovereign Execution Layer v7.1"}
      </motion.p>

      <h1
        className="text-2xl md:text-3xl lg:text-4xl 2xl:text-5xl font-bold text-platinum glitch-text uppercase tracking-tighter z-10 mt-2"
        data-text={scrambledText}
      >
        {scrambledText}
      </h1>

      {/* Centerpiece Nexus */}
      <motion.div
        style={{ rotateX, rotateY }}
        className="relative w-[320px] h-[320px] sm:w-[420px] sm:h-[420px] lg:w-[620px] lg:h-[620px] 2xl:w-[clamp(640px,50vw,1140px)] 2xl:h-[clamp(640px,50vw,1140px)] flex items-center justify-center perspective-[1000px] my-6 shrink-0 pointer-events-none"
      >
        <div
          ref={coinRef}
          className="relative w-full h-full rounded-full bg-ink-black pointer-events-auto group cursor-pointer"
          style={{ clipPath: "circle(48.2% at 50% 50%)" }}
        >
          <Image
            src={COIN_IMAGE}
            alt="ULTRA Blockchain Network"
            fill
            sizes="(min-width: 1536px) 50vw, (min-width: 1024px) 620px, 420px"
            className="absolute inset-0 z-10 w-full h-full object-contain"
          />

          <div
            className="absolute inset-0 z-30 pointer-events-none mix-blend-overlay opacity-0 group-hover:opacity-70 transition-opacity duration-300"
            style={{ background: "radial-gradient(circle 350px at var(--mask-x) var(--mask-y), rgba(15, 255, 255, 0.4) 0%, transparent 70%)" }}
          />

          {/* Interactive Letter Hotzones — Positioned over the central "ULTRA" text */}
          {["U", "L", "T", "R", "A"].map((l, i) => (
            <Link
              key={l}
              href={`/docs/${LETTER_CONTENT[l].slug}`}
              onMouseEnter={() => setHoveredLetter(l)}
              onMouseLeave={() => setHoveredLetter(null)}
              onFocus={() => setHoveredLetter(l)}
              onBlur={() => setHoveredLetter(null)}
              aria-label={`Open ${LETTER_CONTENT[l].title} documentation`}
              className="absolute z-[45] flex h-[12%] w-[8%] cursor-pointer pointer-events-auto items-center justify-center rounded-sm focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow focus-visible:ring-offset-2 focus-visible:ring-offset-ink-black group/letter"
              style={{
                left: `${26 + i * 12}%`,
                top: "50%",
                transform: "translate(-50%, -50%)",
              }}
            >
              <span className="sr-only">{l}</span>
              <motion.div
                aria-hidden="true"
                animate={{ opacity: hoveredLetter === l ? 1 : 0 }}
                className="absolute -bottom-1 h-[1px] w-full bg-cyan-glow shadow-[0_0_8px_rgba(15,255,255,0.8)]"
              />
            </Link>
          ))}
        </div>

        {/* Orbital Rings — Locked at the technical core scale ([0.27, 0.4, 0.52]) to maintain the high-density cockpit aesthetic
            while orbiting the enlarged centerpiece. */}
        {[0.27, 0.4, 0.52].map((factor, i) => (
          <motion.div
            key={i}
            animate={{ rotate: i % 2 === 0 ? 360 : -360 }}
            transition={{ duration: 26 + i * 16, repeat: Infinity, ease: "linear" }}
            className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 rounded-full border border-platinum/10 border-dashed pointer-events-none z-40"
            style={{ width: `${factor * 100}%`, height: `${factor * 100}%` }}
          />
        ))}

        {/* Flash sparkles orbiting the coin.
            The wrapper handles static centering on its (top, left) anchor point;
            the icon's own animation only drives scale/rotate, so the two
            transforms never fight each other. */}
        {SPARKLE_POSITIONS.map((s, i) => (
          <div
            key={i}
            className="absolute -translate-x-1/2 -translate-y-1/2 pointer-events-none z-50"
            style={{ top: s.top, left: s.left }}
          >
            <Sparkles
              className="sparkle-blink text-cyan-glow"
              style={{ width: s.size, height: s.size, animationDelay: `${s.delay}s` }}
            />
          </div>
        ))}

        {/* Crosshair micro-ticks */}
        <div className="absolute inset-0 pointer-events-none opacity-20 z-50">
          <div className="absolute top-0 left-1/2 -translate-x-1/2 w-[1px] h-8 bg-cyan-glow/50" />
          <div className="absolute bottom-0 left-1/2 -translate-x-1/2 w-[1px] h-8 bg-cyan-glow/50" />
          <div className="absolute left-0 top-1/2 -translate-y-1/2 h-[1px] w-8 bg-cyan-glow/50" />
          <div className="absolute right-0 top-1/2 -translate-y-1/2 h-[1px] w-8 bg-cyan-glow/50" />
        </div>
      </motion.div>

      {/* Side Content Panel — Opens on the right for (U, L, T) and left for (R, A) as requested */}
      <AnimatePresence>
        {hoveredLetter && (
          <motion.div
            key={hoveredLetter}
            initial={{ 
              opacity: 0, 
              x: ["R", "A"].includes(hoveredLetter) ? -40 : 40, 
              scale: 0.95 
            }}
            animate={{ opacity: 1, x: 0, scale: 1 }}
            exit={{ 
              opacity: 0, 
              x: ["R", "A"].includes(hoveredLetter) ? -20 : 20, 
              scale: 0.98 
            }}
            className={clsx(
              "absolute top-1/2 -translate-y-1/2 w-[320px] z-[60] hidden lg:block",
              ["R", "A"].includes(hoveredLetter) ? "left-[2vw] 2xl:left-[6vw]" : "right-[2vw] 2xl:right-[6vw]"
            )}
          >
            {/* "Couple pages" effect - stacked layers behind the main card */}
            <div className={clsx(
              "absolute -inset-2 bg-cyan-glow/5 border border-cyan-glow/20 z-[-1]",
              ["R", "A"].includes(hoveredLetter) ? "rotate-1" : "-rotate-1"
            )} />
            <div className={clsx(
              "absolute -inset-1 bg-ink-black/40 border border-platinum/10 z-[-1]",
              ["R", "A"].includes(hoveredLetter) ? "-rotate-1" : "rotate-1"
            )} />

            <div className="relative bg-[#010B13]/95 border border-cyan-glow/30 backdrop-blur-xl p-6 shadow-2xl">
              <div className="flex justify-between items-start mb-6">
                <div className="text-[10px] text-cyan-glow font-mono tracking-widest uppercase">
                  {"// Protocol_Module_"}{hoveredLetter}
                </div>
                <div className="flex gap-1">
                  <div className="w-1.5 h-1.5 bg-cyan-glow animate-pulse" />
                  <div className="w-1.5 h-1.5 bg-platinum/20" />
                </div>
              </div>

              <h3 className="text-xl font-bold text-platinum mb-4 tracking-tighter uppercase border-b border-platinum/10 pb-2">
                {LETTER_CONTENT[hoveredLetter].title}
              </h3>

              <p className="text-xs text-platinum/70 font-mono leading-relaxed mb-6">
                {LETTER_CONTENT[hoveredLetter].desc}
              </p>

              <div className="grid grid-cols-2 gap-4 pt-4 border-t border-platinum/10">
                <div>
                  <div className="text-[9px] text-platinum/30 uppercase mb-1">Status</div>
                  <div className="text-[10px] text-green-400 font-mono uppercase tracking-widest">Active_Link</div>
                </div>
                <div>
                  <div className="text-[9px] text-platinum/30 uppercase mb-1">Integrity</div>
                  <div className="text-[10px] text-platinum/80 font-mono">99.998%</div>
                </div>
              </div>

              <div className="mt-8 flex items-center justify-between">
                <div className="text-[8px] text-platinum/20 font-mono tracking-widest uppercase">
                  Ref_ID: {hoveredLetter}2026-X
                </div>
                <Link 
                  href={`/docs/${LETTER_CONTENT[hoveredLetter].slug}`}
                  className="px-3 py-1 bg-cyan-glow/10 border border-cyan-glow/20 text-cyan-glow text-[9px] font-bold uppercase tracking-widest hover:bg-cyan-glow/20 transition-colors cursor-pointer"
                >
                  Open_Docs
                </Link>
              </div>
            </div>
          </motion.div>
        )}
      </AnimatePresence>

      {/* Genesis stats */}
      <div className="flex gap-14 font-mono z-10">
        <div className="text-center">
          <div className="text-xs lg:text-sm uppercase text-platinum/40 mb-1">Genesis Supply</div>
          <div className="text-xl md:text-2xl lg:text-3xl font-medium text-cyan-glow">1.00B $ULTRA</div>
        </div>
        <div className="text-center">
          <div className="text-xs lg:text-sm uppercase text-platinum/40 mb-1">Staked</div>
          <div className="text-xl md:text-2xl lg:text-3xl font-medium text-cyan-glow">64.2%</div>
        </div>
      </div>

      {/* CTAs */}
      <div className="z-10 mt-8 flex w-full flex-col items-center gap-4 px-4 sm:w-auto sm:flex-row sm:px-0">
        <Link href="/login" className="inline-flex w-full justify-center px-9 py-4 lg:px-10 lg:py-5 sm:w-auto bg-cyan-glow text-[#010B13] font-bold text-sm lg:text-base uppercase tracking-[0.15em] cut-corner hover:bg-white transition-colors">
          Initialize_Node
        </Link>
        <Link href="/docs" className="text-center text-sm lg:text-base font-mono text-platinum/50 hover:text-cyan-glow transition-colors uppercase tracking-widest">
          [ WHITEPAPER.pdf ]
        </Link>
      </div>
    </div>
  );
}
