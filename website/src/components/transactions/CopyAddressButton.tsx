"use client";

import { AlertCircle, Check, Copy } from "lucide-react";
import { useState } from "react";
import type { CopyStatus } from "@/components/transactions/transaction.types";

interface CopyAddressButtonProps {
  value: string;
  label?: string;
}

export default function CopyAddressButton({ value, label = "Copy address" }: CopyAddressButtonProps) {
  const [status, setStatus] = useState<CopyStatus>("idle");
  const Icon = status === "copied" ? Check : status === "error" ? AlertCircle : Copy;
  const buttonLabel = status === "copied" ? "Copied" : status === "error" ? "Copy failed" : label;

  const handleCopy = async () => {
    try {
      if (!navigator.clipboard) throw new Error("Clipboard API unavailable");
      await navigator.clipboard.writeText(value);
      setStatus("copied");
    } catch {
      setStatus("error");
    }
  };

  return (
    <div className="flex flex-wrap items-center gap-3">
      <button
        type="button"
        onClick={() => void handleCopy()}
        className="inline-flex min-h-11 items-center gap-2 border border-platinum/20 px-4 py-3 font-mono text-[10px] font-bold uppercase tracking-[0.14em] text-platinum/70 transition-colors hover:border-cyan-glow/60 hover:text-cyan-glow focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
        aria-label={`Copy full ${label.toLowerCase()}`}
      >
        <Icon className="h-4 w-4" aria-hidden="true" />
        {buttonLabel}
      </button>
      <span role="status" aria-live="polite" className="font-mono text-[10px] text-platinum/40">
        {status === "copied" ? `${label} copied.` : status === "error" ? "Select the value manually." : ""}
      </span>
    </div>
  );
}
