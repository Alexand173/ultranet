import type { Metadata } from "next";
import AppChainsConsole from "@/components/appchains/AppChainsConsole";

export const metadata: Metadata = {
  title: "AppChains | UltraNet Operator Console",
  description: "Authenticated UltraNet AppChain registry: owner-signed chains, anchors and account accounting.",
};

export default function AppChainsPage() {
  return <AppChainsConsole />;
}
