import type { Metadata } from "next";
import TransactionPage from "@/components/transactions/TransactionPage";
import { getSafeReturnPath } from "@/lib/redirects";

export const metadata: Metadata = {
  title: "Send $ULTRA | UltraNet",
  description: "Create or unlock a local UltraNet wallet and send $ULTRA with an explicit review and local Dilithium-5 signature.",
};

export default async function SendUltraPage({
  searchParams,
}: {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const params = await searchParams;
  const rawReturnTo = typeof params.returnTo === "string" ? params.returnTo : undefined;
  const returnTo = getSafeReturnPath(rawReturnTo) ?? undefined;

  return <TransactionPage returnTo={returnTo} />;
}
