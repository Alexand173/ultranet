import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";
import {
  NOISE_FIELD,
  NOISE_LABEL_COUNT,
  NOISE_PARTICLE_COUNT,
  NOISE_SEED,
  buildNoiseField,
  mulberry32,
} from "./deterministic-noise";

/**
 * The decoration must be reproducible. It used to call Math.random() on every
 * render, so the field changed between renders and could never be matched by a
 * snapshot, a golden test, or a hydration pass.
 */
describe("seeded noise field", () => {
  it("builds the same field on every call", () => {
    expect(buildNoiseField()).toEqual(buildNoiseField());
    expect(buildNoiseField(NOISE_SEED)).toEqual(NOISE_FIELD);
  });

  it("builds a different field for a different seed", () => {
    const other = buildNoiseField(NOISE_SEED + 1);
    expect(other.labels[0]).not.toEqual(NOISE_FIELD.labels[0]);
    expect(other.labels[0].xVw).not.toBe(NOISE_FIELD.labels[0].xVw);
  });

  /**
   * Golden values. If this fails, the decoration visibly changes — which is a
   * design decision to be made on purpose, not a failing test to be updated.
   */
  it("matches its golden values", () => {
    expect(NOISE_FIELD.labels[0]).toEqual({
      id: "noise-0",
      xVw: 83.42,
      yVh: 78.94,
      driftPx: -56.58,
      duration: 12.57,
      suffix: "43bd29",
    });
    expect(NOISE_FIELD.particles[0]).toEqual({ id: 0, cxPct: 16.7, cyPct: 6.92, duration: 5.01 });
    expect(NOISE_FIELD.particles[NOISE_FIELD.particles.length - 1]).toEqual({
      id: 39,
      cxPct: 90.47,
      cyPct: 32.03,
      duration: 5.69,
    });
  });

  it("keeps the node counts the page expects", () => {
    expect(NOISE_FIELD.labels).toHaveLength(NOISE_LABEL_COUNT);
    expect(NOISE_FIELD.particles).toHaveLength(NOISE_PARTICLE_COUNT);
    expect(NOISE_LABEL_COUNT).toBe(12);
    expect(NOISE_PARTICLE_COUNT).toBe(40);
  });

  it("keeps every value inside its declared range", () => {
    for (const label of NOISE_FIELD.labels) {
      expect(label.xVw).toBeGreaterThanOrEqual(0);
      expect(label.xVw).toBeLessThan(100);
      expect(label.yVh).toBeGreaterThanOrEqual(0);
      expect(label.yVh).toBeLessThan(100);
      expect(label.driftPx).toBeLessThanOrEqual(-20);
      expect(label.driftPx).toBeGreaterThanOrEqual(-100);
      expect(label.duration).toBeGreaterThanOrEqual(10);
      expect(label.duration).toBeLessThanOrEqual(20);
      expect(label.suffix).toMatch(/^[0-9a-f]{6}$/);
    }

    for (const particle of NOISE_FIELD.particles) {
      expect(particle.cxPct).toBeGreaterThanOrEqual(0);
      expect(particle.cxPct).toBeLessThan(100);
      expect(particle.cyPct).toBeGreaterThanOrEqual(0);
      expect(particle.cyPct).toBeLessThan(100);
      expect(particle.duration).toBeGreaterThanOrEqual(2);
      expect(particle.duration).toBeLessThanOrEqual(6);
    }
  });

  it("does not place every node at the same point", () => {
    const columns = new Set(NOISE_FIELD.particles.map((particle) => particle.cxPct));
    expect(columns.size).toBeGreaterThan(1);
  });

  it("keeps identities unique so React keys stay stable", () => {
    const labelIds = new Set(NOISE_FIELD.labels.map((label) => label.id));
    const particleIds = new Set(NOISE_FIELD.particles.map((particle) => particle.id));
    expect(labelIds.size).toBe(NOISE_LABEL_COUNT);
    expect(particleIds.size).toBe(NOISE_PARTICLE_COUNT);
  });
});

describe("mulberry32", () => {
  it("stays inside [0, 1) and repeats exactly for a seed", () => {
    const first = mulberry32(42);
    const second = mulberry32(42);
    for (let i = 0; i < 500; i += 1) {
      const value = first();
      expect(value).toBeGreaterThanOrEqual(0);
      expect(value).toBeLessThan(1);
      expect(value).toBe(second());
    }
  });
});

describe("VisualNoise source guard", () => {
  it("never calls Math.random while rendering", () => {
    const source = readFileSync(
      path.join(process.cwd(), "src", "components", "ui", "VisualNoise.tsx"),
      "utf8",
    )
      // Strip comments first: prose about the old bug must not trip the guard.
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/^\s*\/\/.*$/gm, "");
    expect(source).not.toMatch(/Math\.random\s*\(/);
    expect(source).toContain("NOISE_FIELD");
  });
});
