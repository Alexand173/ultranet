"use client";

import Image from "next/image";
import { motion, useMotionValue, useSpring, useTransform } from "framer-motion";
import { useEffect, useRef } from "react";
import { usePathname } from "next/navigation";

const COIN_IMAGE = "/images/ultra-coin.png";

export default function GlobalCoin() {
  const coinRef = useRef<HTMLDivElement>(null);
  const pathname = usePathname();

  // Parallax Motion Values
  const mouseX = useMotionValue(0);
  const mouseY = useMotionValue(0);

  const springX = useSpring(mouseX, { stiffness: 100, damping: 30 });
  const springY = useSpring(mouseY, { stiffness: 100, damping: 30 });

  const rotateX = useTransform(springY, [-0.5, 0.5], [10, -10]);
  const rotateY = useTransform(springX, [-0.5, 0.5], [-10, 10]);

  useEffect(() => {
    const handleMouseMove = (e: MouseEvent) => {
      const x = (e.clientX / window.innerWidth) - 0.5;
      const y = (e.clientY / window.innerHeight) - 0.5;
      mouseX.set(x);
      mouseY.set(y);

      if (coinRef.current) {
        const rect = coinRef.current.getBoundingClientRect();
        const maskX = e.clientX - rect.left;
        const maskY = e.clientY - rect.top;
        coinRef.current.style.setProperty("--mask-x", `${maskX}px`);
        coinRef.current.style.setProperty("--mask-y", `${maskY}px`);
      }
    };
    window.addEventListener("mousemove", handleMouseMove);
    return () => window.removeEventListener("mousemove", handleMouseMove);
  }, [mouseX, mouseY]);

  // The Command Center home page and focused educational/auth/operator surfaces
  // already have their own visual hierarchy, so the persistent corner coin stays
  // hidden there instead of covering the content's primary visual.
  if (
    pathname === "/" ||
    pathname === "/validator" ||
    pathname === "/docs/whitepaper" ||
    pathname === "/login" ||
    pathname.startsWith("/operator")
  ) return null;

  return (
    <div className="fixed bottom-8 right-8 z-[100] pointer-events-none hidden lg:block">
      <motion.div
        style={{ rotateX, rotateY }}
        className="relative w-48 h-48 pointer-events-auto group cursor-help"
      >
        {/* Glow Aura */}
        <div className="absolute inset-0 bg-cyan-glow/10 blur-[40px] rounded-full animate-pulse" />

        {/* Orbital HUD */}
        <motion.div
          animate={{ rotate: 360 }}
          transition={{ duration: 20, repeat: Infinity, ease: "linear" }}
          className="absolute inset-0 border border-cyan-glow/20 border-dashed rounded-full"
        />

        {/* The Persistent Coin (exact shape supplied by engineer) */}
        <div
          ref={coinRef}
          className="relative w-full h-full rounded-full overflow-hidden border border-white/10 shadow-[0_0_50px_rgba(15,255,255,0.1)] bg-ink-black shimmer-sweep"
          style={{ "--mask-x": "50%", "--mask-y": "50%" } as React.CSSProperties}
        >
          {/* object-contain guarantees the full coin rim text is never cropped */}
          <Image
            src={COIN_IMAGE}
            alt="ULTRA Blockchain Network coin — decentralized, scalable, secure"
            fill
            sizes="192px"
            className="absolute inset-0 z-10 w-full h-full object-contain"
          />

          {/* Periodic flash pulse */}
          <div className="absolute inset-0 z-20 bg-white coin-flash-pulse pointer-events-none mix-blend-overlay" />

          {/* Cursor-tracked light bloom */}
          <div
            className="absolute inset-0 z-30 pointer-events-none mix-blend-overlay opacity-0 group-hover:opacity-70 transition-opacity duration-300"
            style={{ background: "radial-gradient(circle 70px at var(--mask-x) var(--mask-y), rgba(15, 255, 255, 0.4) 0%, transparent 70%)" }}
          />
        </div>

        {/* Micro Status Label */}
        <div className="absolute -top-6 left-1/2 -translate-x-1/2 font-mono text-[8px] text-cyan-glow/40 whitespace-nowrap uppercase tracking-widest">
          Node_Status: Active
        </div>
      </motion.div>
    </div>
  );
}
