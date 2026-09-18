(() => {
  "use strict";

  const APPROVED_HOSTNAME = "faucet-staging.ultranetwork.cc";
  const TURNSTILE_SITE_KEY = "0x4AAAAAAEqF5I0TO-lamwb7";
  const TURNSTILE_ACTION = "faucet_claim";
  const CLAIM_ENDPOINT = "/api/faucet/claims";
  const CLAIM_REQUEST_TIMEOUT_MS = 15000;
  const ADDRESS_PATTERN = /^[0-9a-f]{64}$/;
  const CLAIM_ID_PATTERN = /^[0-9a-f]{32}$/;
  const STOP_INSTRUCTION = "Do not retry or reload to resubmit. Follow the private operator stop and reconciliation procedure.";
  const UNCERTAIN_MESSAGE = `Request outcome is uncertain. ${STOP_INSTRUCTION}`;
  const CONTRACT_MESSAGE = `The claim response did not match the expected contract. ${STOP_INSTRUCTION}`;

  const form = document.getElementById("claim-form");
  const recipientInput = document.getElementById("recipient");
  const turnstileContainer = document.getElementById("turnstile-widget");
  const submitButton = document.getElementById("submit-button");
  const resetButton = document.getElementById("reset-button");
  const status = document.getElementById("status");

  let turnstileWidgetId = null;
  let verificationToken = null;
  let resettingVerification = false;
  // Monotonic for this loaded page only; not durable/server-side enforcement.
  let attemptStarted = false;
  let phase = "idle";

  function isApprovedLocation() {
    return window.location.protocol === "https:"
      && window.location.hostname === APPROVED_HOSTNAME
      && (window.location.port === "" || window.location.port === "443");
  }

  function setStatus(message, tone = "") {
    status.textContent = message;
    if (tone) {
      status.dataset.tone = tone;
    } else {
      delete status.dataset.tone;
    }
  }

  function updateControls() {
    const addressIsValid = ADDRESS_PATTERN.test(recipientInput.value.trim());
    submitButton.disabled = attemptStarted || phase !== "idle" || !isApprovedLocation()
      || !addressIsValid || !verificationToken;
    recipientInput.disabled = attemptStarted;
    resetButton.disabled = attemptStarted;
    form.setAttribute("aria-busy", phase === "submitting" ? "true" : "false");
  }

  function clearVerification(resetWidget = true) {
    verificationToken = null;
    resettingVerification = true;
    let resetSucceeded = true;
    try {
      if (resetWidget && turnstileWidgetId !== null && window.turnstile) {
        window.turnstile.reset(turnstileWidgetId);
      }
    } catch {
      resetSucceeded = false;
    } finally {
      verificationToken = null;
      resettingVerification = false;
      updateControls();
    }
    return resetSucceeded;
  }

  function validateRecipient() {
    const value = recipientInput.value.trim();
    const valid = ADDRESS_PATTERN.test(value);
    recipientInput.setAttribute("aria-invalid", valid ? "false" : "true");
    if (!valid) {
      setStatus("Enter a valid 64-character lowercase hexadecimal recipient address.", "error");
      return null;
    }
    return value;
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

  function safeFailureMessage(responseStatus) {
    let reason;
    switch (responseStatus) {
      case 400:
        reason = "The claim request was rejected as invalid.";
        break;
      case 403:
        reason = "The claim request was forbidden.";
        break;
      case 409:
        reason = "This request conflicts with an existing claim or request key.";
        break;
      case 422:
        reason = "Turnstile verification was rejected.";
        break;
      case 429:
        reason = "The request was rate limited.";
        break;
      case 503:
        reason = "The faucet is unavailable or disabled.";
        break;
      default:
        reason = "The claim request failed.";
    }
    return `${reason} ${STOP_INSTRUCTION}`;
  }

  function canHandleVerification() {
    return !attemptStarted && !resettingVerification && isApprovedLocation();
  }

  function renderTurnstile() {
    if (!window.turnstile || turnstileWidgetId !== null || attemptStarted) return;
    if (!isApprovedLocation()) {
      setStatus(`Open this helper only at https://${APPROVED_HOSTNAME}.`, "error");
      updateControls();
      return;
    }

    try {
      turnstileWidgetId = window.turnstile.render(turnstileContainer, {
        sitekey: TURNSTILE_SITE_KEY,
        action: TURNSTILE_ACTION,
        // Keep the token out of Turnstile's default hidden form input.
        "response-field": false,
        callback(token) {
          if (!canHandleVerification()) return;
          verificationToken = typeof token === "string" && token.length > 0 ? token : null;
          setStatus(verificationToken ? "Verification complete. Ready for one operator-scoped test request." : "Verification failed.", verificationToken ? "success" : "error");
          updateControls();
        },
        "expired-callback"() {
          if (!canHandleVerification()) return;
          clearVerification(false);
          setStatus("Verification expired. Complete Turnstile again before submitting.", "warning");
        },
        "error-callback"() {
          if (!canHandleVerification()) return;
          clearVerification(false);
          setStatus("Turnstile could not load. Stop and check the staging host/network.", "error");
        },
      });
    } catch {
      clearVerification(false);
      setStatus("Turnstile could not load. Stop and check the staging host/network.", "error");
    }
  }

  function waitForTurnstile(attempt = 0) {
    if (attemptStarted) return;
    if (!isApprovedLocation()) {
      setStatus(`Open this helper only at https://${APPROVED_HOSTNAME}.`, "error");
      updateControls();
      return;
    }
    if (window.turnstile) {
      renderTurnstile();
      return;
    }
    if (attempt >= 50) {
      setStatus("Turnstile did not load. Stop and do not submit a claim.", "error");
      return;
    }
    window.setTimeout(() => waitForTurnstile(attempt + 1), 200);
  }

  function isRecord(value) {
    return value !== null && typeof value === "object" && !Array.isArray(value);
  }

  function validClaim(payload, address) {
    const claim = isRecord(payload) && payload.success === true ? payload.data : null;
    return isRecord(claim)
      && typeof claim.claim_id === "string" && CLAIM_ID_PATTERN.test(claim.claim_id)
      && claim.address === address && claim.status === "queued"
      && claim.amount_base_units === 1000000 && claim.amount_ultra === "1.000000"
      && claim.decimals === 6 && Number.isSafeInteger(claim.retry_after_seconds)
      && claim.retry_after_seconds >= 0;
  }

  function finishAttempt(nextPhase, message) {
    // An abort/timeout remains authoritative even if a promise settles later.
    if (phase !== "submitting") return;
    phase = nextPhase;
    verificationToken = null;
    setStatus(message, nextPhase === "accepted" ? "success" : "error");
    updateControls();
  }

  async function submitClaim(event) {
    event.preventDefault();
    if (attemptStarted || phase !== "idle") return;
    if (!isApprovedLocation()) {
      clearVerification(false);
      setStatus(`Open this helper only at https://${APPROVED_HOSTNAME}.`, "error");
      return;
    }

    const address = validateRecipient();
    if (!address || !verificationToken) {
      if (address && !verificationToken) setStatus("Complete Turnstile verification before submitting.", "warning");
      updateControls();
      return;
    }

    let controller;
    let idempotencyKey;
    let body;
    try {
      controller = new window.AbortController();
      idempotencyKey = makeIdempotencyKey();
      body = JSON.stringify({ address, captcha_token: verificationToken });
    } catch {
      clearVerification(false);
      setStatus("Required secure browser capabilities are unavailable. No request was sent. Stop and review the browser configuration.", "error");
      return;
    }

    attemptStarted = true;
    phase = "submitting";
    verificationToken = null;
    updateControls();
    setStatus("Submitting the single operator-scoped test request…");

    let timeoutId;
    try {
      timeoutId = window.setTimeout(() => {
        if (phase !== "submitting") return;
        finishAttempt("stopped", UNCERTAIN_MESSAGE);
        controller.abort();
      }, CLAIM_REQUEST_TIMEOUT_MS);

      const response = await fetch(CLAIM_ENDPOINT, {
        method: "POST",
        headers: {
          "Content-Type": "application/json",
          "Idempotency-Key": idempotencyKey,
        },
        body,
        credentials: "same-origin",
        cache: "no-store",
        mode: "same-origin",
        redirect: "error",
        signal: controller.signal,
      });
      if (phase !== "submitting") return;

      if (response.status !== 202) {
        finishAttempt("stopped", response.ok ? CONTRACT_MESSAGE : safeFailureMessage(response.status));
        return;
      }
      const mediaType = (response.headers.get("Content-Type") || "").split(";")[0].trim().toLowerCase();
      if (mediaType !== "application/json") {
        finishAttempt("stopped", CONTRACT_MESSAGE);
        return;
      }

      let payload;
      try {
        payload = await response.json();
      } catch {
        finishAttempt("stopped", CONTRACT_MESSAGE);
        return;
      }
      if (phase !== "submitting") return;
      if (!validClaim(payload, address)) {
        finishAttempt("stopped", CONTRACT_MESSAGE);
        return;
      }

      finishAttempt("accepted", `Claim accepted: ${payload.data.claim_id}. Acceptance is not payout confirmation. Follow the private operator monitoring and reconciliation procedure; do not submit again.`);
    } catch {
      finishAttempt("stopped", UNCERTAIN_MESSAGE);
    } finally {
      window.clearTimeout(timeoutId);
      verificationToken = null;
      updateControls();
    }
  }

  function resetForm(event) {
    if (attemptStarted) {
      event.preventDefault();
      return;
    }
    recipientInput.setAttribute("aria-invalid", "false");
    const resetSucceeded = clearVerification(true);
    setStatus(resetSucceeded
      ? "Complete Turnstile verification before submitting."
      : "Turnstile could not reset. Stop and check the staging host/network.", resetSucceeded ? "" : "error");
  }

  recipientInput.addEventListener("input", () => {
    if (attemptStarted) return;
    recipientInput.setAttribute(
      "aria-invalid",
      ADDRESS_PATTERN.test(recipientInput.value.trim()) ? "false" : "true",
    );
    updateControls();
  });
  form.addEventListener("submit", submitClaim);
  form.addEventListener("reset", resetForm);
  window.addEventListener("load", () => waitForTurnstile());
  window.addEventListener("beforeunload", () => {
    verificationToken = null;
    updateControls();
  });
  updateControls();
})();
