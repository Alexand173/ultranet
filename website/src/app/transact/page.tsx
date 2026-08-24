import type { Metadata } from "next";
import TransactionPage from "@/components/transactions/TransactionPage";

export const metadata: Metadata = {
  title: "Send $ULTRA | UltraNet",
  description: "Create or unlock a local UltraNet wallet and send $ULTRA with an explicit review and local Dilithium-5 signature.",
};

export default function TransactPage() {
  return <TransactionPage />;
}
