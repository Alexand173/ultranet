import type { Metadata } from "next";
import "@/styles/globals.css";

export const metadata: Metadata = {
  title: "UltraNet | Blockchain Network",
  description: "UltraNet protocol documentation, wallet tools, validator onboarding, and node-reported network data.",
  authors: [{ name: "Vladan Jotov" }],
  creator: "Vladan Jotov",
  publisher: "Vladan Jotov",
};

import Navbar from "@/components/ui/Navbar";
import SkipToContent from "@/components/ui/SkipToContent";
import SmoothScroll from "@/components/ui/SmoothScroll";
import GlobalCoin from "@/components/ui/GlobalCoin";
import VisualNoise from "@/components/ui/VisualNoise";
import GlobalCommandSearch from "@/components/ui/GlobalCommandSearch";
import WalletSessionProvider from "@/components/wallet/WalletSessionProvider";
import WalletBalancePill from "@/components/wallet/WalletBalancePill";

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  // data-scroll-behavior lets Next suspend CSS smooth scrolling while it swaps
  // routes; without it the smooth scroll fights the route transition.
  return (
    <html lang="en" data-scroll-behavior="smooth">
      <body className="antialiased relative bg-ink-black noise-bg">
        {/* Must stay the first focusable element in the document. */}
        <SkipToContent />
        <VisualNoise />
        <GlobalCommandSearch />
        {/* Global High-Density Background Flash Overlay */}
        <div data-global-visual-overlay className="fixed inset-0 pointer-events-none z-[60] overflow-hidden opacity-10 mix-blend-screen">
          <div className="absolute top-0 left-0 w-full h-[1px] bg-cyan-glow/50 animate-[scanline_10s_linear_infinite]" />
        </div>
        
        <WalletSessionProvider>
          <SmoothScroll>
            <Navbar />
            <GlobalCoin />
            <WalletBalancePill />
            {children}
          </SmoothScroll>
        </WalletSessionProvider>
      </body>
    </html>
  );
}
