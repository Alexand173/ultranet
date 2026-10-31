import { readFileSync, readdirSync } from "node:fs";
import path from "node:path";
import { describe, expect, it } from "vitest";

/**
 * Text below the WCAG AA floor must use the opaque `platinum-muted` token,
 * never an alpha step of `platinum`.
 *
 * Measured on ink-black `#010B13`, an alpha step composites with the row it
 * sits in: /30 is 2.45:1, /40 is 3.51:1, /45 is 4.13:1 and even /50 only
 * reaches 4.49:1 on a `bg-platinum/5` row. The token holds 5.48:1 on ink,
 * 5.00:1 on a raised row and 5.15:1 over the cyan hover wash.
 */
const SUB_FLOOR_ALPHA = /text-platinum\/(?:10|15|20|25|30|35|40|45)\b/;
const SOURCE_EXTENSIONS = new Set([".ts", ".tsx", ".js", ".jsx"]);
const SRC_ROOT = path.join(process.cwd(), "src");
const STYLESHEET = path.join(SRC_ROOT, "styles", "globals.css");

function sourceFiles(directory: string): string[] {
  return readdirSync(directory, { withFileTypes: true }).flatMap((entry) => {
    const entryPath = path.join(directory, entry.name);
    if (entry.isDirectory()) return sourceFiles(entryPath);
    return SOURCE_EXTENSIONS.has(path.extname(entry.name)) ? [entryPath] : [];
  });
}

describe("muted text contrast floor", () => {
  it("never uses a platinum alpha step below the AA floor", () => {
    const offences = sourceFiles(SRC_ROOT).flatMap((file) =>
      readFileSync(file, "utf8")
        .split("\n")
        .flatMap((line, lineIndex) =>
          SUB_FLOOR_ALPHA.test(line)
            ? [`${path.relative(SRC_ROOT, file)}:${lineIndex + 1}  ${line.trim().slice(0, 140)}`]
            : [],
        ),
    );

    expect(
      offences,
      "use text-platinum-muted (5.48:1) instead of an alpha step that measures below 4.5:1",
    ).toEqual([]);
  });

  it("keeps the platinum-muted token at an AA-safe value", () => {
    const stylesheet = readFileSync(STYLESHEET, "utf8");

    expect(stylesheet).toMatch(/--color-platinum-muted:\s*#82878C/i);
    expect(stylesheet).toContain("--color-ink-black: #010B13");
  });
});
