"use client";

import React, { useState, useEffect, useCallback, useMemo, useRef } from "react";
import { useRouter } from "next/navigation";
import { motion, AnimatePresence } from "framer-motion";
import { Search, Command, CornerDownLeft } from "lucide-react";
import { CHAPTERS, getIcon, Chapter } from "@/lib/constants";
import { clsx } from "clsx";

export default function GlobalCommandSearch() {
  const [isOpen, setIsOpen] = useState(false);
  const [search, setSearch] = useState("");
  const [selectedIndex, setSelectedIndex] = useState(0);
  const router = useRouter();
  const inputRef = useRef<HTMLInputElement>(null);

  const filteredChapters = useMemo(() => {
    const results = CHAPTERS.filter((c) =>
      c.title.toLowerCase().includes(search.toLowerCase()) ||
      c.category.toLowerCase().includes(search.toLowerCase())
    );
    return results;
  }, [search]);

  useEffect(() => {
    const handleKeyDown = (e: KeyboardEvent) => {
      if (e.key === "k" && (e.metaKey || e.ctrlKey)) {
        e.preventDefault();
        setIsOpen((prev) => !prev);
      }
      if (e.key === "Escape") {
        setIsOpen(false);
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, []);

  useEffect(() => {
    if (!isOpen) return;

    setSearch("");
    setSelectedIndex(0);
    setTimeout(() => inputRef.current?.focus(), 100);

    const previousOverflow = document.body.style.overflow;
    document.body.style.overflow = "hidden";

    return () => {
      document.body.style.overflow = previousOverflow;
    };
  }, [isOpen]);

  const handleSelect = useCallback((chapter: Chapter) => {
    if (chapter.slug) {
      router.push(`/docs/${chapter.slug}`);
      setIsOpen(false);
    } else {
      // For now, if no slug, we just go to docs home (or stay)
      router.push(`/docs`);
      setIsOpen(false);
    }
  }, [router]);

  const handleKeyDown = (e: React.KeyboardEvent) => {
    if (e.key === "ArrowDown") {
      e.preventDefault();
      setSelectedIndex((prev) => (prev + 1) % filteredChapters.length);
    } else if (e.key === "ArrowUp") {
      e.preventDefault();
      setSelectedIndex((prev) => (prev - 1 + filteredChapters.length) % filteredChapters.length);
    } else if (e.key === "Enter") {
      e.preventDefault();
      if (filteredChapters[selectedIndex]) {
        handleSelect(filteredChapters[selectedIndex]);
      }
    }
  };

  return (
    <AnimatePresence>
      {isOpen && (
        <div className="fixed inset-0 z-[100] flex items-start justify-center pt-[15vh] px-4">
          <motion.div
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            exit={{ opacity: 0 }}
            className="absolute inset-0 bg-ink-black/80 backdrop-blur-md"
            onClick={() => setIsOpen(false)}
          />

          <motion.div
            role="dialog"
            aria-modal="true"
            aria-labelledby="command-search-title"
            initial={{ opacity: 0, y: -20, scale: 0.98 }}
            animate={{ opacity: 1, y: 0, scale: 1 }}
            exit={{ opacity: 0, y: -10, scale: 0.99 }}
            className="relative w-full max-w-2xl bg-[#010B13] border border-cyan-glow/30 shadow-[0_0_50px_rgba(15,255,255,0.15)] overflow-hidden"
          >
            {/* Search Input */}
            <div className="flex items-center gap-4 px-6 py-5 border-b border-platinum/10">
              <Search className="w-5 h-5 text-cyan-glow animate-pulse" aria-hidden="true" />
              <label id="command-search-title" htmlFor="command-search-input" className="sr-only">Search protocol documentation</label>
              <input
                id="command-search-input"
                ref={inputRef}
                type="text"
                value={search}
                onChange={(e) => setSearch(e.target.value)}
                onKeyDown={handleKeyDown}
                placeholder="EXECUTE PROTOCOL SEARCH..."
                role="combobox"
                aria-controls="command-search-results"
                aria-expanded="true"
                aria-activedescendant={filteredChapters[selectedIndex] ? `command-search-option-${filteredChapters[selectedIndex].id}` : undefined}
                className="flex-1 bg-transparent border-none outline-none font-mono text-lg text-platinum uppercase tracking-widest placeholder:text-platinum/20"
              />
              <div className="flex items-center gap-2 px-2 py-1 bg-platinum/5 border border-platinum/10 rounded font-mono text-[10px] text-platinum/30 uppercase tracking-widest">
                <Command className="w-3 h-3" />
                <span>K</span>
              </div>
            </div>

            {/* Results List */}
            <div id="command-search-results" role="listbox" aria-label="Protocol documentation results" className="max-h-[50vh] overflow-y-auto custom-scrollbar">
              {filteredChapters.length > 0 ? (
                <div className="py-2">
                  {filteredChapters.map((chapter, index) => (
                    <div
                      id={`command-search-option-${chapter.id}`}
                      key={chapter.id}
                      role="option"
                      aria-selected={selectedIndex === index}
                      tabIndex={-1}
                      onMouseEnter={() => setSelectedIndex(index)}
                      onClick={() => handleSelect(chapter)}
                      className={clsx(
                        "group flex items-center justify-between px-6 py-4 cursor-pointer transition-colors relative",
                        selectedIndex === index ? "bg-cyan-glow/10" : "hover:bg-platinum/5"
                      )}
                    >
                      {selectedIndex === index && (
                        <div className="absolute left-0 top-0 bottom-0 w-[2px] bg-cyan-glow shadow-[0_0_8px_rgba(15,255,255,0.8)]" />
                      )}
                      
                      <div className="flex items-center gap-4">
                        <div className={clsx(
                          "p-2 border rounded-sm transition-colors",
                          selectedIndex === index ? "border-cyan-glow/50 text-cyan-glow" : "border-platinum/10 text-platinum/30"
                        )}>
                          {getIcon(chapter.iconName)}
                        </div>
                        <div>
                          <div className={clsx(
                            "font-mono text-[10px] uppercase tracking-widest mb-0.5",
                            selectedIndex === index ? "text-cyan-glow/70" : "text-platinum/30"
                          )}>
                            {chapter.category} {"//"} CH_{chapter.id.toString().padStart(2, '0')}
                          </div>
                          <div className={clsx(
                            "font-bold text-sm uppercase tracking-tight transition-colors",
                            selectedIndex === index ? "text-platinum" : "text-platinum/60"
                          )}>
                            {chapter.title}
                          </div>
                        </div>
                      </div>

                      <div className="flex items-center gap-3">
                        {!chapter.slug && (
                          <span className="font-mono text-[9px] text-platinum/20 uppercase tracking-widest border border-platinum/10 px-1.5 py-0.5">
                            Encrypted
                          </span>
                        )}
                        <div className={clsx(
                          "opacity-0 transition-opacity",
                          selectedIndex === index && "opacity-100"
                        )}>
                          <CornerDownLeft className="w-4 h-4 text-cyan-glow/50" />
                        </div>
                      </div>
                    </div>
                  ))}
                </div>
              ) : (
                <div className="py-20 text-center font-mono">
                  <div className="text-xs text-platinum/20 uppercase tracking-[0.4em] mb-2">
                    [ 0_MATCHES_FOUND ]
                  </div>
                  <div className="text-[10px] text-platinum/40 uppercase">
                    Ref_Error: Search_string_not_indexed
                  </div>
                </div>
              )}
            </div>

            {/* Footer / Shortcuts */}
            <div className="flex items-center justify-between px-6 py-3 bg-platinum/5 border-t border-platinum/10 font-mono text-[9px] text-platinum/30 uppercase tracking-[0.2em]">
              <div className="flex gap-6">
                <div className="flex items-center gap-1.5">
                  <span className="p-1 border border-platinum/10 rounded leading-none">↑↓</span>
                  <span>Navigate</span>
                </div>
                <div className="flex items-center gap-1.5">
                  <span className="p-1 border border-platinum/10 rounded leading-none">Enter</span>
                  <span>Select</span>
                </div>
              </div>
              <div>
                Sovereign_OS // Command_v1.0
              </div>
            </div>
          </motion.div>
        </div>
      )}
    </AnimatePresence>
  );
}
