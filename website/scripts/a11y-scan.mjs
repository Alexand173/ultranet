#!/usr/bin/env node
/**
 * Repeatable full-rule accessibility scan.
 *
 * Runs axe-core with its complete default rule set against every route, and —
 * unlike a plain page scan — also descends into embedded frames. The whitepaper
 * reader at /docs/whitepaper renders the generated guide inside an iframe, and
 * axe on the parent document cannot see inside it.
 *
 * Usage:
 *   npm run a11y                       # scans http://localhost:3000
 *   npm run a11y -- --base-url https://dashboard.ultranetwork.cc
 *   npm run a11y -- --routes /,/docs   # subset
 *   npm run a11y -- --json a11y.json   # machine-readable report
 *
 * The browser binary is resolved from A11Y_CHROME_PATH, then the usual system
 * locations, then Playwright's own cache. No browser is downloaded by install.
 */
import { readFileSync, writeFileSync } from "node:fs";
import { createRequire } from "node:module";
import process from "node:process";
import { chromium } from "playwright-core";
import { resolveChrome } from "./browser.mjs";
import { ROUTES } from "./routes.mjs";

const require = createRequire(import.meta.url);
const AXE_SOURCE = readFileSync(require.resolve("axe-core/axe.min.js"), "utf8");

function parseArgs(argv) {
  const options = { baseUrl: "http://localhost:3000", routes: ROUTES, json: null, width: 1440 };
  for (let i = 0; i < argv.length; i += 1) {
    const [flag, inline] = argv[i].split("=");
    const value = inline ?? argv[i + 1];
    if (flag === "--base-url") { options.baseUrl = value.replace(/\/$/, ""); if (inline === undefined) i += 1; }
    else if (flag === "--routes") { options.routes = value.split(",").map((r) => r.trim()).filter(Boolean); if (inline === undefined) i += 1; }
    else if (flag === "--json") { options.json = value; if (inline === undefined) i += 1; }
    else if (flag === "--width") { options.width = Number(value); if (inline === undefined) i += 1; }
  }
  return options;
}

const AUDIT = async () => {
  const result = await window.axe.run(document, { resultTypes: ["violations", "incomplete"] });
  return {
    violations: result.violations.map((v) => ({
      id: v.id,
      impact: v.impact,
      help: v.help,
      nodes: v.nodes.slice(0, 5).map((n) => ({ target: n.target.join(" "), summary: (n.failureSummary ?? "").slice(0, 220) })),
      nodeCount: v.nodes.length,
    })),
    incomplete: result.incomplete.map((v) => ({ id: v.id, nodeCount: v.nodes.length })),
  };
};

async function scanDocument(frameLike, label) {
  await frameLike.addScriptTag({ content: AXE_SOURCE });
  const { violations, incomplete } = await frameLike.evaluate(AUDIT);
  return { frame: label, violations, incomplete };
}

const options = parseArgs(process.argv.slice(2));
const browser = await chromium.launch({ executablePath: resolveChrome(), args: ["--no-sandbox"] });
const context = await browser.newContext({ viewport: { width: options.width, height: 900 }, reducedMotion: "reduce" });
const report = [];
let failureCount = 0;

for (const route of options.routes) {
  const page = await context.newPage();
  const scanned = [];
  try {
    const response = await page.goto(options.baseUrl + route, { waitUntil: "domcontentloaded", timeout: 45000 });
    await page.waitForTimeout(2200);

    scanned.push(await scanDocument(page, "document"));
    for (const frame of page.frames()) {
      if (frame === page.mainFrame()) continue;
      // Give embedded documents (the reader's generated HTML) their own settle.
      await frame.waitForTimeout(600).catch(() => {});
      scanned.push(await scanDocument(frame, frame.url() || "frame"));
    }

    const violations = scanned.flatMap((s) => s.violations.map((v) => ({ ...v, frame: s.frame })));
    const incomplete = scanned.reduce((total, s) => total + s.incomplete.reduce((n, i) => n + i.nodeCount, 0), 0);
    failureCount += violations.length;
    report.push({ route, status: response ? response.status() : null, frames: scanned.length, violations, incompleteContrast: incomplete });
    const marker = violations.length === 0 ? "ok  " : "FAIL";
    console.log(`${marker} ${route}  frames=${scanned.length} incomplete=${incomplete} violations=${violations.length}`);
    for (const v of violations) console.log(`       ${v.id} ×${v.nodeCount} — ${v.nodes[0]?.target ?? ""}`);
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
  writeFileSync(options.json, JSON.stringify({ baseUrl: options.baseUrl, width: options.width, report }, null, 2));
  console.log(`report written to ${options.json}`);
}

console.log(
  failureCount === 0
    ? `\n${report.length} routes scanned, 0 violations.`
    : `\n${failureCount} violation group(s) across ${report.length} routes.`,
);
process.exit(failureCount === 0 ? 0 : 1);
