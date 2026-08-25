"use client";

import { motion } from "framer-motion";
import Link from "next/link";
import { ChevronLeft } from "lucide-react";
import EducationalWalletCallout from "@/components/wallet/EducationalWalletCallout";

interface DocPageProps {
  title: string;
  subtitle: string;
  pillar: string;
  content: React.ReactNode;
  icon: React.ReactNode;
}

export default function DocPage({ title, subtitle, pillar, content, icon }: DocPageProps) {
  return (
    <div className="min-h-screen bg-[#010B13] text-platinum font-sans selection:bg-cyan-glow/30 selection:text-white">
      {/* Background Grids */}
      <div className="fixed inset-0 dot-grid opacity-20 pointer-events-none" />
      <div className="fixed inset-0 bg-[radial-gradient(circle_at_50%_50%,rgba(15,255,255,0.03)_0%,transparent_70%)] pointer-events-none" />

      {/* Header */}
      <header className="sticky top-0 z-50 bg-[#010B13]/80 backdrop-blur-md border-b border-platinum/10 px-6 py-4">
        <div className="max-w-7xl mx-auto flex items-center justify-between">
          <Link 
            href="/" 
            className="flex items-center gap-2 text-platinum/50 hover:text-cyan-glow transition-colors group"
          >
            <ChevronLeft className="w-4 h-4 group-hover:-translate-x-1 transition-transform" />
            <span className="font-mono text-xs uppercase tracking-widest">Back to_Nexus</span>
          </Link>
          <div className="flex items-center gap-3">
            <div className="w-2 h-2 bg-cyan-glow animate-pulse" />
            <span className="font-mono text-[10px] text-cyan-glow uppercase tracking-[0.4em]">
              Sovereign_Protocol_Docs // v7.1
            </span>
          </div>
        </div>
      </header>

      <main className="max-w-5xl mx-auto px-6 py-20 relative">
        {/* Pillar Indicator */}
        <motion.div 
          initial={{ opacity: 0, y: 20 }}
          animate={{ opacity: 1, y: 0 }}
          className="flex items-center gap-4 mb-8"
        >
          <div className="p-3 bg-cyan-glow/10 border border-cyan-glow/20 rounded-sm text-cyan-glow shadow-[0_0_15px_rgba(15,255,255,0.1)]">
            {icon}
          </div>
          <div>
            <div className="text-[10px] text-cyan-glow font-mono uppercase tracking-widest mb-1">
              Protocol_Pillar // {pillar}
            </div>
            <h1 className="text-4xl md:text-5xl font-bold tracking-tighter uppercase glitch-text" data-text={title}>
              {title}
            </h1>
          </div>
        </motion.div>

        {/* Content Area */}
        <motion.div 
          initial={{ opacity: 0 }}
          animate={{ opacity: 1 }}
          transition={{ delay: 0.2 }}
          className="relative"
        >
          <div className="absolute -left-12 top-0 bottom-0 w-px bg-gradient-to-b from-cyan-glow/50 via-platinum/10 to-transparent hidden xl:block" />
          
          <div className="prose prose-invert prose-cyan max-w-none">
            <p className="text-xl text-platinum/60 font-mono leading-relaxed mb-12 italic border-l-2 border-cyan-glow/30 pl-6">
              &quot;{subtitle}&quot;
            </p>
            <div className="space-y-12">
              {content}
            </div>
          </div>
        </motion.div>

        <div className="mt-16">
          <EducationalWalletCallout variant="dark" placement="docs" />
        </div>

        {/* Footer Navigation */}
        <div className="mt-32 pt-12 border-t border-platinum/10 flex flex-col md:flex-row gap-8 items-center justify-between font-mono">
          <div className="text-[10px] text-platinum/30 uppercase tracking-[0.2em]">
            &copy; 2026 Vladan Jotov // UltraNet original code and documentation // ISC_License
          </div>
          <div className="flex gap-8">
            <Link href="/docs" className="text-[10px] text-platinum/50 hover:text-cyan-glow transition-colors uppercase tracking-widest">
              [ Whitepaper ]
            </Link>
            <Link href="/" className="text-[10px] text-platinum/50 hover:text-cyan-glow transition-colors uppercase tracking-widest">
              [ Back_to_Nexus ]
            </Link>
          </div>
        </div>
      </main>
    </div>
  );
}
