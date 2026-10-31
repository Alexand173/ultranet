import type { Metadata } from "next";
import UltraVerseLauncher from "@/components/ultraverse/UltraVerseLauncher";

export const metadata: Metadata = {
  title: "UltraVerse Services | UltraNet",
  description: "Browse supported UltraNet wallet, AppChain, and validator paths alongside clearly marked ecosystem concepts.",
};

export default function UltraVersePage() {
  return <UltraVerseLauncher />;
}
