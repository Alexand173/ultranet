"use client";

import { useEffect } from "react";
import Lenis from "lenis";

const REDUCED_MOTION_QUERY = "(prefers-reduced-motion: reduce)";

/**
 * Animated scrolling is the thing a reduced-motion preference asks us *not* to
 * do, so Lenis is not started when the preference is set — the document keeps
 * its native scrolling. The preference is watched rather than read once, so
 * changing it mid-session takes effect without a reload.
 */
export default function SmoothScroll({ children }: { children: React.ReactNode }) {
  useEffect(() => {
    const preference = window.matchMedia(REDUCED_MOTION_QUERY);
    let lenis: Lenis | null = null;

    const start = () => {
      if (lenis) return;
      lenis = new Lenis({
        duration: 1.2,
        easing: (t) => Math.min(1, 1.001 - Math.pow(2, -10 * t)),
        autoRaf: true,
      });
    };

    const stop = () => {
      lenis?.destroy();
      lenis = null;
    };

    const apply = () => {
      if (preference.matches) stop();
      else start();
    };

    apply();
    preference.addEventListener("change", apply);

    return () => {
      preference.removeEventListener("change", apply);
      stop();
    };
  }, []);

  return <>{children}</>;
}
