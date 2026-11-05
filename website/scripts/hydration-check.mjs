#!/usr/bin/env node
/**
 * Hydration mismatch check.
 *
 * React reports a hydration mismatch only in a development build, only once,
 * and only to the browser console — nowhere axe or a production deploy can see
 * it. This script loads every route against a dev server with the console
 * captured, and fails if React says the server and client disagreed.
 *
 * A mismatch means the browser discarded server HTML and re-rendered. It is
 * invisible to users on a fast local load and very visible on a slow one: the
 * page can flash, and any state in the discarded tree is lost.
 *
 * Usage:
 *   npm run dev            # in another terminal — this must be a dev build
 *   npm run hydration
 *   npm run hydration -- --base-url http://localhost:3001
 *   npm run hydration -- --routes /,/login
 *   npm run hydration -- --json hydration.json
 *
 * The browser binary is resolved from A11Y_CHROME_PATH, then the usual system
 * locations, then Playwright's own cache. No browser is downloaded by install.
 */
import { writeFileSync } from "node:fs";
import process from "node:process";
import { chromium } from "playwright-core";
import { resolveChrome } from "./browser.mjs";
import { ROUTES } from "./routes.mjs";

/**
 * React's own wording, plus its minified codes, plus the Next.js phrasing.
 * #418 is "Hydration failed because the server rendered HTML didn't match the
 * client", #423 "error while hydrating", #425 "Text content does not match".
 */
const HYDRATION_SIGNATURES = [
  /hydrat/i,
  /#418\b/,
  /#423\b/,
  /#425\b/,
  /did not match/i,
  /did not expect server HTML/i,
  /server rendered text didn't match/i,
];

function parseArgs(argv) {
  const options = { baseUrl: "http://localhost:3000", routes: ROUTES, json: null };
  for (let i = 0; i < argv.length; i += 1) {
    const [flag, inline] = argv[i].split("=");
    const value = inline ?? argv[i + 1];
    if (flag === "--base-url") {
      options.baseUrl = value.replace(/\/$/, "");
      if (inline === undefined) i += 1;
    } else if (flag === "--routes") {
      options.routes = value.split(",").map((r) => r.trim()).filter(Boolean);
      if (inline === undefined) i += 1;
    } else if (flag === "--json") {
      options.json = value;
      if (inline === undefined) i += 1;
    }
  }
  return options;
}

function matches(message) {
  return HYDRATION_SIGNATURES.some((signature) => signature.test(message));
}

const options = parseArgs(process.argv.slice(2));
const browser = await chromium.launch({ executablePath: resolveChrome(), args: ["--no-sandbox"] });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const report = [];
let failureCount = 0;
let devBuildConfirmed = false;

for (const route of options.routes) {
  const page = await context.newPage();
  const findings = [];
  const seen = new Set();

  const record = (source, text) => {
    const trimmed = text.replace(/\s+/g, " ").trim().slice(0, 500);
    if (!matches(trimmed)) return;
    const key = `${source}:${trimmed}`;
    if (seen.has(key)) return;
    seen.add(key);
    findings.push({ source, text: trimmed });
  };

  // Listeners must be attached before navigation: React reports the mismatch
  // once, during hydration, and never repeats it.
  page.on("console", (message) => {
    if (message.type() === "error" || message.type() === "warning") record("console", message.text());
  });
  page.on("pageerror", (error) => record("pageerror", String(error?.message ?? error)));

  try {
    const response = await page.goto(options.baseUrl + route, {
      waitUntil: "domcontentloaded",
      timeout: 45000,
    });
    // Hydration happens after the HTML lands; give effects a chance to settle.
    await page.waitForTimeout(3000);

    // Components revealed on scroll mount later — sweep once so they hydrate too.
    await page.evaluate(() => window.scrollTo(0, Math.round(document.body.scrollHeight / 2)));
    await page.waitForTimeout(900);
    await page.evaluate(() => window.scrollTo(0, 0));
    await page.waitForTimeout(400);

    // Next injects a <nextjs-portal> for its dev tools; production builds have none.
    const isDevBuild = await page.evaluate(() => Boolean(document.querySelector("nextjs-portal")));
    if (isDevBuild) devBuildConfirmed = true;

    failureCount += findings.length;
    report.push({ route, status: response ? response.status() : null, devBuild: isDevBuild, findings });
    console.log(
      `${findings.length === 0 ? "ok  " : "FAIL"} ${route}  dev=${isDevBuild ? "yes" : "no"}  messages=${findings.length}`,
    );
    for (const finding of findings) console.log(`       ${finding.source}: ${finding.text.slice(0, 200)}`);
  } catch (error) {
    failureCount += 1;
    report.push({ route, error: String(error).slice(0, 300) });
    console.log(`ERR  ${route}  ${String(error).slice(0, 160)}`);
  } finally {
    await page.close();
  }
}

await browser.close();

if (options.json) {
  writeFileSync(
    options.json,
    JSON.stringify({ baseUrl: options.baseUrl, devBuildConfirmed, report }, null, 2),
  );
  console.log(`report written to ${options.json}`);
}

if (!devBuildConfirmed) {
  console.warn(
    "\nwarning: this looks like a production build. React only reports hydration\n" +
      "mismatches in development, so a clean result here proves nothing. Run\n" +
      "`npm run dev` and point this check at it.",
  );
}

console.log(
  failureCount === 0
    ? `\n${report.length} routes checked, 0 hydration messages.`
    : `\n${failureCount} hydration message(s) across ${report.length} routes.`,
);
process.exit(failureCount === 0 ? 0 : 1);
