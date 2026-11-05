/**
 * The decorative noise field, generated deterministically.
 *
 * `VisualNoise` used to call `Math.random()` while rendering — roughly 170
 * calls per render. Any component that does that cannot be reproduced: React
 * may render it more than once (StrictMode in development, a `usePathname()`
 * change, any parent update), and each pass produced a different layout, so
 * the decoration reshuffled and the markup was never stable.
 *
 * A seeded generator fixes the cause rather than the symptom. The field is
 * built once at module scope from a fixed seed, so every render, every client
 * and every build compute exactly the same numbers — which is also what makes
 * the field testable.
 */

/** 32-bit PRNG (mulberry32). Small, fast, and identical across runtimes. */
export function mulberry32(seed: number): () => number {
  let a = seed >>> 0;
  return () => {
    a = (a + 0x6d2b79f5) >>> 0;
    let t = Math.imul(a ^ (a >>> 15), 1 | a);
    t = (t + Math.imul(t ^ (t >>> 7), 61 | t)) ^ t;
    return ((t ^ (t >>> 14)) >>> 0) / 4294967296;
  };
}

/** A floating technical label. Coordinates are unitless until render. */
export interface NoiseLabel {
  id: string;
  /** Horizontal start, in viewport-width percent. */
  xVw: number;
  /** Vertical start, in viewport-height percent. */
  yVh: number;
  /** Total upward drift over one cycle, in px (negative = up). */
  driftPx: number;
  /** Seconds for one full cycle. */
  duration: number;
  /** Six hex characters, the label's decorative suffix. */
  suffix: string;
}

/** A background particle, positioned in percent of the SVG box. */
export interface NoiseParticle {
  id: number;
  cxPct: number;
  cyPct: number;
  /** Seconds for one pulse. */
  duration: number;
}

export interface NoiseField {
  labels: NoiseLabel[];
  particles: NoiseParticle[];
}

/**
 * Fixed seed. Changing it changes the visible decoration, so it is a reviewed
 * edit, not a knob — the golden test pins the field it produces.
 */
export const NOISE_SEED = 0x5eed1a7e;

/** Same counts as the pre-seed implementation, so the page looks unchanged. */
export const NOISE_LABEL_COUNT = 12;
export const NOISE_PARTICLE_COUNT = 40;

/** Two decimals: enough for sub-pixel placement, stable in a golden test. */
function round(value: number): number {
  return Math.round(value * 100) / 100;
}

/**
 * Build the decoration. The loop counts are constants, so the sequence of
 * `random()` calls is fixed and the result is fully reproducible.
 */
export function buildNoiseField(seed: number = NOISE_SEED): NoiseField {
  const random = mulberry32(seed);

  const labels: NoiseLabel[] = Array.from({ length: NOISE_LABEL_COUNT }, (_, index) => ({
    id: `noise-${index}`,
    xVw: round(random() * 100),
    yVh: round(random() * 100),
    driftPx: -round(20 + random() * 80),
    duration: round(10 + random() * 10),
    suffix: Math.floor(random() * 0xffffff)
      .toString(16)
      .padStart(6, "0"),
  }));

  const particles: NoiseParticle[] = Array.from({ length: NOISE_PARTICLE_COUNT }, (_, index) => ({
    id: index,
    cxPct: round(random() * 100),
    cyPct: round(random() * 100),
    duration: round(2 + random() * 4),
  }));

  return { labels, particles };
}

/**
 * The field itself: computed once per process, never per render. Server and
 * client therefore agree without any effect, hydration dance, or state.
 */
export const NOISE_FIELD: NoiseField = buildNoiseField();
