"use client";

import { motion } from "framer-motion";
import { useEffect, useState } from "react";
import { Cpu, Network, ShieldAlert, Zap, ArrowRight } from "lucide-react";
import Link from "next/link";
import { cn } from "@/lib/utils";
import { EXPLORER_URL } from "@/lib/links";

const DOCS_CELLS = [
  {
    title: "Block-STM Parallel Execution",
    description: "16-way logical sharding with optimistic concurrency control. Near-linear scaling across CPU cores.",
    icon: <Cpu className="w-8 h-8 text-cyan-glow" />,
    size: "col-span-1 row-span-1",
    tag: "Performance",
  },
  {
    title: "Mysticeti DAG Consensus",
    description: "Low-latency Bullshark ordering rule. 27.79µs vertex commit time with post-quantum finality.",
    icon: <Network className="w-8 h-8 text-cyan-glow" />,
    size: "col-span-1 row-span-1",
    tag: "Consensus",
  },
  {
    title: "Post-Quantum Dilithium-5",
    description: "NIST-standardized lattice-based signatures. Secured against future quantum adversarial clusters.",
    icon: <ShieldAlert className="w-8 h-8 text-cyan-glow" />,
    size: "col-span-1 row-span-1",
    tag: "Security",
  },
  {
    title: "Layer-3 AppChain Anchoring",
    description: "Modular sub-networks with native STARK verifiability and L1 security inheritance.",
    icon: <Zap className="w-8 h-8 text-cyan-glow" />,
    size: "col-span-1 row-span-1",
    tag: "Scalability",
  },
];

export default function BentoDocs() {
  const [isMounted, setIsMounted] = useState(false);

  useEffect(() => {
    setIsMounted(true);
  }, []);

  return (
    <section className="py-16 px-4 max-w-7xl mx-auto space-y-12 relative z-10">
      {/* Header with glitch effect */}
      <div className="space-y-4">
        <p className="text-cyan-glow font-mono text-sm tracking-widest uppercase flex items-center gap-2">
          <span className="inline-block w-2 h-2 bg-cyan-glow rounded-full animate-pulse" />
          Documentation // Core Modules
        </p>
        <h2 
          className="text-4xl md:text-6xl font-bold glitch-text" 
          data-text="The Sovereign Guide"
        >
          The Sovereign Guide
        </h2>
        <p className="text-platinum/40 max-w-2xl font-mono text-sm">
          Explore the foundational pillars of UltraNet&apos;s next-generation blockchain architecture.
        </p>
      </div>

      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 auto-rows-[300px]">
        {/* Main Feature Cell with Coin SVG Background */}
        <motion.div
          whileHover={{ scale: 0.99 }}
          className="md:col-span-2 md:row-span-2 neon-inset p-8 flex flex-col justify-between relative overflow-hidden group cut-corner"
        >
          {/* Scanline effect */}
          <div className="scanline" />
          
          {/* Crosshair lines */}
          <div className="crosshair-lines absolute inset-0 pointer-events-none opacity-30" />
          
          {/* Coin SVG Background */}
          <div className="absolute top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 w-[120%] h-[120%] pointer-events-none">
            <svg 
              viewBox="0 0 400 400" 
              className="w-full h-full object-contain rotate-12 transition-all duration-700 group-hover:rotate-6 group-hover:scale-110 opacity-[0.04] group-hover:opacity-[0.08] coin-glow"
            >
              {isMounted && (
                <>
                  {/* Outer Coin Ring */}
                  <circle cx="200" cy="200" r="190" fill="none" stroke="#00ffff" strokeWidth="1.5" opacity="0.3"/>
                  <circle cx="200" cy="200" r="185" fill="none" stroke="#00ffff" strokeWidth="0.5" opacity="0.2"/>
                  
                  {/* Inner Ring */}
                  <circle cx="200" cy="200" r="170" fill="none" stroke="#00ffff" strokeWidth="0.5" opacity="0.15"/>
                  
                  {/* Decorative dots with sparkle */}
                  {[...Array(24)].map((_, i) => {
                    const angle = (i / 24) * 2 * Math.PI - Math.PI / 2;
                    const x = 200 + 185 * Math.cos(angle);
                    const y = 200 + 185 * Math.sin(angle);
                    return (
                      <circle 
                        key={i} 
                        cx={x} 
                        cy={y} 
                        r="2" 
                        fill="#00ffff" 
                        opacity="0.15"
                        className="sparkle-blink"
                        style={{ animationDelay: `${i * 0.15}s` }}
                      />
                    );
                  })}
                  
                  {/* Text Content */}
                  <text 
                    x="200" 
                    y="155" 
                    textAnchor="middle" 
                    fill="#00ffff" 
                    opacity="0.2"
                    fontSize="80" 
                    fontWeight="bold"
                    fontFamily="monospace"
                    letterSpacing="5"
                  >
                    U
                  </text>
                  
                  <text 
                    x="200" 
                    y="205" 
                    textAnchor="middle" 
                    fill="#00ffff" 
                    opacity="0.12"
                    fontSize="13" 
                    fontWeight="bold"
                    fontFamily="monospace"
                    letterSpacing="4"
                  >
                    NETWORK
                  </text>
                  
                  <text 
                    x="200" 
                    y="228" 
                    textAnchor="middle" 
                    fill="#00ffff" 
                    opacity="0.12"
                    fontSize="13" 
                    fontWeight="bold"
                    fontFamily="monospace"
                    letterSpacing="4"
                  >
                    SCALE
                  </text>
                  
                  <text 
                    x="200" 
                    y="255" 
                    textAnchor="middle" 
                    fill="#00ffff" 
                    opacity="0.08"
                    fontSize="9" 
                    fontFamily="monospace"
                    letterSpacing="6"
                  >
                    U
                  </text>
                  
                  {/* Center Glow */}
                  <circle cx="200" cy="200" r="50" fill="url(#glowGradient)" opacity="0.3"/>
                  <circle cx="200" cy="200" r="30" fill="url(#glowGradient)" opacity="0.2">
                    <animate attributeName="r" values="20;35;20" dur="3s" repeatCount="indefinite"/>
                    <animate attributeName="opacity" values="0.2;0.4;0.2" dur="3s" repeatCount="indefinite"/>
                  </circle>
                  
                  <defs>
                    <radialGradient id="glowGradient">
                      <stop offset="0%" stopColor="#00ffff" stopOpacity="0.8"/>
                      <stop offset="100%" stopColor="#00ffff" stopOpacity="0"/>
                    </radialGradient>
                  </defs>
                </>
              )}
            </svg>
          </div>
          
          {/* Hex grid background */}
          <div className="absolute inset-0 hex-grid opacity-20 pointer-events-none" />
          
          {/* Content */}
          <div className="space-y-4 relative z-10">
            <div className="flex items-center gap-3">
              <span className="px-3 py-1 text-[10px] font-mono uppercase tracking-widest border border-cyan-glow/20 text-cyan-glow rounded">
                v7.1
              </span>
              <span className="px-3 py-1 text-[10px] font-mono uppercase tracking-widest bg-cyan-glow/5 text-platinum/40 rounded">
                Architecture
              </span>
            </div>
            <h3 className="text-3xl font-bold text-platinum">UltraNet v7.1 Architecture</h3>
            <p className="max-w-md text-platinum/60 leading-relaxed">
              A comprehensive deep-dive into the 34 technical subsystems that make UltraNet 
              the most resilient ledger ever engineered. From FHE privacy layers to sharded state trie pruning.
            </p>
          </div>
          
          <div className="relative z-10 flex gap-4 flex-wrap">
            <a
              href={EXPLORER_URL}
              target="_blank"
              rel="noreferrer"
              className="px-6 py-3 border border-cyan-glow text-cyan-glow text-sm font-bold uppercase tracking-widest hover:bg-cyan-glow/10 transition-colors relative overflow-hidden shimmer-sweep flex items-center gap-2 group/btn"
            >
              LAUNCH_EXPLORER.sh
              <ArrowRight className="w-4 h-4 group-hover/btn:translate-x-1 transition-transform" aria-hidden="true" />
            </a>
            <Link href="/docs" className="px-6 py-3 bg-platinum/10 text-platinum text-sm font-bold uppercase tracking-widest hover:bg-platinum/20 transition-colors neon-flicker-border">
              DOCS.md
            </Link>
          </div>
          
          {/* Background Decoration */}
          <div className="absolute top-0 right-0 w-64 h-64 bg-cyan-glow/5 rounded-full blur-3xl group-hover:bg-cyan-glow/10 transition-colors" />
          <div className="absolute bottom-4 right-4 text-8xl font-black text-platinum/[0.03] pointer-events-none select-none">V7.1</div>
          
          {/* Data rain effect */}
          <div className="data-rain-col absolute inset-0 pointer-events-none opacity-20" />
        </motion.div>

        {/* Dynamic Cells */}
        {DOCS_CELLS.map((cell, i) => (
          <motion.div
            key={i}
            initial={{ opacity: 0, y: 20 }}
            whileInView={{ opacity: 1, y: 0 }}
            transition={{ delay: i * 0.1 }}
            whileHover={{ y: -5 }}
            className={cn(
              "neon-inset p-6 flex flex-col gap-4 group cursor-pointer cut-corner relative overflow-hidden",
              cell.size
            )}
          >
            {/* Hover glow */}
            <div className="absolute -inset-1 bg-cyan-glow/0 group-hover:bg-cyan-glow/5 transition-colors duration-300 blur-xl" />
            
            {/* Orbital ring */}
            <div className="orbital-ring w-[150%] h-[150%] opacity-0 group-hover:opacity-10 transition-opacity duration-500" />
            
            {/* Tag */}
            <div className="relative z-10 flex items-start justify-between">
              <div className="p-3 bg-cyan-glow/5 w-fit rounded-lg group-hover:bg-cyan-glow/20 transition-colors">
                {cell.icon}
              </div>
              <span className="text-[10px] font-mono uppercase tracking-widest text-cyan-glow/30 group-hover:text-cyan-glow/60 transition-colors">
                {cell.tag}
              </span>
            </div>
            
            <div className="relative z-10 flex-1">
              <h4 className="text-xl font-bold text-platinum group-hover:text-cyan-glow transition-colors">
                {cell.title}
              </h4>
              <p className="text-sm text-platinum/50 leading-relaxed mt-2">{cell.description}</p>
            </div>
            
            {/* Bottom border glow */}
            <div className="absolute bottom-0 left-0 right-0 h-[1px] bg-gradient-to-r from-transparent via-cyan-glow/20 to-transparent opacity-0 group-hover:opacity-100 transition-opacity duration-300" />
            
            {/* Corner accent */}
            <div className="absolute bottom-0 right-0 w-12 h-12 border-r border-b border-cyan-glow/0 group-hover:border-cyan-glow/10 transition-colors duration-300 rounded-br-lg" />
          </motion.div>
        ))}
      </div>
    </section>
  );
}