import type { Metadata } from "next";
import GamesNav from "@/components/games/GamesNav";
import GamesSessionProvider from "@/components/games/GamesSessionProvider";

export const metadata: Metadata = {
  title: "Games | UltraNet",
  description: "Play games that run on UltraNet AppChains. Results are verified, and rewards are paid only from each game's own funded pool, after the network confirms them.",
};

export default function GamesLayout({ children }: { children: React.ReactNode }) {
  return (
    <GamesSessionProvider>
      <div className="min-h-screen pt-20">
        <GamesNav />
        <main id="main-content" tabIndex={-1}>
          {children}
        </main>
      </div>
    </GamesSessionProvider>
  );
}
