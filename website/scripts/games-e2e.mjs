#!/usr/bin/env node
/**
 * End-to-end check of the Games pages in a real browser.
 *
 * A stand-in for the Games service answers with the shapes the real service gives (captured in
 * src/lib/games-api-contract.json by a Rust test that drives the real routes), and Chromium drives the built site:
 * browsing without a wallet, search and filters, loading/empty/error states, restoring a wallet, signing in, playing,
 * the reserved-versus-paid statuses, owner actions, accessibility (axe) and layout at three widths.
 *
 * The site must be built with NEXT_PUBLIC_API_BASE_URL=http://127.0.0.1:18095 and started on port 3100:
 *   NEXT_PUBLIC_API_BASE_URL=http://127.0.0.1:18095 npm run build && npx next start -p 3100
 *   node scripts/games-e2e.mjs
 * Nothing here touches a real service, wallet or ledger: the wallet is a throwaway restored from a public test phrase.
 */
import http from "node:http";
import { createRequire } from "node:module";
import { mkdirSync, readFileSync } from "node:fs";
import process from "node:process";
import { chromium } from "playwright-core";
import { sha3_256 } from "@noble/hashes/sha3.js";
import { resolveChrome } from "./browser.mjs";

const require = createRequire(import.meta.url);
const AXE = readFileSync(require.resolve("axe-core/axe.min.js"), "utf8");
const contract = JSON.parse(readFileSync(new URL("../src/lib/games-api-contract.json", import.meta.url), "utf8"));
const SITE = process.env.GAMES_SITE ?? "http://127.0.0.1:3100";
const API_PORT = 18095;
const SHOTS = process.env.GAMES_SHOTS ?? "/home/valerian/tmp/games-shots";
const PHRASE = "abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon abandon about";
const PASSWORD = "correct horse battery staple";
mkdirSync(SHOTS, { recursive: true });

// ---------------------------------------------------------------- the stand-in service ----------------------
const IDS = { race: "a1".repeat(16), archive: "b2".repeat(16), echo: "c3".repeat(16) };
const OTHER_OWNER = "cd".repeat(32);
const state = { mode: "ok", wallet: null, token: "tok-" + "9".repeat(60), stage: "reward_reserved", payout: "submitted", requests: [], registered: [] };

const store = () => [
  { ...contract.store.games[0], game_id: IDS.race, chain_id: 7, name: "Skyline Racer", description: "Race the signal across an AppChain grid.", status: "active", validation_scheme: "verified_adapter", adapter_hash: "ee".repeat(32), owner: state.wallet ?? OTHER_OWNER, playable: true, playable_reason: null, rewards: "available", reward_units: 100, rewards_reason: null },
  { ...contract.store.games[0], game_id: IDS.archive, chain_id: 1, name: "Chess Archive", description: "Chess results are recorded, not verified.", status: "active", validation_scheme: "record_only", adapter_hash: null, owner: OTHER_OWNER, playable: false, playable_reason: "record_only_no_approved_adapter", rewards: "none", reward_units: null, rewards_reason: "not_playable" },
  { ...contract.store.games[0], game_id: IDS.echo, chain_id: 9, name: "Echo Arena", description: "Playable now; the owner has not set a reward policy.", status: "active", validation_scheme: "verified_adapter", adapter_hash: "dd".repeat(32), owner: OTHER_OWNER, playable: true, playable_reason: null, rewards: "none", reward_units: null, rewards_reason: "no_approved_reward_policy" },
  ...state.registered,
];

const play = () => ({ ...contract.play, game_id: IDS.race, result_id: "web-test", stage: state.stage, payout_state: state.payout, reward_units: 100, not_rewarded: null, outcome: "win" });

function respond(res, status, body) {
  res.writeHead(status, {
    "Content-Type": "application/json",
    "Access-Control-Allow-Origin": SITE,
    "Access-Control-Allow-Headers": "Content-Type, Accept, X-Wallet-Session",
    "Access-Control-Allow-Methods": "GET, POST, OPTIONS",
  });
  res.end(JSON.stringify(body));
}

const server = http.createServer((req, res) => {
  const url = new URL(req.url, "http://x");
  let raw = "";
  req.on("data", (chunk) => (raw += chunk));
  req.on("end", () => {
    if (req.method === "OPTIONS") return respond(res, 204, {});
    let body = null;
    try { body = raw ? JSON.parse(raw) : null; } catch { body = null; }
    state.requests.push({ method: req.method, path: url.pathname + url.search, body, session: req.headers["x-wallet-session"] ?? null });
    const path = url.pathname;
    if (path === "/__mode") { state.mode = url.searchParams.get("m"); return respond(res, 200, { ok: true }); }
    if (path === "/__confirm") { state.stage = "reward_paid"; state.payout = "confirmed"; return respond(res, 200, { ok: true }); }
    if (path === "/api/appchain/overview") {
      return respond(res, 200, { success: true, chains: [
        { id: 7, name: "Arcade", owner: state.wallet ?? OTHER_OWNER, account_address: state.wallet ?? OTHER_OWNER, genesis_root: "00".repeat(32), anchor_fee: "1000", balance: "0", anchor_spend: "0", anchor_count: 0, anchor_availability: "production", source: "ledger" },
        { id: 1, name: "Arcade", owner: OTHER_OWNER, account_address: OTHER_OWNER, genesis_root: "00".repeat(32), anchor_fee: "1000", balance: "0", anchor_spend: "0", anchor_count: 0, anchor_availability: "production", source: "ledger" },
      ], totals: { anchor_count: 0, anchor_spend: "0" }, anchor_availability: "production" });
    }
    if (!path.startsWith("/api/games/")) return respond(res, 404, { success: false, message: "not found" });
    const route = path.slice("/api/games".length);
    if (route === "/store") {
      if (state.mode === "error") return respond(res, 503, { success: false, code: "payouts_paused", message: "The Games service is busy. Try again shortly." });
      return respond(res, 200, { success: true, games: state.mode === "empty" ? [] : store() });
    }
    let m;
    if ((m = route.match(/^\/players\/([0-9a-f]{64})\/results$/))) {
      state.wallet = m[1];
      return respond(res, 200, { success: true, results: [{ ...contract.player_results.results[0], game_id: IDS.race, result_id: "earlier-1", reward_units: 100, stage: "reward_paid", payout_state: "confirmed", transaction_hash: "ab".repeat(32) }, { ...contract.player_results.results[0], game_id: IDS.race, result_id: "earlier-2", reward_units: 100, stage: "reward_reserved", payout_state: "queued", transaction_hash: null }] });
    }
    if (route === "/auth/challenge") {
      state.wallet = body.address;
      const id = "0f".repeat(32);
      const issued = Math.floor(Date.now() / 1000);
      const expires = issued + 300;
      const parts = [Buffer.from("UltraNet/wallet-challenge/v1")];
      for (const text of ["play", body.address]) { const b = Buffer.from(text); const len = Buffer.alloc(4); len.writeUInt32LE(b.length); parts.push(len, b); }
      const times = Buffer.alloc(16); times.writeBigUInt64LE(BigInt(issued), 0); times.writeBigUInt64LE(BigInt(expires), 8);
      parts.push(Buffer.from(id, "hex"), times);
      const digest = Buffer.from(sha3_256(Buffer.concat(parts))).toString("hex");
      return respond(res, 200, { success: true, challenge_id: id, address: body.address, purpose: "play", issued_at: issued, expires_at: expires, digest });
    }
    if (route === "/auth/verify") {
      const address = Buffer.from(sha3_256(Buffer.from(body.public_key, "hex"))).toString("hex");
      const wellFormed = body.public_key.length === 2592 * 2 && body.signature.length === 4627 * 2 && address === state.wallet;
      if (!wellFormed) return respond(res, 401, { success: false, code: "auth_failed", message: "the sign-in could not be verified" });
      return respond(res, 200, { success: true, token: state.token, address, expires_at: Math.floor(Date.now() / 1000) + 3600 });
    }
    if (route === "/auth/logout") return respond(res, 200, { success: true, signed_out: true });
    if ((m = route.match(/^\/play\/([0-9a-f]{32})$/))) {
      if (req.headers["x-wallet-session"] !== state.token) return respond(res, 401, { success: false, code: "auth_required", message: "sign in with your wallet first" });
      state.stage = "reward_reserved"; state.payout = "submitted";
      return respond(res, 200, play());
    }
    if ((m = route.match(/^\/results\/[0-9a-f]{32}\/.+$/))) return respond(res, 200, play());
    if ((m = route.match(/^\/pools\/([0-9a-f]{32})$/))) return respond(res, 200, { ...contract.pool, game_id: m[1], funded: 10_000, reserved: 100, paid: 0, withdrawn: 0, fees_reserved: 1, fees_paid: 0, available: 9_899 });
    if (route.endsWith("/deposits")) return respond(res, 200, { success: true, deposits: [{ ...contract.deposits.deposits[0], funder: state.wallet ?? OTHER_OWNER, amount_units: 10_000 }] });
    if (route.endsWith("/policies")) return respond(res, 200, { success: true, policies: [{ ...contract.policies.policies[0], version: 1, reward_units: 100, eligible_outcomes: "win", per_player_per_day: 3, per_game_per_day: 100, status: "approved" }] });
    if (route.endsWith("/payouts")) return respond(res, 200, { success: true, payouts: [{ ...contract.payouts.payouts[0], kind: "reward", amount_units: 100, to_address: "11".repeat(32), state: "submitted", tx_hash: "ab".repeat(32) }] });
    if (route === "/catalog" && req.method === "POST") {
      const id = "d4".repeat(16);
      state.registered.push({ ...contract.store.games[0], game_id: id, chain_id: body.chain_id, name: body.name, description: body.description, status: "active", validation_scheme: "record_only", adapter_hash: null, owner: state.wallet, playable: false, playable_reason: "adapter_module_not_installed", rewards: "none", reward_units: null, rewards_reason: "not_playable" });
      return respond(res, 200, { ...contract.catalog_register, game_id: id, chain_id: body.chain_id, name: body.name, description: body.description });
    }
    if (route.endsWith("/fund") || route.endsWith("/withdraw")) return respond(res, 200, { ...contract.pool, game_id: IDS.race, funded: 10_000, available: 9_899 });
    if (route.endsWith("/policy")) return respond(res, 200, { ...contract.policy_propose, game_id: IDS.race, version: 2, reward_units: body.reward_units, status: "proposed" });
    return respond(res, 404, { success: false, code: "not_found", message: "no such route" });
  });
});

// ---------------------------------------------------------------- the checks ------------------------------------
const failures = [];
let passed = 0;
function check(name, ok, detail = "") {
  if (ok) { passed += 1; console.log(`  ok   ${name}`); } else { failures.push(`${name} ${detail}`); console.log(`  FAIL ${name} ${detail}`); }
}

// The site-wide wallet balance pill (.bottom-5, WalletBalancePill) is excluded: it is not part of Games, and its own
// accessible-name finding is reported separately.
async function axe(page, label) {
  await page.evaluate(AXE);
  const result = await page.evaluate(async () => (await window.axe.run({ exclude: [[".bottom-5"]] }, { resultTypes: ["violations"] })).violations.map((v) => ({ id: v.id, impact: v.impact, nodes: v.nodes.slice(0, 3).map((n) => n.target.join(" ")) })));
  const bad = result.filter((v) => v.impact === "serious" || v.impact === "critical");
  check(`axe: no serious or critical issues on ${label}`, bad.length === 0, JSON.stringify(bad));
  const minor = result.filter((v) => !bad.includes(v));
  if (minor.length) console.log(`       (minor/moderate on ${label}: ${minor.map((v) => v.id).join(", ")})`);
}

// A full page load locks the local wallet (by design), so after the wallet is unlocked the script moves like a user does.
const clientNav = (page, url) => page.evaluate((target) => window.next.router.push(target), url);
async function waitText(page, pattern, label, timeout = 120_000) {
  try {
    await page.getByText(pattern).first().waitFor({ timeout });
  } catch (error) {
    const shown = (await page.locator("main").innerText().catch(() => "")).replace(/\s+/g, " ").slice(0, 700);
    const calls = state.requests.slice(-4).map((r) => `${r.method} ${r.path}`).join(" | ");
    throw new Error(`${label}: text ${pattern} never appeared. Page: ${shown}. Last requests: ${calls}`, { cause: error });
  }
}
const setMode = (mode) => fetch(`http://127.0.0.1:${API_PORT}/__mode?m=${mode}`);

async function main() {
  await new Promise((resolve) => server.listen(API_PORT, "127.0.0.1", resolve));
  const browser = await chromium.launch({ executablePath: resolveChrome(), headless: true, args: ["--no-sandbox"] });
  const context = await browser.newContext({ viewport: { width: 1440, height: 900 } });
  const page = await context.newPage();
  const consoleErrors = [];
  const pageErrors = [];
  page.on("pageerror", (error) => pageErrors.push(String(error)));
  page.on("console", (message) => {
    if (message.type() !== "error") return;
    const text = message.text();
    // The wallet pill and stats read node endpoints this stand-in does not serve, and a mode that fails the store on purpose.
    if (/Failed to load resource|ERR_|status of (404|503)/.test(text)) return;
    consoleErrors.push(text);
  });

  console.log("Catalog (no wallet)");
  await page.goto(`${SITE}/games`, { waitUntil: "networkidle" });
  check("hero names the platform and its promise", (await page.locator("h1").innerText()).replace(/\s+/g, " ").includes("ULTRANET GAMES") && (await page.getByText("Play. Compete. Earn ULTRA.").count()) === 1);
  check("Explore Games is a prominent action", (await page.getByRole("link", { name: /explore games/i }).count()) === 1);
  await page.locator("#catalog [role=group] button").first().waitFor();
  const cards = page.locator("#catalog article");
  check("all three catalog games are listed", (await cards.count()) === 3);
  const text = await page.locator("main").innerText();
  check("cards show ids and AppChain ids", text.includes("#7") && text.includes("#1") && text.includes("#9") && text.includes("a1a1a1a1"));
  check("availability is explicit: Rewards Available, Record Only, Rewards Unavailable", ["rewards available", "record only", "rewards unavailable"].every((word) => text.toLowerCase().includes(word)));
  check("featured shows only the game with a reward on offer", (await page.locator("#featured-title").count()) === 1 && (await page.locator("section[aria-labelledby=featured-title] article").count()) === 1);
  check("no invented ratings, player counts or earnings", !/rating|players online|\bplayers\b.*\d|trending|top rated|earn(ed|ings) \d/i.test(text.replace("Play. Compete. Earn ULTRA.", "").replace(/per player per day/gi, "")), text.slice(0, 200));
  await page.screenshot({ path: `${SHOTS}/catalog-1440.png`, fullPage: true });
  await axe(page, "/games");

  await page.getByLabel("Search games by name or description").fill("chess");
  check("search by name narrows the list", (await cards.count()) === 1 && (await cards.first().innerText()).includes("Chess Archive"));
  await page.getByLabel("Search games by name or description").fill("playable now");
  check("search by description narrows the list", (await cards.count()) === 1 && (await cards.first().innerText()).includes("Echo Arena"));
  await page.getByLabel("Search games by name or description").fill("zzzz");
  check("a search with no match says so", (await page.getByText("No game matches").count()) === 1);
  await page.getByLabel("Search games by name or description").fill("");
  await page.getByRole("button", { name: "Record only" }).click();
  check("the Record only filter shows the record-only game", (await cards.count()) === 1 && (await cards.first().innerText()).toLowerCase().includes("record only"));
  await page.getByRole("button", { name: "Rewards available" }).click();
  check("the Rewards filter shows only the game with a reward on offer", (await cards.count()) === 1 && (await cards.first().innerText()).includes("Skyline Racer"));
  await page.getByRole("button", { name: "On my AppChains" }).click();
  check("the ownership filter needs a wallet and says so", (await cards.count()) === 0 && (await page.getByText(/needs a wallet on this device/).count()) === 1);
  await page.getByRole("button", { name: "All games" }).click();

  console.log("States");
  await setMode("empty");
  await page.reload({ waitUntil: "networkidle" });
  check("an empty catalog has an empty state with a way forward", (await page.getByText("No games registered yet").count()) === 1);
  await setMode("error");
  await page.reload({ waitUntil: "networkidle" });
  check("a failing service shows an error with the service's words and a retry", (await page.locator("main [role=alert]").innerText()).includes("busy") && (await page.getByRole("button", { name: "Try again" }).count()) === 1);
  await axe(page, "/games (error state)");
  await setMode("ok");
  await page.getByRole("button", { name: "Try again" }).click();
  await page.locator("#catalog article").first().waitFor();
  check("retrying recovers", (await page.locator("#catalog article").count()) === 3);

  console.log("Game page");
  await page.locator("#catalog article").filter({ hasText: "Skyline Racer" }).getByRole("link", { name: "Skyline Racer" }).click();
  await page.waitForURL(`**/games/${IDS.race}`);
  await page.getByText("Game ID").first().waitFor();
  const detail = await page.locator("main").innerText();
  check("the page shows id, AppChain, owner and validation from the service", detail.includes(IDS.race) && detail.includes("#7") && detail.includes(OTHER_OWNER) && detail.includes("platform-approved verifier"));
  const lower = detail.toLowerCase();
  check("rewards state the policy and the pool, from the service", lower.includes("version 1") && detail.includes("0.0001 ULTRA") && lower.includes("funded by the owner") && detail.includes("0.009899 ULTRA"), detail.slice(0, 600));
  check("Play now is offered, and no wallet is needed to read the page", (await page.getByRole("link", { name: "Play now" }).count()) === 1);
  check("without a wallet, play asks for one only at the action", (await page.getByText(/Create or restore an UltraNet wallet to play this game/).count()) === 1);
  await page.screenshot({ path: `${SHOTS}/detail-1440.png`, fullPage: true });
  await axe(page, "/games/[id]");

  await page.goto(`${SITE}/games/${IDS.archive}`, { waitUntil: "networkidle" });
  await page.getByText("Game ID").first().waitFor();
  const record = await page.locator("main").innerText();
  check("a record-only game is not playable and says why", record.toLowerCase().includes("not playable") && record.includes("no approved verifier") && (await page.getByRole("link", { name: "Play now" }).count()) === 0);
  await page.goto(`${SITE}/games/${"f".repeat(32)}`, { waitUntil: "networkidle" });
  check("an unknown game id says no such game", (await page.getByText("No such game").count()) === 1);
  const bad = await page.goto(`${SITE}/games/not-an-id`, { waitUntil: "networkidle" });
  check("a malformed id is a 404", bad.status() === 404);

  console.log("Wallet, sign-in and play");
  await page.goto(`${SITE}/send-ultra`, { waitUntil: "networkidle" });
  await page.getByRole("button", { name: "Restore wallet" }).first().click();
  await page.locator("#restore-recovery-phrase").fill(PHRASE);
  await page.locator("#wallet-password").fill(PASSWORD);
  await page.locator("#wallet-password-confirm").fill(PASSWORD);
  await page.locator("button[type=submit]", { hasText: "Restore wallet" }).click();
  await page.getByText(/Ready to sign locally|unlocked/i).first().waitFor({ timeout: 120_000 });
  check("a wallet was restored in the browser (a public test phrase, never a real wallet)", true);

  await clientNav(page, `/games/${IDS.race}`);
  await page.getByText("Developer input").waitFor();
  check("playing needs no AppChain ownership, and the raw hex field is tucked away as a developer fallback", !(await page.getByLabel("Raw verifier input (hex)").isVisible()));
  await page.getByText("Developer input").click();
  check("the raw input is a labelled developer fallback, and the integration point says so", (await page.getByText(/no play interface on this site yet/).count()) === 1);
  const bytesBefore = state.requests.length;
  await page.getByLabel("Raw verifier input (hex)").fill("zz");
  await page.getByRole("button", { name: "Play now" }).click();
  check("bad input is refused before anything is signed or sent", (await page.getByText("Use only 0 to 9 and a to f.").count()) === 1 && !state.requests.slice(bytesBefore).some((r) => r.path.includes("/auth/") || r.path.includes("/play/")));
  await page.getByLabel("Raw verifier input (hex)").fill("01");
  await page.getByRole("button", { name: "Play now" }).click();
  await page.getByTestId("play-result").waitFor({ timeout: 120_000 });
  const result = (await page.getByTestId("play-result").innerText()).toLowerCase();
  check("the result is reserved, not paid, and says it is not yet paid", result.includes("reward reserved — not yet paid") && !result.includes("paid — confirmed"), result);
  const sent = state.requests.filter((r) => r.path.startsWith("/api/games/play/"));
  check("play was sent with the wallet session in its own header and no recovery phrase", sent.length === 1 && sent[0].session === state.token && !JSON.stringify(state.requests).includes("abandon"));
  const verify = state.requests.find((r) => r.path === "/api/games/auth/verify");
  check("sign-in sent only a public key and a signature", verify && Object.keys(verify.body).sort().join() === "challenge_id,public_key,signature");
  await axe(page, "/games/[id] after play");
  await page.getByRole("button", { name: "Check payment status" }).click();
  await page.waitForTimeout(500);
  check("checking while the network has not confirmed still says not yet paid", (await page.getByTestId("play-result").innerText()).toLowerCase().includes("not yet paid"));
  await fetch(`http://127.0.0.1:${API_PORT}/__confirm`);
  await page.getByRole("button", { name: "Check payment status" }).click();
  await page.getByText(/paid — confirmed on ultranet/i).first().waitFor();
  check("only after the service reports confirmation does the page say Paid", true);

  const history = await page.locator("#results").innerText();
  check("your results in this game list a paid and a reserved reward, each with its own status", history.toLowerCase().includes("paid — confirmed on ultranet") && history.toLowerCase().includes("reward reserved — not yet paid"));
  await clientNav(page, "/games/rewards");
  await page.getByRole("heading", { name: /my rewards/i }).waitFor();
  await page.locator("main li").first().waitFor();
  check("My rewards shows every reward with its transaction", (await page.locator("main li").count()) === 2 && (await page.locator("main").innerText()).includes("Transaction abababababab"), (await page.locator("main").innerText()).replace(/\s+/g, " ").slice(0, 500));
  await axe(page, "/games/rewards");

  console.log("Owner");
  await clientNav(page, "/games/manage");
  await page.getByText(/AppChain #7 · Arcade/i).waitFor();
  const hub = await page.locator("main").innerText();
  check("only the chain whose L1 owner is this wallet is listed (another chain with the same name is not)", hub.toLowerCase().includes("appchain #7") && !hub.toLowerCase().includes("appchain #1 "));
  await axe(page, "/games/manage");
  await page.getByLabel(/Game name/).fill("Neon Drift");
  await page.getByLabel(/Description/).fill("A new game");
  await page.getByRole("button", { name: "Sign and register" }).click();
  await waitText(page, /in the shared catalog now/, "registration");
  const reg = state.requests.find((r) => r.path === "/api/games/catalog" && r.method === "POST");
  check("registration carries the chain id, the name and the owner's five proof fields", reg && reg.body.chain_id === 7 && reg.body.name === "Neon Drift" && ["nonce", "timestamp", "public_key", "signature"].every((k) => k in reg.body) && reg.body.signature.length === 4627 * 2);
  await clientNav(page, "/games");
  await page.locator("#catalog article").first().waitFor();
  check("the registered game is in the shared catalog the site reads", (await page.locator("#catalog article").filter({ hasText: "Neon Drift" }).count()) === 1);

  await clientNav(page, `/games/${IDS.race}/manage`);
  await page.getByText(/owner actions/i).first().waitFor();
  await page.getByLabel("Deposit transaction hash").fill("ab".repeat(32));
  await page.getByRole("button", { name: "Sign and claim deposit" }).click();
  await page.getByText(/Deposit claimed/).waitFor({ timeout: 120_000 });
  await page.getByLabel("Reward per result (ULTRA)").fill("0.0002");
  await page.getByRole("button", { name: "Sign and propose policy" }).click();
  await page.getByText(/pays nothing until the platform approves it/).first().waitFor({ timeout: 120_000 });
  await page.getByLabel("Amount (ULTRA)").fill("0.0004");
  await page.getByRole("button", { name: "Sign and withdraw" }).click();
  await page.getByText(/Withdrawal of 0.0004 ULTRA queued/).waitFor({ timeout: 120_000 });
  const proofKeys = (r) => ["nonce", "timestamp", "public_key", "signature"].every((k) => k in r.body);
  const fund = state.requests.find((r) => r.path.endsWith("/fund"));
  const policy = state.requests.find((r) => r.path.endsWith("/policy"));
  const withdraw = state.requests.find((r) => r.path.endsWith("/withdraw"));
  check("every owner action carries a signature", fund && policy && withdraw && proofKeys(fund) && proofKeys(policy) && proofKeys(withdraw));
  check("the amounts are exact base units", policy.body.reward_units === 200 && withdraw.body.amount_units === 400 && fund.body.tx_hash === "ab".repeat(32));
  await axe(page, "/games/[id]/manage");

  await clientNav(page, `/games/${IDS.echo}/manage`);
  await page.getByText("You are not this chain's owner").waitFor();
  check("a wallet that does not own the chain is offered no owner actions", (await page.getByText(/^owner actions$/i).count()) === 0);

  console.log("Navigation and layout");
  await page.goto(`${SITE}/games`, { waitUntil: "networkidle" });
  const navText = await page.locator("nav").allInnerTexts();
  check("Games is in the site's own navigation", navText.some((t) => /GAMES/.test(t)));
  for (const label of ["Home", "AppChains", "Wallet", "Explorer", "Developers"]) {
    check(`Games links out to ${label}`, (await page.getByRole("navigation", { name: "Elsewhere on UltraNet" }).getByRole("link", { name: new RegExp(label) }).count()) === 1);
  }
  for (const width of [1024, 1280, 1440, 1536]) {
    await page.setViewportSize({ width, height: 900 });
    await page.goto(`${SITE}/games`, { waitUntil: "networkidle" });
    const boxes = await page.evaluate(() => {
      const bar = document.querySelector("body > div nav, body nav");
      const brand = document.querySelector("nav a[href='/']")?.getBoundingClientRect();
      const links = Array.from(document.querySelectorAll("nav .xl\\:flex a")).map((a) => a.getBoundingClientRect());
      return { brandRight: brand ? brand.right : 0, firstLeft: links.length ? Math.min(...links.map((l) => l.left)) : 0, lastRight: links.length ? Math.max(...links.map((l) => l.right)) : 0, count: links.length, visible: !!bar, width: window.innerWidth };
    });
    if (width >= 1280) {
      check(`the site's top bar does not overlap or overflow at ${width}px`, boxes.count >= 10 && boxes.firstLeft >= boxes.brandRight - 1 && boxes.lastRight <= boxes.width, JSON.stringify(boxes));
    }
  }
  for (const [name, width, height] of [["desktop", 1440, 900], ["tablet", 820, 1100], ["mobile", 390, 844]]) {
    await page.setViewportSize({ width, height });
    await page.goto(`${SITE}/games`, { waitUntil: "networkidle" });
    await page.locator("#catalog article").first().waitFor();
    const overflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    check(`no horizontal overflow at ${name} (${width}px)`, overflow <= 1, `overflow ${overflow}px`);
    await page.screenshot({ path: `${SHOTS}/catalog-${name}.png`, fullPage: true });
    await page.goto(`${SITE}/games/${IDS.race}`, { waitUntil: "networkidle" });
    await page.getByText("Game ID").first().waitFor();
    const detailOverflow = await page.evaluate(() => document.documentElement.scrollWidth - window.innerWidth);
    check(`the game page has no horizontal overflow at ${name}`, detailOverflow <= 1, `overflow ${detailOverflow}px`);
    await page.screenshot({ path: `${SHOTS}/detail-${name}.png`, fullPage: true });
  }

  check("no uncaught page errors", pageErrors.length === 0, pageErrors.join(" | "));
  check("no console errors from the Games pages", consoleErrors.length === 0, consoleErrors.join(" | "));
  await browser.close();
  server.close();
  console.log(`\n${passed} checks passed, ${failures.length} failed`);
  if (failures.length) {
    console.log(failures.map((f) => ` - ${f}`).join("\n"));
    process.exit(1);
  }
}

main().catch((error) => {
  console.error(error);
  server.close();
  process.exit(2);
});
