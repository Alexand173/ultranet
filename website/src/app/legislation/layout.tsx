import type { Metadata } from "next";

/** Client page — the title has to be declared by a server layout. */
export const metadata: Metadata = {
  title: "Protocol Legislation | UltraNet",
  description:
    "Repository source notes for the protocol rules that govern the UltraNet network.",
};

export default function LegislationLayout({ children }: { children: React.ReactNode }) {
  return children;
}
