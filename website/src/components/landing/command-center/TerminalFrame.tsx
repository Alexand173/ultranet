"use client";

import { ReactNode } from "react";
import { cn } from "@/lib/utils";

interface TerminalFrameProps {
  title: string;
  liveLabel?: string;
  children: ReactNode;
  className?: string;
  contentClassName?: string;
  cornerAccent?: "tl" | "tr" | "bl" | "br";
}

export default function TerminalFrame({
  title,
  liveLabel,
  children,
  className,
  contentClassName,
  cornerAccent = "tl",
}: TerminalFrameProps) {
  return (
    <div
      className={cn(
        "relative flex flex-col bg-ink-black/70 border border-platinum/10 hover:border-cyan-glow/40 neon-flicker-border overflow-hidden group",
        className
      )}
    >
      {/* Corner bracket accent */}
      <div
        className={cn(
          "absolute w-6 h-6 border-cyan-glow/30 group-hover:border-cyan-glow/70 pointer-events-none z-20",
          cornerAccent === "tl" && "top-0 left-0 border-t-2 border-l-2",
          cornerAccent === "tr" && "top-0 right-0 border-t-2 border-r-2",
          cornerAccent === "bl" && "bottom-0 left-0 border-b-2 border-l-2",
          cornerAccent === "br" && "bottom-0 right-0 border-b-2 border-r-2"
        )}
      />

      {/* Header */}
      <div className="flex items-center justify-between px-6 py-4 border-b border-platinum/10 bg-platinum/[0.02] shrink-0">
        <span className="text-sm font-mono font-bold uppercase tracking-[0.15em] text-platinum/70">
          {title}
        </span>
        {liveLabel && (
          <span className="flex items-center gap-2 text-xs font-mono uppercase tracking-widest text-cyan-glow/70">
            <span className="w-1.5 h-1.5 rounded-full bg-cyan-glow animate-pulse shadow-[0_0_6px_#0FFFFF]" />
            {liveLabel}
          </span>
        )}
      </div>

      {/* Content */}
      <div className={cn("relative flex-1 overflow-hidden", contentClassName)}>
        {children}
      </div>
    </div>
  );
}
