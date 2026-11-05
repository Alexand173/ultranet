"use client";

import { motion } from "framer-motion";
import { usePathname } from "next/navigation";
import { useEffect, useState } from "react";
import { NOISE_FIELD } from "@/lib/deterministic-noise";

/**
 * Decorative background field: drifting technical labels and a particle sheet.
 *
 * The geometry comes from `NOISE_FIELD`, a seeded field computed once at module
 * scope. It used to come from `Math.random()` calls during render, which meant
 * the decoration was different on every render — unreproducible, and a
 * hydration mismatch waiting for the values to reach server-rendered markup.
 *
 * The mount gate stays: the decoration renders after hydration, so it never
 * reaches the server HTML and never delays first paint.
 */
export default function VisualNoise() {
  const pathname = usePathname();
  const [isMounted, setIsMounted] = useState(false);

  useEffect(() => {
    setIsMounted(true);
  }, []);

  if (!isMounted || pathname === "/ultraverse") return null;

  return (
    <div aria-hidden="true" className="fixed inset-0 pointer-events-none z-0 overflow-hidden">
      {/* Floating Technical Labels */}
      {NOISE_FIELD.labels.map((label) => (
        <motion.div
          key={label.id}
          initial={{
            x: `${label.xVw}vw`,
            y: `${label.yVh}vh`,
            opacity: 0.05,
          }}
          animate={{
            y: [null, `${label.driftPx}px`],
            opacity: [0.05, 0.1, 0.05],
          }}
          transition={{
            duration: label.duration,
            repeat: Infinity,
            ease: "linear",
          }}
          className="absolute font-mono text-[6px] text-cyan-glow uppercase tracking-[0.4em] whitespace-nowrap"
        >
          Visual_Decoration: {label.suffix} {"//"} NOT_LIVE
        </motion.div>
      ))}

      {/* Background Particles */}
      <svg className="w-full h-full opacity-20">
        {NOISE_FIELD.particles.map((particle) => (
          <motion.circle
            key={particle.id}
            cx={`${particle.cxPct}%`}
            cy={`${particle.cyPct}%`}
            r="0.5"
            fill="#0FFFFF"
            initial={{ opacity: 0.1 }}
            animate={{ opacity: [0.1, 0.5, 0.1] }}
            transition={{ duration: particle.duration, repeat: Infinity }}
          />
        ))}
      </svg>

      {/* Global Vignette */}
      <div className="absolute inset-0 shadow-[inset_0_0_200px_rgba(0,0,0,0.8)]" />
    </div>
  );
}
