"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { EXPLORER_URL } from "@/lib/links";
import { useGamesAuth } from "@/components/games/GamesSessionProvider";
import { shortId } from "@/lib/games-status";

const LINKS = [
  { href: "/games", label: "Catalog", match: (path: string) => path === "/games" || /^\/games\/[0-9a-f]{32}$/.test(path) },
  { href: "/games/rewards", label: "My rewards", match: (path: string) => path === "/games/rewards" },
  { href: "/games/manage", label: "My games", match: (path: string) => path.startsWith("/games/manage") || /^\/games\/[0-9a-f]{32}\/manage/.test(path) },
];

const ELSEWHERE = [
  { href: "/", label: "Home", external: false },
  { href: "/appchains", label: "AppChains", external: false },
  { href: "/send-ultra", label: "Wallet", external: false },
  { href: EXPLORER_URL, label: "Explorer", external: true },
  { href: "/docs", label: "Developers", external: false },
];

/** Navigation inside Games, plus the way out to the rest of the site. The site's own top bar stays the only top bar. */
export default function GamesNav() {
  const pathname = usePathname();
  const { address, walletState, status, signOut } = useGamesAuth();
  return (
    <div className="border-b border-platinum/10 bg-ink-black/70">
      <div className="mx-auto flex max-w-7xl flex-wrap items-center justify-between gap-x-6 gap-y-2 px-6 py-3">
        <nav aria-label="Games" className="flex flex-wrap items-center gap-1">
          {LINKS.map((link) => {
            const current = link.match(pathname);
            return (
              <Link
                key={link.href}
                href={link.href}
                aria-current={current ? "page" : undefined}
                className={`min-h-11 inline-flex items-center border-b-2 px-3 font-mono text-xs font-bold uppercase tracking-[0.12em] focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow ${
                  current ? "border-cyan-glow text-cyan-glow" : "border-transparent text-platinum/70 hover:text-platinum"
                }`}
              >
                {link.label}
              </Link>
            );
          })}
        </nav>
        <div className="flex flex-wrap items-center gap-x-5 gap-y-1">
          <nav aria-label="Elsewhere on UltraNet" className="flex flex-wrap items-center gap-x-4">
            {ELSEWHERE.map((link) => (
              <Link
                key={link.label}
                href={link.href}
                {...(link.external ? { target: "_blank", rel: "noopener noreferrer" } : {})}
                className="min-h-11 inline-flex items-center text-xs text-platinum-muted hover:text-cyan-glow focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow"
              >
                {link.label}
                {link.external ? <span className="sr-only"> (opens in a new tab)</span> : null}
              </Link>
            ))}
          </nav>
          <p className="font-mono text-[11px] text-platinum-muted" aria-live="polite">
            {walletState === "checking"
              ? "Checking wallet…"
              : address
                ? `${shortId(address, 6, 4)} · ${status === "signed-in" ? "signed in" : walletState === "unlocked" ? "unlocked" : "locked"}`
                : "No wallet"}
            {status === "signed-in" ? (
              <button type="button" onClick={() => void signOut()} className="ml-2 underline hover:text-cyan-glow focus:outline-none focus-visible:ring-2 focus-visible:ring-cyan-glow">
                Sign out
              </button>
            ) : null}
          </p>
        </div>
      </div>
    </div>
  );
}
