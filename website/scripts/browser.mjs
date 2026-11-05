/**
 * Shared browser resolution for the scan scripts.
 *
 * `playwright-core` deliberately ships no browser, so the binary is resolved
 * from A11Y_CHROME_PATH, then the usual system locations, then Playwright's
 * own cache. Shared by a11y-scan.mjs and hydration-check.mjs so the two cannot
 * drift apart on the machine-specific part.
 */
import { readFileSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import path from "node:path";
import process from "node:process";

export function resolveChrome() {
  if (process.env.A11Y_CHROME_PATH) return process.env.A11Y_CHROME_PATH;
  const candidates = [
    "/usr/bin/google-chrome",
    "/usr/bin/google-chrome-stable",
    "/usr/bin/chromium",
    "/usr/bin/chromium-browser",
    "/opt/google/chrome/chrome",
    "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
  ];
  for (const candidate of candidates) {
    try {
      readFileSync(candidate);
      return candidate;
    } catch {
      // try the next one
    }
  }
  // Playwright's own downloaded Chromium, if the cache exists.
  const cacheRoot = path.join(homedir(), ".cache/ms-playwright");
  try {
    const builds = readdirSync(cacheRoot)
      .filter((entry) => entry.startsWith("chromium-"))
      .sort()
      .reverse();
    for (const build of builds) {
      for (const dir of ["chrome-linux64", "chrome-linux"]) {
        const candidate = path.join(cacheRoot, build, dir, "chrome");
        try {
          readFileSync(candidate);
          return candidate;
        } catch {
          // keep looking
        }
      }
    }
  } catch {
    // no cache directory
  }
  throw new Error(
    "No Chromium binary found. Set A11Y_CHROME_PATH to a Chrome/Chromium executable, " +
      "or install one via your package manager.",
  );
}
