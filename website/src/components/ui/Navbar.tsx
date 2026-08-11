"use client";

import Link from "next/link";
import { motion } from "framer-motion";
import { Menu, X } from "lucide-react";
import { usePathname } from "next/navigation";
import { useState } from "react";
import { EXPLORER_URL } from "@/lib/links";

const NAV_LINKS = [
  { name: "LEARN", href: "/learn" },
  { name: "DOCS", href: "/docs" },
  { name: "EXPLORER", href: EXPLORER_URL },
  { name: "WHITEPAPER", href: "/docs/whitepaper" },
  { name: "JOIN_SWARM", href: "/#swarm" },
];

export default function Navbar() {
  const pathname = usePathname();
  const [isOpen, setIsOpen] = useState(false);

  if (pathname === "/login" || pathname.startsWith("/operator")) return null;

  return (
    <nav className="fixed top-0 left-0 right-0 z-50 bg-ink-black/80 backdrop-blur-md border-b border-platinum/10">
      <div className="max-w-7xl mx-auto px-6 h-20 flex items-center justify-between">
        <Link href="/" className="flex items-center gap-3 group">
          <div className="w-10 h-10 bg-cyan-glow flex items-center justify-center font-black text-ink-black rounded-sm shadow-[0_0_15px_rgba(15,255,255,0.4)] group-hover:scale-110 transition-transform">
            UN
          </div>
          <div className="flex flex-col">
            <span className="font-bold text-lg tracking-tighter leading-none">ULTRANET</span>
            <span className="text-[10px] text-platinum/40 font-mono tracking-widest leading-none mt-1 uppercase">Sovereign_OS</span>
          </div>
        </Link>

        {/* Desktop Nav */}
        <div className="hidden md:flex items-center gap-12">
          {NAV_LINKS.map((link) => (
            <Link 
              key={link.name} 
              href={link.href}
              className="text-xs font-bold tracking-[0.2em] text-platinum/60 hover:text-cyan-glow transition-colors"
            >
              {link.name}
            </Link>
          ))}
          <Link href="/login" className="px-6 py-2 border border-cyan-glow text-cyan-glow text-xs font-black uppercase tracking-[0.2em] hover:bg-cyan-glow hover:text-ink-black transition-all shadow-[inset_0_0_10px_rgba(15,255,255,0)] hover:shadow-[inset_0_0_20px_rgba(15,255,255,0.5)]">
            VAULT_LOGIN
          </Link>
        </div>

        {/* Mobile Toggle */}
        <button
          type="button"
          className="md:hidden text-platinum"
          onClick={() => setIsOpen(!isOpen)}
          aria-label={isOpen ? "Close navigation menu" : "Open navigation menu"}
          aria-expanded={isOpen}
          aria-controls="mobile-navigation"
        >
          {isOpen ? <X aria-hidden="true" /> : <Menu aria-hidden="true" />}
        </button>
      </div>

      {/* Mobile Menu */}
      {isOpen && (
        <motion.div
          id="mobile-navigation"
          role="region"
          aria-label="Mobile navigation"
          initial={{ opacity: 0, y: -20 }}
          animate={{ opacity: 1, y: 0 }}
          className="md:hidden absolute top-20 left-0 right-0 bg-ink-black border-b border-platinum/10 p-8 flex flex-col gap-6"
        >
          {NAV_LINKS.map((link) => (
            <Link 
              key={link.name} 
              href={link.href}
              onClick={() => setIsOpen(false)}
              className="text-sm font-bold tracking-widest text-platinum/60"
            >
              {link.name}
            </Link>
          ))}
          <Link href="/login" onClick={() => setIsOpen(false)} className="block w-full py-4 bg-cyan-glow text-center text-ink-black font-black uppercase tracking-widest">
            VAULT_LOGIN
          </Link>
        </motion.div>
      )}
    </nav>
  );
}
