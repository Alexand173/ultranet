"use client";

import { useEffect, useState } from "react";
import Link from "next/link";
import { ChevronRight } from "lucide-react";
import TerminalFrame from "./TerminalFrame";

const LOG_LINES = [
  "> DOMINIE_ROOT_ACCESS // UNSTAINED_SIG_VERIFIED",
  "> LOADING_STATE_TRIE... OK",
  "> FHE_ENGINE_HANDSHAKE... COMPLETE",
  "> AWAITING_ROOT_COMMAND_",
];

export default function AdminConsoleTerminal({ className }: { className?: string }) {
  const [visibleLines, setVisibleLines] = useState(0);

  useEffect(() => {
    if (visibleLines >= LOG_LINES.length) return;
    const timeout = setTimeout(() => setVisibleLines((v) => v + 1), 600);
    return () => clearTimeout(timeout);
  }, [visibleLines]);

  return (
    <TerminalFrame
      title="Dominie_Root // Admin_Console"
      liveLabel="ROOT_TTY"
      cornerAccent="br"
      className={className}
    >
      <div className="flex flex-col h-full">
        {/* Window chrome */}
        <div className="flex items-center gap-2 px-6 py-3 border-b border-platinum/5 shrink-0">
          <span className="w-2.5 h-2.5 rounded-full bg-red-500/60" />
          <span className="w-2.5 h-2.5 rounded-full bg-amber-400/60" />
          <span className="w-2.5 h-2.5 rounded-full bg-cyan-glow/60" />
          <span className="ml-2 text-xs font-mono text-platinum/20 uppercase tracking-widest">
            root@sovereign:~
          </span>
        </div>

        {/* Log lines */}
        <div className="flex-1 px-6 py-4 font-mono text-base text-cyan-glow/70 space-y-3 overflow-hidden">
          {LOG_LINES.slice(0, visibleLines).map((line, i) => (
            <div key={i} className="flex items-center gap-1">
              <span className={i === visibleLines - 1 ? "" : "text-platinum/40"}>{line}</span>
              {i === visibleLines - 1 && <span className="blink-cursor text-cyan-glow">_</span>}
            </div>
          ))}
        </div>

        {/* Quick actions */}
        <div className="flex items-center gap-5 px-6 py-4 border-t border-platinum/5 shrink-0">
          <Link
            href="/#swarm"
            className="flex items-center gap-1.5 text-sm font-bold uppercase tracking-widest text-platinum/60 hover:text-cyan-glow transition-colors"
          >
            JOIN_SWARM
            <ChevronRight className="w-4 h-4" />
          </Link>
          <span className="text-platinum/10">|</span>
          <Link
            href="/docs"
            className="text-sm font-bold uppercase tracking-widest text-platinum/60 hover:text-cyan-glow transition-colors"
          >
            DOCS.md
          </Link>
        </div>
      </div>
    </TerminalFrame>
  );
}
