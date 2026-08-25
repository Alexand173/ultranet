import type { Metadata } from "next";
import "@/styles/globals.css";

export const metadata: Metadata = {
  title: "UltraNet | The 100-Year Sovereign Engine",
  description: "Ultra-modern, post-quantum blockchain protocol for a multi-decade operational horizon.",
  authors: [{ name: "Vladan Jotov" }],
  creator: "Vladan Jotov",
  publisher: "Vladan Jotov",
};

import Navbar from "@/components/ui/Navbar";
import SmoothScroll from "@/components/ui/SmoothScroll";
import GlobalCoin from "@/components/ui/GlobalCoin";
import VisualNoise from "@/components/ui/VisualNoise";
import GlobalCommandSearch from "@/components/ui/GlobalCommandSearch";
import WalletSessionProvider from "@/components/wallet/WalletSessionProvider";

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en">
      <body className="antialiased relative bg-ink-black noise-bg">
        <VisualNoise />
        <GlobalCommandSearch />
        {/* Global High-Density Background Flash Overlay */}
        <div className="fixed inset-0 pointer-events-none z-[60] overflow-hidden opacity-10 mix-blend-screen">
          <div className="absolute top-0 left-0 w-full h-[1px] bg-cyan-glow/50 animate-[scanline_10s_linear_infinite]" />
        </div>
        
        <WalletSessionProvider>
          <SmoothScroll>
            <Navbar />
            <GlobalCoin />
            {children}
          </SmoothScroll>
        </WalletSessionProvider>
      </body>
    </html>
  );
}
