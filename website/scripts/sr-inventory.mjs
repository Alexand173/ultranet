#!/usr/bin/env node
/**
 * Screen-reader inventory.
 *
 * Prints the structural facts a screen reader announces on each route: title,
 * first heading, heading outline, landmarks, live regions, tables, controls,
 * embedded frames and the first few tab stops. It exists so the manual audit
 * checklist carries *observed* expectations instead of remembered ones — run it
 * before an audit session and the per-route expectations can be re-derived
 * rather than trusted from a document that has since gone stale.
 *
 * It is not an audit and it cannot judge speech. It reports what is there; a
 * human decides whether what is there makes sense out loud.
 *
 * Usage:
 *   npm run dev
 *   npm run sr:inventory
 *   npm run sr:inventory -- --json sr-inventory.json
 *   npm run sr:inventory -- --routes /validator,/send-ultra
 */
import { writeFileSync } from "node:fs";
import process from "node:process";
import { chromium } from "playwright-core";
import { resolveChrome } from "./browser.mjs";
import { ROUTES } from "./routes.mjs";

function parseArgs(argv) {
  const options = { baseUrl: "http://localhost:3000", routes: ROUTES, json: null, tabStops: 4 };
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
    } else if (flag === "--tab-stops") {
      options.tabStops = Number(value);
      if (inline === undefined) i += 1;
    }
  }
  return options;
}

/** Runs in the page. Everything here must be serialisable. */
const COLLECT = () => {
  const text = (el) => (el?.textContent ?? "").replace(/\s+/g, " ").trim();
  const clip = (value, max = 90) => (value.length > max ? `${value.slice(0, max)}…` : value);
  // Only a *declared* name. Falling back to text content would print a nav's
  // link text as if it were its accessible name, which is exactly the sort of
  // plausible-looking wrong answer an audit cannot afford.
  const name = (el) => {
    const labelledBy = el.getAttribute("aria-labelledby");
    return (
      el.getAttribute("aria-label") ||
      (labelledBy && text(document.getElementById(labelledBy))) ||
      null
    );
  };
  const label = (el) => name(el) || clip(text(el), 60) || null;

  const landmarks = [
    ...document.querySelectorAll(
      "header, nav, main, footer, aside, form[aria-label], " +
        "[role=banner], [role=navigation], [role=main], [role=contentinfo], [role=complementary], [role=search]",
    ),
  ].map((el) => ({
    role: el.getAttribute("role") || el.tagName.toLowerCase(),
    name: name(el) || null,
  }));

  return {
    title: document.title,
    lang: document.documentElement.lang || null,
    skipLink: (() => {
      const link = document.querySelector('a[href="#main-content"]');
      return link ? { text: text(link), target: link.getAttribute("href") } : null;
    })(),
    mainCount: document.querySelectorAll("main, [role=main]").length,
    headings: [...document.querySelectorAll("h1,h2,h3,h4,h5,h6")].map((el) => ({
      level: Number(el.tagName[1]),
      text: clip(text(el)),
    })),
    landmarks,
    liveRegions: [
      ...document.querySelectorAll("[aria-live], [role=status], [role=alert], [role=log]"),
    ].map((el) => ({
      politeness: el.getAttribute("aria-live") || el.getAttribute("role"),
      text: clip(text(el), 60),
    })),
    frames: [...document.querySelectorAll("iframe")].map((el) => ({
      title: el.getAttribute("title"),
      src: el.getAttribute("src"),
    })),
    tables: [...document.querySelectorAll("table")].map((el) => ({
      caption: text(el.querySelector("caption")) || null,
      headers: [...el.querySelectorAll("th")].map((th) => text(th)).slice(0, 8),
    })),
    controls: [
      ...document.querySelectorAll("input, select, textarea, button, [role=button]"),
    ]
      .map((el) => ({
        tag: el.tagName.toLowerCase(),
        type: el.getAttribute("type"),
        name: label(el),
      }))
      .slice(0, 24),
    counts: {
      links: document.querySelectorAll("a[href]").length,
      images: document.querySelectorAll("img").length,
      imagesWithoutAlt: [...document.querySelectorAll("img")].filter((el) => !el.hasAttribute("alt"))
        .length,
      buttons: document.querySelectorAll("button, [role=button]").length,
    },
  };
};

/** Describe whatever currently has focus, for the tab-stop walk. */
const DESCRIBE_FOCUS = () => {
  const el = document.activeElement;
  if (!el || el === document.body) return "document body";
  const text = (el.textContent ?? "").replace(/\s+/g, " ").trim();
  const label =
    el.getAttribute("aria-label") || el.getAttribute("title") || (text ? text.slice(0, 70) : "");
  const id = el.id ? `#${el.id}` : "";
  return `${el.tagName.toLowerCase()}${id}${label ? ` “${label}”` : ""}`;
};

const options = parseArgs(process.argv.slice(2));
const browser = await chromium.launch({ executablePath: resolveChrome(), args: ["--no-sandbox"] });
const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
const report = [];

for (const route of options.routes) {
  const page = await context.newPage();
  try {
    await page.goto(options.baseUrl + route, { waitUntil: "domcontentloaded", timeout: 45000 });
    await page.waitForTimeout(2500);

    const facts = await page.evaluate(COLLECT);

    // First tab stops, taken from a clean focus position.
    const tabStops = [];
    await page.evaluate(() => {
      document.body.setAttribute("tabindex", "-1");
      document.body.focus();
    });
    for (let i = 0; i < options.tabStops; i += 1) {
      await page.keyboard.press("Tab");
      tabStops.push(await page.evaluate(DESCRIBE_FOCUS));
    }
    await page.evaluate(() => document.body.removeAttribute("tabindex"));

    report.push({ route, ...facts, tabStops });

    console.log(`\n${route}`);
    console.log(`  title:    ${facts.title}`);
    console.log(`  first h:  ${facts.headings[0] ? `h${facts.headings[0].level} “${facts.headings[0].text}”` : "none"}`);
    console.log(`  headings: ${facts.headings.map((h) => `h${h.level}`).join(" ")}`);
    console.log(
      `  landmarks: ${facts.landmarks.map((l) => `${l.role}${l.name ? `(${l.name})` : ""}`).join(" → ")}`,
    );
    console.log(`  mains:    ${facts.mainCount}`);
    console.log(`  skip:     ${facts.skipLink ? `${facts.skipLink.text} → ${facts.skipLink.target}` : "none"}`);
    console.log(
      `  live:     ${
        facts.liveRegions.length === 0
          ? "none"
          : facts.liveRegions.map((r) => `${r.politeness}:${r.text || "empty"}`).join(" | ")
      }`,
    );
    if (facts.frames.length) console.log(`  frames:   ${facts.frames.map((f) => f.title ?? f.src).join(", ")}`);
    if (facts.tables.length) console.log(`  tables:   ${facts.tables.length}`);
    console.log(`  counts:   ${facts.counts.links} links, ${facts.counts.buttons} buttons, ${facts.counts.images} images`);
    console.log(`  tabs:     ${tabStops.join("  →  ")}`);
  } catch (error) {
    report.push({ route, error: String(error).slice(0, 300) });
    console.log(`\n${route}\n  ERR  ${String(error).slice(0, 160)}`);
  } finally {
    await page.close();
  }
}

await browser.close();

if (options.json) {
  writeFileSync(
    options.json,
    JSON.stringify({ baseUrl: options.baseUrl, routes: options.routes.length, report }, null, 2),
  );
  console.log(`\nreport written to ${options.json}`);
}

console.log(`\n${report.length} routes inventoried. This is a description, not a pass.`);
