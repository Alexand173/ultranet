"use client";

import { Check, Copy } from "lucide-react";
import { useState } from "react";
import { formatAddress } from "@/lib/transaction-format";

interface TreasuryAddressProps {
  address: string | null;
  compact?: boolean;
}

export default function TreasuryAddress({ address, compact = false }: TreasuryAddressProps) {
  const [status, setStatus] = useState<"idle" | "copied" | "failed">("idle");

  const handleCopy = async () => {
    if (!address) return;
    try {
      await navigator.clipboard.writeText(address);
      setStatus("copied");
      window.setTimeout(() => setStatus("idle"), 1800);
    } catch {
      setStatus("failed");
    }
  };

  if (!address) return <span className="font-mono text-xs text-platinum/35">Treasury unavailable</span>;

  return (
    <span className="flex min-w-0 items-center gap-2">
      <span className="min-w-0 truncate font-mono text-xs text-platinum/70" title={address}>{formatAddress(address)}</span>
      <button
        type="button"
        onClick={() => void handleCopy()}
        aria-label={`Copy AppChain treasury address ${address}`}
        title={status === "copied" ? "Treasury address copied" : "Copy treasury address"}
        className={`inline-flex ${compact ? "h-7 w-7" : "h-8 w-8"} shrink-0 items-center justify-center border transition-colors focus-visible:outline-hidden focus-visible:ring-2 focus-visible:ring-cyan-glow ${status === "copied" ? "border-emerald-300/40 text-emerald-200" : status === "failed" ? "border-red-300/40 text-red-200" : "border-platinum/15 text-platinum/45 hover:border-cyan-glow/60 hover:text-cyan-glow"}`}
      >
        {status === "copied" ? <Check className="h-3.5 w-3.5" aria-hidden="true" /> : <Copy className="h-3.5 w-3.5" aria-hidden="true" />}
      </button>
      <span className="sr-only" aria-live="polite">{status === "copied" ? "Treasury address copied." : status === "failed" ? "Unable to copy treasury address." : ""}</span>
    </span>
  );
}
