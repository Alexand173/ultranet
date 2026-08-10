"use client";

import { motion } from "framer-motion";
import { useEffect, useState } from "react";

export default function VisualNoise() {
  const [isMounted, setIsMounted] = useState(false);

  useEffect(() => {
    setIsMounted(true);
  }, []);

  if (!isMounted) return null;

  return (
    <div className="fixed inset-0 pointer-events-none z-0 overflow-hidden">
      {/* Floating Technical Labels */}
      {Array.from({ length: 12 }).map((_, i) => (
        <motion.div
          key={i}
          initial={{ 
            x: Math.random() * 100 + "vw", 
            y: Math.random() * 100 + "vh",
            opacity: 0.05
          }}
          animate={{ 
            y: [null, (Math.random() * -100) + "px"],
            opacity: [0.05, 0.1, 0.05]
          }}
          transition={{ 
            duration: 10 + Math.random() * 10, 
            repeat: Infinity, 
            ease: "linear" 
          }}
          className="absolute font-mono text-[6px] text-cyan-glow uppercase tracking-[0.4em] whitespace-nowrap"
        >
          System_Check: {Math.random().toString(16).substring(2, 8)} // OK
        </motion.div>
      ))}

      {/* Background Particles */}
      <svg className="w-full h-full opacity-20">
        {Array.from({ length: 40 }).map((_, i) => (
          <motion.circle
            key={i}
            cx={Math.random() * 100 + "%"}
            cy={Math.random() * 100 + "%"}
            r="0.5"
            fill="#0FFFFF"
            initial={{ opacity: 0.1 }}
            animate={{ opacity: [0.1, 0.5, 0.1] }}
            transition={{ duration: 2 + Math.random() * 4, repeat: Infinity }}
          />
        ))}
      </svg>

      {/* Global Vignette */}
      <div className="absolute inset-0 shadow-[inset_0_0_200px_rgba(0,0,0,0.8)]" />
    </div>
  );
}
