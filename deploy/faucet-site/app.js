/* UltraNet faucet claim page.
 *
 * Logic adapted from the internal staging helper (staging-token.js), minus its staging-only
 * hostname guard and with the production Turnstile site key. The claim contract is unchanged:
 * POST /api/faucet/claims with Content-Type: application/json, an Idempotency-Key header and
 * { address, captcha_token }. The server answers 202 Accepted with a queued claim, and the
 * transaction is confirmed only when a block includes it, so acceptance is not payout. */
(() => {
  "use strict";

  // Production Turnstile widget: hostname faucet.ultranetwork.cc, action faucet_claim.
  const TURNSTILE_SITE_KEY = "0x4AAAAAAEiUXnIE63ifzTHN";
  const TURNSTILE_ACTION = "faucet_claim";
  const CLAIM_ENDPOINT = "/api/faucet/claims";
  const DEVICE_TIMEOUT_MS = 20000;
  const POLL_INTERVAL_MS = 5000;
  const POLL_LIMIT = 36;
  const ADDRESS_PATTERN = /^[0-9a-f]{64}$/;
  const CLAIM_ID_PATTERN = /^[0-9a-f]{32}$/;
  const HEX_DIGITS = /^[0-9a-fA-F]$/;

  const form = document.getElementById("claim-form");
  const recipientInput = document.getElementById("recipient");
  const rule = document.getElementById("address-rule");
  const turnstileContainer = document.getElementById("turnstile-widget");
  const submitButton = document.getElementById("submit-button");
  const submitLabel = document.getElementById("submit-label");
  const statusRow = document.getElementById("status");
  const statusText = document.getElementById("status-text");
  const receipt = document.getElementById("receipt");
  const receiptId = document.getElementById("receipt-id");
  const receiptState = document.getElementById("receipt-state");
  const pendingRule = receipt.querySelector(".pending-rule");

  let widgetId = null;
  let token = null;
  let phase = "idle"; // idle | submitting | accepted | stopped
  let pollTimer = null;
  let pollCount = 0;

  for (let index = 0; index < 64; index += 1) {
    rule.appendChild(document.createElement("i"));
  }
  const cells = Array.from(rule.children);

  function setStatus(message, tone = "neutral") {
    statusText.textContent = message;
    statusRow.dataset.tone = tone;
  }

  function normalizeAddress(value) {
    return value.trim().replace(/^0[xX]/, "").toLowerCase();
  }

  function paintRule(value) {
    for (let index = 0; index < cells.length; index += 1) {
      const character = value[index];
      const cell = cells[index];
      cell.className = character ? (HEX_DIGITS.test(character) ? "on" : "bad") : "";
    }
  }

  function updateControls() {
    const address = normalizeAddress(recipientInput.value);
    const addressReady = ADDRESS_PATTERN.test(address);
    const ready = phase === "idle" && addressReady && Boolean(token);
    submitButton.disabled = phase !== "idle" || !ready;
    submitButton.dataset.ready = ready ? "true" : "false";
    recipientInput.disabled = phase !== "idle";
    if (phase === "submitting") {
      submitLabel.textContent = "Submitting…";
    } else if (phase === "accepted") {
      submitLabel.textContent = "Claim submitted";
    } else {
      submitLabel.textContent = "Claim 1 $ULTRA";
    }
  }

  function lockForm() {
    phase = "stopped";
    updateControls();
  }

  function clearToken(resetWidget) {
    token = null;
    if (resetWidget && widgetId !== null && window.turnstile) {
      try {
        window.turnstile.reset(widgetId);
      } catch {
        /* the widget will be re-rendered on the next attempt */
      }
    }
    updateControls();
  }

  function humanizeSeconds(totalSeconds) {
    const seconds = Math.max(0, Math.floor(totalSeconds));
    if (seconds >= 3600) {
      const hours = Math.floor(seconds / 3600);
      const minutes = Math.round((seconds % 3600) / 60);
      return minutes > 0 ? `${hours} h ${minutes} min` : `${hours} h`;
    }
    if (seconds >= 60) return `${Math.round(seconds / 60)} min`;
    return `${seconds} s`;
  }

  function retryAfterSeconds(response) {
    const header = response.headers.get("Retry-After");
    if (!header) return null;
    const value = Number.parseInt(header, 10);
    return Number.isFinite(value) && value >= 0 ? value : null;
  }

  async function describeFailure(response) {
    const wait = retryAfterSeconds(response);
    switch (response.status) {
      case 409:
        return wait
          ? { message: `This address already claimed. The next claim is available in ${humanizeSeconds(wait)}.`, tone: "caution" }
          : { message: "This request was already used. Reload the page to start a new one.", tone: "caution" };
      case 422:
        return { message: "Verification failed. Complete the check again and resubmit.", tone: "error", resetVerification: true };
      case 503:
        // The API returns one message for both "paused" and "out of funds"; do not guess.
        return { message: "The faucet cannot pay out right now — it may be paused or out of funds.", tone: "caution" };
      case 429:
        return wait
          ? { message: `Too many claims from this network. Try again in ${humanizeSeconds(wait)}.`, tone: "caution" }
          : { message: "Too many claims from this network. Try again later.", tone: "caution" };
      case 400:
        return { message: "The claim was rejected as invalid. Check the address and try again.", tone: "error" };
      case 403:
        return { message: "The claim was refused. If this keeps happening, the faucet proxy is misconfigured.", tone: "error" };
      default:
        return { message: "The claim failed. Nothing was paid; try again.", tone: "error" };
    }
  }

  function renderTurnstile() {
    if (!window.turnstile || widgetId !== null || phase !== "idle") return;
    try {
      widgetId = window.turnstile.render(turnstileContainer, {
        sitekey: TURNSTILE_SITE_KEY,
        action: TURNSTILE_ACTION,
        "response-field": false,
        callback(value) {
          token = typeof value === "string" && value.length > 0 ? value : null;
          setStatus(token ? "Verification complete. Claim when ready." : "Verification failed. Try again.", token ? "success" : "error");
          updateControls();
        },
        "expired-callback"() {
          clearToken(false);
          setStatus("Verification expired. Complete the check again.", "caution");
        },
        "error-callback"() {
          clearToken(false);
          setStatus("The verification widget could not load. Check your connection and reload.", "error");
        },
      });
    } catch {
      clearToken(false);
      setStatus("The verification widget could not start. Reload the page.", "error");
    }
  }

  function waitForTurnstile(attempt = 0) {
    if (phase !== "idle") return;
    if (window.turnstile) {
      renderTurnstile();
      return;
    }
    if (attempt >= 50) {
      setStatus("The verification widget did not load. Reload the page before claiming.", "error");
      return;
    }
    window.setTimeout(() => waitForTurnstile(attempt + 1), 200);
  }

  function makeIdempotencyKey() {
    if (window.crypto && typeof window.crypto.randomUUID === "function") {
      return window.crypto.randomUUID();
    }
    if (window.crypto && typeof window.crypto.getRandomValues === "function") {
      const bytes = new Uint8Array(32);
      window.crypto.getRandomValues(bytes);
      return Array.from(bytes, (byte) => byte.toString(16).padStart(2, "0")).join("");
    }
    throw new Error("Secure randomness is unavailable.");
  }

  function showReceipt(claim) {
    phase = "accepted";
    form.hidden = true;
    receipt.hidden = false;
    receiptId.textContent = claim.claim_id;
    receiptState.textContent = "Waiting for a block to confirm it.";
    pendingRule.className = "pending-rule";
    setStatus("Claim accepted. Payout follows once a block includes it.", "success");
    updateControls();
    receipt.scrollIntoView({ block: "nearest" });
    pollClaim(claim.claim_id);
  }

  function settleReceipt(text, tone, ruleClass) {
    receiptState.textContent = text;
    pendingRule.className = `pending-rule ${ruleClass}`;
    setStatus(text, tone);
  }

  async function pollClaim(claimId) {
    if (pollCount >= POLL_LIMIT) {
      settleReceipt(
        "Still not confirmed. Your claim stays valid — check the address balance before claiming again.",
        "caution",
        "",
      );
      return;
    }
    pollCount += 1;
    pollTimer = window.setTimeout(async () => {
      try {
        const response = await fetch(`${CLAIM_ENDPOINT}/${claimId}`, {
          method: "GET",
          credentials: "same-origin",
          cache: "no-store",
          headers: { Accept: "application/json" },
        });
        if (response.ok) {
          const payload = await response.json().catch(() => null);
          const state = payload && payload.data && payload.data.status;
          if (state === "confirmed") {
            settleReceipt("Confirmed on chain. The 1 $ULTRA is in your wallet.", "success", "done");
            return;
          }
          if (state === "failed") {
            settleReceipt("The faucet could not complete this payout. Nothing was sent.", "error", "stopped");
            return;
          }
        }
      } catch {
        /* transient: keep waiting */
      }
      pollClaim(claimId);
    }, POLL_INTERVAL_MS);
  }

  async function submitClaim(event) {
    event.preventDefault();
    if (phase !== "idle") return;

    const address = normalizeAddress(recipientInput.value);
    if (!ADDRESS_PATTERN.test(address)) {
      recipientInput.setAttribute("aria-invalid", "true");
      setStatus("That is not a 64-character hexadecimal address.", "error");
      recipientInput.focus();
      return;
    }
    if (!token) {
      setStatus("Complete the verification check first.", "caution");
      return;
    }

    phase = "submitting";
    const submittedToken = token;
    token = null;
    updateControls();
    setStatus("Submitting the claim…");

    const controller = new AbortController();
    const timeout = window.setTimeout(() => controller.abort(), DEVICE_TIMEOUT_MS);
    try {
      const response = await fetch(CLAIM_ENDPOINT, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": makeIdempotencyKey(),
        },
        body: JSON.stringify({ address, captcha_token: submittedToken }),
        credentials: "same-origin",
        cache: "no-store",
        redirect: "error",
        signal: controller.signal,
      });

      if (response.status !== 202) {
        const failure = await describeFailure(response);
        phase = "idle";
        if (failure.resetVerification) clearToken(true);
        updateControls();
        setStatus(failure.message, failure.tone);
        return;
      }

      const payload = await response.json().catch(() => null);
      const claim = payload && payload.success === true ? payload.data : null;
      const valid =
        claim &&
        typeof claim.claim_id === "string" &&
        CLAIM_ID_PATTERN.test(claim.claim_id) &&
        normalizeAddress(String(claim.address || "")) === address &&
        typeof claim.amount_ultra === "string";

      if (!valid) {
        lockForm();
        setStatus("The claim was sent, but the reply could not be read. Check the address balance before trying again.", "caution");
        return;
      }
      if (claim.status === "confirmed") {
        form.hidden = true;
        receipt.hidden = false;
        receiptId.textContent = claim.claim_id;
        settleReceipt("Confirmed on chain. The 1 $ULTRA is in your wallet.", "success", "done");
        phase = "accepted";
        updateControls();
        return;
      }
      showReceipt(claim);
    } catch {
      lockForm();
      setStatus(
        "No answer from the faucet. Submit was not confirmed — wait a minute and check the address balance before claiming again.",
        "caution",
      );
    } finally {
      window.clearTimeout(timeout);
      if (phase !== "accepted" && phase !== "stopped") phase = "idle";
      updateControls();
    }
  }

  recipientInput.addEventListener("input", () => {
    const value = normalizeAddress(recipientInput.value);
    paintRule(value);
    const valid = ADDRESS_PATTERN.test(value);
    recipientInput.setAttribute("aria-invalid", value.length === 0 || valid ? "false" : "true");
    if (phase === "idle") {
      setStatus(valid ? (token ? "Ready to claim." : "Address accepted. Complete the verification check.") : "Paste an address to begin.");
    }
    updateControls();
  });

  form.addEventListener("submit", submitClaim);
  window.addEventListener("load", () => waitForTurnstile());
  window.addEventListener("beforeunload", () => {
    token = null;
    if (pollTimer) window.clearTimeout(pollTimer);
  });

  paintRule("");
  updateControls();
})();
