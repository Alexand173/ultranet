import type { Metadata } from "next";

/**
 * `/docs` is a client component (it has search and filters), and a client
 * component cannot export metadata. The title lives here instead; every child
 * route that wants its own sets one in its own page file.
 */
export const metadata: Metadata = {
  title: "Documentation Hub | UltraNet",
  description:
    "Repository source notes for the UltraNet protocol pillars: consensus, scalability, execution, and governance.",
};

export default function DocsLayout({ children }: { children: React.ReactNode }) {
  return children;
}
