"use client";

import Link from "next/link";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";
import { fetchAccount } from "@/lib/api";
import { SEND_ULTRA_PATH } from "@/lib/links";
import { formatUltraAmount } from "@/lib/transaction-format";
import { useWalletSession } from "@/components/wallet/WalletSessionProvider";

const REFRESH_MS = 30_000;
const BALANCE_DECIMALS = 6;

/**
 * The unlocked wallet's balance, carried on every page so it stays visible while the user works in
 * another service. Mounted inside the root layout's wallet provider, which is why the session
 * survives page navigation: this pill simply reads it.
 *
 * Node data is the only source. When the read fails it says "Balance unavailable" instead of
 * showing a stale or invented figure, and the link still works.
 */
export default function WalletBalancePill() {
  const pathname = usePathname();
  const { sessionState, walletAddress } = useWalletSession();
  const [balanceBaseUnits, setBalanceBaseUnits] = useState<number | null>(null);

  useEffect(() => {
    if (sessionState !== "unlocked" || !walletAddress) {
      setBalanceBaseUnits(null);
      return;
    }

    const controller = new AbortController();
    let active = true;

    const readBalance = async () => {
      try {
        const account = await fetchAccount(walletAddress, controller.signal);
        if (active) setBalanceBaseUnits(account.balanceBaseUnits);
      } catch {
        // Aborted reads keep the previous value; a real failure is reported as unavailable.
        if (active && !controller.signal.aborted) setBalanceBaseUnits(null);
      }
    };

    void readBalance();
    const interval = window.setInterval(readBalance, REFRESH_MS);

    return () => {
      active = false;
      controller.abort();
      window.clearInterval(interval);
    };
  }, [pathname, sessionState, walletAddress]);

  if (sessionState !== "unlocked" || pathname === "/login") return null;

  const balanceLabel = balanceBaseUnits === null
    ? "Balance unavailable"
    : `${formatUltraAmount(balanceBaseUnits, BALANCE_DECIMALS)} ULTRA`;

  return (
    <Link
      href={SEND_ULTRA_PATH}
      aria-label={`Wallet ${balanceLabel}. Open Portfolio.`}
      className="fixed bottom-5 left-4 z-[95] flex min-h-11 items-center gap-3 rounded-sm border border-platinum/20 bg-ink-black/90 px-3 py-2 font-mono text-[10px] uppercase tracking-[0.14em] text-platinum backdrop-blur-md transition-colors hover:border-cyan-glow/60 hover:text-white focus:outline-hidden focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
    >
      <span className="text-platinum/45">Wallet</span>
      <span className={balanceBaseUnits === null ? "text-platinum/55" : "font-bold text-cyan-glow"}>
        {balanceLabel}
      </span>
      <span className="text-platinum/45">Portfolio</span>
    </Link>
  );
}
