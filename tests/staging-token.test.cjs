"use strict";

const test = require("node:test");
const assert = require("node:assert/strict");
const fs = require("node:fs");
const path = require("node:path");
const vm = require("node:vm");

const source = fs.readFileSync(path.join(__dirname, "../staging-token.js"), "utf8");
const html = fs.readFileSync(path.join(__dirname, "../staging-token.html"), "utf8");
const HOST = "faucet-staging.ultranetwork.cc";
const ADDRESS = "a".repeat(64);
const CLAIM_ID = "b".repeat(32);
const TOKEN = "synthetic-captcha-not-a-live-token";
const KEY = "00000000-0000-4000-8000-000000000001";
const PRIVATE_MARKER = "synthetic-private-error-must-not-be-rendered";
const DEADLINE = 15000;

function deferred() {
  let resolve;
  let reject;
  const promise = new Promise((yes, no) => { resolve = yes; reject = no; });
  return { promise, resolve, reject };
}

async function flush() {
  // Drain the small fetch -> body -> finalization chain without wall-clock timers.
  for (let index = 0; index < 12; index += 1) await Promise.resolve();
}

class Element {
  constructor() {
    this.listeners = new Map();
    this.attributes = new Map();
    this.dataset = {};
    this.textContent = "";
    this.value = "";
    this.defaultValue = "";
    this.disabled = false;
  }

  addEventListener(type, listener) {
    const listeners = this.listeners.get(type) || [];
    listeners.push(listener);
    this.listeners.set(type, listeners);
  }

  setAttribute(name, value) { this.attributes.set(name, value); }
  getAttribute(name) { return this.attributes.get(name); }

  dispatch(type) {
    const event = {
      defaultPrevented: false,
      preventDefault() { this.defaultPrevented = true; },
    };
    const results = (this.listeners.get(type) || []).map((listener) => listener(event));
    return { event, results };
  }
}

function response(options = {}) {
  const payload = options.payload === undefined ? {
    success: true,
    message: PRIVATE_MARKER, // Server prose must never be displayed.
    data: {
      claim_id: CLAIM_ID,
      address: ADDRESS,
      status: "queued",
      amount_base_units: 1000000,
      amount_ultra: "1.000000",
      decimals: 6,
      retry_after_seconds: 86400,
      ...options.claim,
    },
  } : options.payload;
  const status = options.status === undefined ? 202 : options.status;
  let reads = 0;
  return {
    status,
    ok: status >= 200 && status < 300,
    headers: {
      get(name) {
        assert.equal(name.toLowerCase(), "content-type");
        return options.contentType === undefined ? "application/json; charset=utf-8" : options.contentType;
      },
    },
    json() {
      reads += 1;
      return options.json ? options.json() : Promise.resolve(payload);
    },
    get reads() { return reads; },
  };
}

function harness(options = {}) {
  const elements = Object.fromEntries([
    "claim-form", "recipient", "turnstile-widget", "submit-button", "reset-button", "status",
  ].map((id) => [id, new Element()]));
  const window = new Element();
  const timers = new Map();
  const requests = [];
  const forbiddenUses = [];
  let clock = 0;
  let nextTimer = 0;
  let widgetOptions;
  let renderCount = 0;
  let resetCount = 0;
  let cryptoCalls = 0;
  const forbidden = (name) => () => {
    forbiddenUses.push(name);
    throw new Error("Forbidden offline capability used");
  };
  const deniedObject = (name) => new Proxy({}, { get: forbidden(name), set: forbidden(name) });
  window.location = new URL(options.url || `https://${HOST}/staging-token.html`);
  window.setTimeout = (callback, delay) => {
    const id = ++nextTimer;
    timers.set(id, { callback, at: clock + delay });
    return id;
  };
  window.clearTimeout = (id) => timers.delete(id);
  window.crypto = Object.hasOwn(options, "crypto") ? options.crypto : {
    randomUUID() { cryptoCalls += 1; return KEY; },
  };
  window.AbortController = Object.hasOwn(options, "AbortController") ? options.AbortController : AbortController;
  window.localStorage = deniedObject("localStorage");
  window.sessionStorage = deniedObject("sessionStorage");
  window.console = deniedObject("console");
  window.turnstile = options.noWidget ? undefined : {
    render(container, config) {
      assert.equal(container, elements["turnstile-widget"]);
      renderCount += 1;
      widgetOptions = config;
      if (options.renderThrows) throw new Error(PRIVATE_MARKER);
      return "synthetic-widget";
    },
    reset() {
      resetCount += 1;
      if (options.resetCallback) widgetOptions.callback(TOKEN);
      if (options.resetThrows) throw new Error(PRIVATE_MARKER);
    },
  };
  const fetch = (endpoint, init) => {
    requests.push({ endpoint, init });
    if (!options.fetch) throw new Error("Unexpected mocked request");
    return options.fetch(endpoint, init);
  };
  window.fetch = fetch;
  window.XMLHttpRequest = forbidden("XMLHttpRequest");
  window.WebSocket = forbidden("WebSocket");
  window.navigator = { sendBeacon: forbidden("sendBeacon") };
  const document = { getElementById: (id) => elements[id] };
  Object.defineProperty(document, "cookie", { get: forbidden("cookie"), set: forbidden("cookie") });

  const form = elements["claim-form"];
  const recipient = elements.recipient;
  // Native reset is cancelable and applies default values AFTER reset listeners.
  form.reset = () => {
    const { event } = form.dispatch("reset");
    if (!event.defaultPrevented) recipient.value = recipient.defaultValue;
    return event;
  };
  elements["reset-button"].click = () => {
    if (elements["reset-button"].disabled) return;
    const { event } = elements["reset-button"].dispatch("click");
    if (!event.defaultPrevented) form.reset();
  };

  const context = vm.createContext({
    window, document, fetch,
    console: window.console,
    localStorage: window.localStorage,
    sessionStorage: window.sessionStorage,
    XMLHttpRequest: window.XMLHttpRequest,
    WebSocket: window.WebSocket,
    navigator: window.navigator,
  });
  vm.runInContext(source, context, { filename: "staging-token.js", timeout: 1000 });

  return {
    elements, window, form, recipient, requests, timers, forbiddenUses,
    get widget() { return widgetOptions; },
    get renderCount() { return renderCount; },
    get resetCount() { return resetCount; },
    get cryptoCalls() { return cryptoCalls; },
    get status() { return elements.status.textContent; },
    load() { window.dispatch("load"); },
    input(value = ADDRESS) { recipient.value = value; recipient.dispatch("input"); },
    ready() { this.load(); this.input(); widgetOptions.callback(TOKEN); },
    submit() { return form.dispatch("submit"); },
    advance(milliseconds) {
      const end = clock + milliseconds;
      let executions = 0;
      while (true) {
        const next = [...timers].sort((left, right) => left[1].at - right[1].at)
          .find(([, timer]) => timer.at <= end);
        if (!next) break;
        assert.ok(++executions < 1000, "Timer fixture runaway");
        const [id, timer] = next;
        clock = timer.at;
        timers.delete(id);
        timer.callback();
      }
      clock = end;
    },
  };
}

function assertPrivate(h) {
  const rendered = Object.values(h.elements).map((element) => [
    element.textContent, element.value, ...element.attributes.values(), ...Object.values(element.dataset),
  ].join(" ")).join(" ") + h.window.location.href;
  assert.ok(![TOKEN, KEY, PRIVATE_MARKER].some((value) => rendered.includes(value)), "Private fixture leaked");
  assert.equal(h.forbiddenUses.length, 0, "Logging, storage or unexpected network capability used");
}

function assertLocked(h, busy = false) {
  assert.equal(h.elements["submit-button"].disabled, true);
  assert.equal(h.elements["reset-button"].disabled, true);
  assert.equal(h.recipient.disabled, true);
  assert.equal(h.form.getAttribute("aria-busy"), busy ? "true" : "false");
  assert.equal(h.requests.length, 1);
  assertPrivate(h);
}

async function attemptReopen(h, busy = false) {
  const previousStatus = h.status;
  const previousAddress = h.recipient.value;
  h.elements["reset-button"].click();
  assert.equal(h.form.reset().defaultPrevented, true);
  assert.equal(h.recipient.value, previousAddress);
  h.widget.callback(TOKEN);
  h.widget["expired-callback"]();
  h.widget["error-callback"]();
  h.input("c".repeat(64)); // Programmatic mutation cannot bypass disabled controls.
  h.submit();
  h.submit();
  await flush();
  assert.ok(h.status === previousStatus, "A late callback/reset replaced the request status");
  assertLocked(h, busy);
}

test("existing HTML preserves native controls, labels and live status", () => {
  assert.match(html, /<label for="recipient">/);
  assert.match(html, /aria-describedby="recipient-help"/);
  assert.match(html, /id="reset-button"[^>]*type="reset"/);
  assert.match(html, /id="status"[^>]*role="status"[^>]*aria-live="polite"/);
  assert.match(html, /do not retry or reload to resubmit/);
});

test("load, input, verification, expiry, widget error and pre-attempt Reset never post", () => {
  const h = harness();
  assert.equal(h.elements["submit-button"].disabled, true);
  h.load();
  assert.equal(h.widget.sitekey, "0x4AAAAAAEqF5I0TO-lamwb7");
  assert.equal(h.widget.action, "faucet_claim");
  assert.equal(h.widget["response-field"], false, "Turnstile must not create a hidden token input");
  h.input();
  assert.equal(h.elements["submit-button"].disabled, true);
  h.widget.callback(TOKEN);
  assert.equal(h.elements["submit-button"].disabled, false);
  h.widget["expired-callback"]();
  assert.equal(h.elements["submit-button"].disabled, true);
  h.widget.callback(TOKEN);
  h.widget["error-callback"]();
  assert.equal(h.elements["submit-button"].disabled, true);
  h.widget.callback(TOKEN);
  h.elements["reset-button"].click();
  assert.equal(h.recipient.value, "");
  assert.equal(h.recipient.getAttribute("aria-invalid"), "false");
  assert.equal(h.elements["submit-button"].disabled, true);
  assert.equal(h.resetCount, 1);
  h.input();
  h.submit(); // Old token was cleared, even after the address becomes valid again.
  assert.equal(h.requests.length, 0);
  assertPrivate(h);
});

for (const address of ["", "a".repeat(63), "a".repeat(65), "A".repeat(64), "g".repeat(64)]) {
  test(`invalid recipient length ${address.length} and shape ${address[0] || "empty"} never posts`, () => {
    const h = harness();
    h.ready();
    h.input(address);
    h.submit();
    assert.equal(h.recipient.getAttribute("aria-invalid"), "true");
    assert.equal(h.requests.length, 0);
    assertPrivate(h);
  });
}

for (const token of [null, undefined, "", 42, {}]) {
  test(`invalid token type ${typeof token} length ${typeof token === "string" ? token.length : "n/a"} never posts`, () => {
    const h = harness();
    h.ready();
    h.widget.callback(token);
    h.submit();
    assert.equal(h.elements["submit-button"].disabled, true);
    assert.equal(h.requests.length, 0);
    assertPrivate(h);
  });
}

for (const url of [`http://${HOST}`, "https://example.invalid", `https://${HOST}:8443`]) {
  test(`wrong origin ${url} never renders or posts`, () => {
    const h = harness({ url });
    h.load();
    h.input();
    h.submit();
    assert.equal(h.renderCount, 0);
    assert.equal(h.requests.length, 0);
    assert.equal(h.elements["submit-button"].disabled, true);
    assertPrivate(h);
  });

  test(`submit rechecks changed origin ${url} even with an earlier token`, () => {
    const h = harness();
    h.ready();
    h.window.location = new URL(url);
    h.widget.callback(TOKEN);
    h.submit();
    assert.equal(h.requests.length, 0);
    assert.equal(h.elements["submit-button"].disabled, true);
    assertPrivate(h);
  });
}

for (const [name, capabilities] of [
  ["no crypto", { crypto: undefined }],
  ["no secure randomness", { crypto: {} }],
  ["throwing crypto", { crypto: { randomUUID() { throw new Error(PRIVATE_MARKER); } } }],
  ["no abort controller", { AbortController: undefined }],
]) {
  test(`${name} fails closed without dispatch or raw errors`, () => {
    const h = harness(capabilities);
    h.ready();
    h.submit();
    assert.equal(h.requests.length, 0);
    assert.equal(h.elements["submit-button"].disabled, true);
    assert.match(h.status, /No request was sent/);
    assertPrivate(h);
  });
}

test("valid response preserves exact request contract and acknowledges admission only", async () => {
  const h = harness({ fetch: () => Promise.resolve(response()) });
  h.ready();
  h.input(`  ${ADDRESS}  `);
  assert.equal(h.submit().event.defaultPrevented, true);
  assertLocked(h, true);
  h.submit();
  await flush();
  assertLocked(h);
  assert.match(h.status, /Claim accepted:/);
  assert.match(h.status, /Acceptance is not payout confirmation/);
  assert.doesNotMatch(h.status, /Disable intake/);
  const { endpoint, init } = h.requests[0];
  assert.equal(endpoint, "/api/faucet/claims");
  assert.equal(init.method, "POST");
  assert.equal(init.credentials, "same-origin");
  assert.equal(init.mode, "same-origin");
  assert.equal(init.cache, "no-store");
  assert.equal(init.redirect, "error");
  assert.equal(init.headers["Content-Type"], "application/json");
  assert.ok(init.headers["Idempotency-Key"] === KEY, "Unexpected idempotency fixture");
  assert.deepEqual(Object.keys(init.headers).sort(), ["Content-Type", "Idempotency-Key"]);
  const body = JSON.parse(init.body);
  assert.deepEqual(Object.keys(body).sort(), ["address", "captcha_token"]);
  assert.ok(body.address === ADDRESS && body.captcha_token === TOKEN, "Unexpected request body");
  assert.equal(init.signal.aborted, false);
  assert.equal(h.cryptoCalls, 1);
  assert.equal(h.timers.size, 0);
  assert.equal(h.resetCount, 0, "No post-dispatch widget reset is needed");
  await attemptReopen(h);
});

test("secure random bytes fallback and canonical HTTPS port work", async () => {
  let calls = 0;
  const h = harness({
    url: `https://${HOST}:443/staging-token.html`,
    crypto: { getRandomValues(bytes) { calls += 1; bytes.fill(7); return bytes; } },
    fetch: () => Promise.resolve(response({ contentType: "Application/JSON ; charset=utf-8" })),
  });
  h.ready();
  h.submit();
  await flush();
  assert.equal(calls, 1);
  assert.ok(/^[0-9a-f]{64}$/.test(h.requests[0].init.headers["Idempotency-Key"]), "Fallback key shape invalid");
  assert.ok(!h.status.includes(h.requests[0].init.headers["Idempotency-Key"]), "Fallback key leaked");
  assertLocked(h);
  assert.match(h.status, /Claim accepted:/);
});

test("pending fetch survives double submit, native reset, callbacks and unload without unlocking", async () => {
  const pending = deferred();
  const h = harness({ fetch: () => pending.promise });
  h.ready();
  h.submit();
  await attemptReopen(h, true);
  h.window.dispatch("beforeunload");
  assertLocked(h, true);
  pending.resolve(response());
  await flush();
  assert.match(h.status, /Claim accepted:/);
  assertLocked(h);
});

for (const status of [200, 201, 204, 302, 307, 308, 400, 403, 409, 422, 429, 500, 503]) {
  test(`HTTP ${status} permanently locks without reading/displaying response body`, async () => {
    const result = response({ status });
    const h = harness({ fetch: () => Promise.resolve(result) });
    h.ready();
    h.submit();
    await flush();
    assert.equal(result.reads, 0);
    assert.match(h.status, /Do not retry/);
    assert.equal(h.timers.size, 0);
    await attemptReopen(h);
  });
}

const invalidResponses = [
  ["HTML", { contentType: "text/html" }],
  ["missing media type", { contentType: null }],
  ["malformed JSON", { json: () => Promise.reject(new Error(PRIVATE_MARKER)) }],
  ["null payload", { payload: null }],
  ["array payload", { payload: [] }],
  ["success false", { payload: { success: false } }],
  ["no data", { payload: { success: true } }],
  ["array data", { payload: { success: true, data: [] } }],
  ["missing ID", { claim: { claim_id: undefined } }],
  ["uppercase ID", { claim: { claim_id: "B".repeat(32) } }],
  ["short ID", { claim: { claim_id: "b".repeat(31) } }],
  ["non-string ID", { claim: { claim_id: 123 } }],
  ["wrong address", { claim: { address: "c".repeat(64) } }],
  ["wrong amount", { claim: { amount_base_units: 1 } }],
  ["string amount", { claim: { amount_base_units: "1000000" } }],
  ["wrong display", { claim: { amount_ultra: "1" } }],
  ["wrong decimals", { claim: { decimals: 18 } }],
  ["existing pending claim", { claim: { status: "pending" } }],
  ["existing confirmed claim", { claim: { status: "confirmed" } }],
  ["negative cooldown", { claim: { retry_after_seconds: -1 } }],
  ["fractional cooldown", { claim: { retry_after_seconds: 1.5 } }],
  ["unsafe cooldown", { claim: { retry_after_seconds: Number.MAX_SAFE_INTEGER + 1 } }],
  ["missing cooldown", { claim: { retry_after_seconds: undefined } }],
];
for (const [name, options] of invalidResponses) {
  test(`${name} stops with a safe contract error and no resubmission`, async () => {
    const h = harness({ fetch: () => Promise.resolve(response(options)) });
    h.ready();
    h.submit();
    await flush();
    assert.match(h.status, /expected contract/);
    assert.match(h.status, /Do not retry/);
    assert.equal(h.timers.size, 0);
    await attemptReopen(h);
  });
}

for (const [name, fetch] of [
  ["network rejection", () => Promise.reject(new Error(PRIVATE_MARKER))],
  ["redirect rejection", () => Promise.reject(new TypeError(PRIVATE_MARKER))],
  ["synchronous fetch failure", () => { throw new Error(PRIVATE_MARKER); }],
]) {
  test(`${name} stays locked with an uncertain outcome`, async () => {
    const h = harness({ fetch });
    h.ready();
    h.submit();
    await flush();
    assert.match(h.status, /outcome is uncertain/);
    assert.equal(h.timers.size, 0);
    await attemptReopen(h);
  });
}

for (const stage of ["fetch", "body"]) {
  for (const settlement of ["resolve", "reject"]) {
    test(`${stage} timeout aborts and stays authoritative after late ${settlement}`, async () => {
      const pending = deferred();
      const lateResponse = response();
      const h = harness({ fetch: () => stage === "fetch"
        ? pending.promise
        : Promise.resolve(response({ json: () => pending.promise })) });
      h.ready();
      h.submit();
      await flush();
      await attemptReopen(h, true);
      h.advance(DEADLINE - 1);
      assertLocked(h, true);
      h.advance(1);
      assertLocked(h);
      assert.equal(h.requests[0].init.signal.aborted, true);
      assert.match(h.status, /outcome is uncertain/);
      const terminalStatus = h.status;
      await attemptReopen(h);
      if (settlement === "reject") pending.reject(new Error(PRIVATE_MARKER));
      else pending.resolve(stage === "fetch" ? lateResponse : await lateResponse.json());
      await flush();
      assert.ok(h.status === terminalStatus, "Late settlement replaced timeout status");
      assert.equal(h.timers.size, 0);
      if (stage === "fetch") assert.equal(lateResponse.reads, 0);
      await attemptReopen(h);
    });
  }
}

test("native abort rejection does not replace timeout guidance", async () => {
  const h = harness({ fetch: (_endpoint, init) => new Promise((_resolve, reject) => {
    init.signal.addEventListener("abort", () => reject(new Error(PRIVATE_MARKER)), { once: true });
  }) });
  h.ready();
  h.submit();
  h.advance(DEADLINE);
  await flush();
  assert.match(h.status, /outcome is uncertain/);
  await attemptReopen(h);
});

for (const resetThrows of [false, true]) {
  test(`pre-attempt native reset clears token even with synchronous callback; throws=${resetThrows}`, () => {
    const h = harness({ resetCallback: true, resetThrows });
    h.ready();
    assert.equal(h.form.reset().defaultPrevented, false);
    assert.equal(h.recipient.value, "");
    h.input();
    h.submit();
    assert.equal(h.requests.length, 0);
    assert.equal(h.elements["submit-button"].disabled, true);
    assertPrivate(h);
  });
}

test("widget reset failure cannot affect a completed attempt", async () => {
  const h = harness({ resetThrows: true, fetch: () => Promise.resolve(response()) });
  h.ready();
  h.submit();
  await flush();
  await attemptReopen(h);
  assert.equal(h.resetCount, 0);
});

test("widget render exception and unavailable widget fail safely without network", () => {
  const failed = harness({ renderThrows: true });
  failed.load();
  failed.input();
  failed.submit();
  assert.equal(failed.requests.length, 0);
  assertPrivate(failed);
  const absent = harness({ noWidget: true });
  absent.load();
  absent.advance(10000);
  assert.match(absent.status, /Turnstile did not load/);
  assert.equal(absent.timers.size, 0);
  assert.equal(absent.requests.length, 0);
  assertPrivate(absent);
});
