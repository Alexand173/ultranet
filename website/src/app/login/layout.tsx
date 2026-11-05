import type { Metadata } from "next";

/** Client page — the title has to be declared by a server layout. */
export const metadata: Metadata = {
  title: "Vault Access | UltraNet",
  description:
    "Sign in to the UltraNet dashboard with UltraWallet or a locally signed challenge payload.",
};

export default function LoginLayout({ children }: { children: React.ReactNode }) {
  return children;
}
