"use client";

import { AlertCircle, Check, Copy } from "lucide-react";
import { useState } from "react";

type CopyStatus = "idle" | "copied" | "error";

interface CopyMultiaddrButtonProps {
  value: string;
}

function statusMessage(status: CopyStatus): string {
  if (status === "copied") return "Multiaddr copied.";
  if (status === "error") return "Copy failed. Select the address manually.";
  return "";
}

export default function CopyMultiaddrButton({ value }: CopyMultiaddrButtonProps) {
  const [status, setStatus] = useState<CopyStatus>("idle");

  const handleCopy = async () => {
    try {
      if (!navigator.clipboard) throw new Error("Clipboard API unavailable");
      await navigator.clipboard.writeText(value);
      setStatus("copied");
    } catch {
      setStatus("error");
    }
  };

  const Icon = status === "copied" ? Check : status === "error" ? AlertCircle : Copy;
  const label = status === "copied" ? "Copied" : status === "error" ? "Copy failed" : "Copy multiaddr";

  return (
    <div className="flex flex-wrap items-center gap-3">
      <button
        type="button"
        onClick={() => void handleCopy()}
        className="inline-flex min-h-11 items-center justify-center gap-2 border border-cyan-glow/50 px-4 py-3 font-mono text-[10px] font-bold uppercase tracking-[0.16em] text-cyan-glow transition-colors hover:bg-cyan-glow hover:text-ink-black focus:outline-none focus:ring-2 focus:ring-cyan-glow focus:ring-offset-2 focus:ring-offset-ink-black"
        aria-label="Copy the full Genesis multiaddr"
      >
        <Icon className="h-4 w-4" aria-hidden="true" />
        {label}
      </button>
      <span className="font-mono text-[10px] text-platinum/45" role="status" aria-live="polite">
        {statusMessage(status)}
      </span>
    </div>
  );
}
