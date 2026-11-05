import { readFileSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Motion is opt-in, not opt-out.
 *
 * Three separate places can animate without asking: the smooth-scroll library,
 * the CSS scroll behaviour, and the decorative background field. Fixing one and
 * forgetting the others is the failure mode this guards against — a reduced
 * motion user would still get animated scrolling from the CSS alone.
 */
const SRC_ROOT = path.join(process.cwd(), "src");
const read = (...segments: string[]) => readFileSync(path.join(SRC_ROOT, ...segments), "utf8");

describe("reduced motion preference", () => {
  it("keeps CSS smooth scrolling inside a no-preference query", () => {
    const css = read("styles", "globals.css");
    const guarded = css.match(
      /@media\s*\(prefers-reduced-motion:\s*no-preference\)\s*\{[^}]*scroll-behavior:\s*smooth/,
    );
    expect(guarded, "html { scroll-behavior: smooth } must sit inside a no-preference query").not.toBeNull();
  });

  it("does not start Lenis when reduced motion is requested", () => {
    const component = read("components", "ui", "SmoothScroll.tsx");
    expect(component).toContain("prefers-reduced-motion: reduce");
    // The preference is watched, not sampled once.
    expect(component).toMatch(/addEventListener\(\s*"change"/);
    expect(component).toMatch(/\.matches\s*\)\s*stop\(\)/);
  });

  it("stops the decorative field animating when reduced motion is requested", () => {
    const component = read("components", "ui", "VisualNoise.tsx");
    expect(component).toContain("useReducedMotion");
    expect(component).toMatch(/reduceMotion\s*\?/);
  });
});
