import type { Metadata } from "next";
import WhitepaperReader from "@/components/docs/WhitepaperReader";

export const metadata: Metadata = {
  title: "UltraNet Whitepaper | The Sovereign Technical Guide",
  description: "The canonical UltraNet v7.1 technical whitepaper with rendered Mermaid architecture diagrams.",
};

export default function WhitepaperPage() {
  return <WhitepaperReader />;
}
