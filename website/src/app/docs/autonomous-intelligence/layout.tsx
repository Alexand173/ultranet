import type { Metadata } from "next";

/** Client page — the title has to be declared by a server layout. */
export const metadata: Metadata = {
  title: "AI Governor Implementation | UltraNet Docs",
};

export default function AutonomousIntelligenceLayout({ children }: { children: React.ReactNode }) {
  return children;
}
